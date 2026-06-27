use crate::config;
use crate::http::HttpClient;
use crate::output::{self, ProviderPayload, ProviderIdentitySnapshot, RateWindow, UsageSnapshot};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};

const BASE_URL: &str = "https://app.devin.ai";

pub fn fetch(http: &HttpClient) -> ProviderPayload {
    match fetch_inner(http) {
        Ok(payload) => payload,
        Err(error) => ProviderPayload::error("devin", error.to_string()),
    }
}

fn fetch_inner(http: &HttpClient) -> Result<ProviderPayload> {
    let bearer_token = resolve_bearer_token()
        .ok_or_else(|| anyhow!("No Devin bearer token found. Set DEVIN_BEARER_TOKEN or DEVIN_AUTHORIZATION."))?;
    let organization = resolve_organization()
        .ok_or_else(|| anyhow!("No Devin organization found. Set DEVIN_ORGANIZATION or DEVIN_ORG."))?;

    let normalized = normalize_organization(&organization);
    let internal_id = internal_organization_id(&normalized);

    let headers = auth_headers(&bearer_token, internal_id.as_deref())?;

    let mut last_error: Option<String> = None;
    for path in candidate_paths(&normalized, internal_id.as_deref()) {
        let url = format!("{BASE_URL}/api/{path}");
        match http.fetch_json_value(&url, &headers) {
            Ok(value) => {
                let snapshot = parse_quota(&value, &normalized)?;
                return Ok(build_payload(snapshot));
            }
            Err(error) => {
                last_error = Some(error.to_string());
            }
        }
    }

    Err(anyhow!(
        "Devin API request failed: {}",
        last_error.unwrap_or_else(|| "No quota endpoint succeeded.".to_string())
    ))
}

fn resolve_bearer_token() -> Option<String> {
    for env_name in &["DEVIN_BEARER_TOKEN", "DEVIN_AUTHORIZATION"] {
        if let Ok(value) = std::env::var(env_name) {
            let token = strip_bearer_prefix(value.trim());
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }
    config::manual_cookie_header("devin")
}

fn resolve_organization() -> Option<String> {
    for env_name in &["DEVIN_ORGANIZATION", "DEVIN_ORG"] {
        if let Ok(value) = std::env::var(env_name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    config::devin_organization()
}

fn strip_bearer_prefix(raw: &str) -> &str {
    let lower = raw.to_lowercase();
    if lower.starts_with("authorization:") {
        let rest = &raw[15..].trim_start();
        return strip_bearer_prefix(rest);
    }
    if lower.starts_with("bearer ") {
        return &raw[7..].trim();
    }
    raw
}

fn auth_headers(bearer_token: &str, internal_id: Option<&str>) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {bearer_token}"))
            .map_err(|e| anyhow!("invalid bearer token: {e}"))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    if let Some(id) = internal_id {
        headers.insert(
            "x-cog-org-id",
            HeaderValue::from_str(id).map_err(|e| anyhow!("invalid org id header: {e}"))?,
        );
    }
    Ok(headers)
}

fn normalize_organization(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.starts_with("org/") || trimmed.starts_with("organizations/") {
        return trimmed.to_string();
    }
    if is_internal_organization_id(trimmed) {
        return format!("organizations/{trimmed}");
    }
    // Try extracting from URL
    if let Ok(url) = url::Url::parse(trimmed) {
        if let Some(host) = url.host_str() {
            let host = host.to_lowercase();
            if host == "devin.ai" || host.ends_with(".devin.ai") {
                let parts: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
                if parts.len() >= 2 {
                    if parts[0] == "org" {
                        return format!("org/{}", parts[1]);
                    }
                    if parts[0] == "organizations" {
                        return format!("organizations/{}", parts[1]);
                    }
                }
            }
        }
    }
    format!("org/{trimmed}")
}

fn is_internal_organization_id(value: &str) -> bool {
    value.starts_with("org-") || value.starts_with("org_")
}

fn internal_organization_id(normalized: &str) -> Option<String> {
    if normalized.starts_with("organizations/") {
        Some(normalized["organizations/".len()..].to_string())
    } else {
        None
    }
}

fn candidate_paths(normalized: &str, internal_id: Option<&str>) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(id) = internal_id {
        paths.push(format!("{id}/billing/quota/usage"));
    }
    paths.push(format!("{normalized}/billing/quota/usage"));
    if normalized.starts_with("org/") {
        let slug = &normalized[4..];
        paths.push(format!("{slug}/billing/quota/usage"));
    }
    if !normalized.starts_with("org/") && !normalized.starts_with("organizations/") {
        paths.push(format!("org/{normalized}/billing/quota/usage"));
    }
    if let Some(id) = internal_id {
        paths.push(format!("organizations/{id}/billing/quota/usage"));
    }
    paths.dedup();
    paths
}

#[derive(Debug)]
struct QuotaSnapshot {
    daily_used_percent: Option<f64>,
    daily_resets_at: Option<DateTime<Utc>>,
    weekly_used_percent: Option<f64>,
    weekly_resets_at: Option<DateTime<Utc>>,
    plan_name: Option<String>,
    organization: String,
}

fn parse_quota(value: &serde_json::Value, normalized_org: &str) -> Result<QuotaSnapshot> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow!("Devin API returned non-object response"))?;

    let daily_used_percent = extract_percent(obj, &["daily_percentage", "daily_percent", "daily"]);
    let daily_resets_at = extract_reset_at(obj, "daily");
    let weekly_used_percent = extract_percent(obj, &["weekly_percentage", "weekly_percent", "weekly"]);
    let weekly_resets_at = extract_reset_at(obj, "weekly");

    if daily_used_percent.is_none() && weekly_used_percent.is_none() {
        // Try nested quota windows
        let (d, w) = find_nested_windows(value);
        if d.is_none() && w.is_none() {
            return Err(anyhow!("Missing Devin quota windows in API response"));
        }
        return Ok(QuotaSnapshot {
            daily_used_percent: d.map(|(p, _)| p),
            daily_resets_at: d.and_then(|(_, r)| r),
            weekly_used_percent: w.map(|(p, _)| p),
            weekly_resets_at: w.and_then(|(_, r)| r),
            plan_name: extract_plan_name(value),
            organization: display_organization(normalized_org),
        });
    }

    Ok(QuotaSnapshot {
        daily_used_percent,
        daily_resets_at,
        weekly_used_percent,
        weekly_resets_at,
        plan_name: extract_plan_name(value),
        organization: display_organization(normalized_org),
    })
}

fn extract_percent(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if let Some(percent) = parse_percent_value(value) {
                return Some(percent);
            }
        }
    }
    None
}

fn parse_percent_value(value: &serde_json::Value) -> Option<f64> {
    let raw = value.as_f64()?;
    // Values ≤1 are fractions (0.0–1.0), convert to percentage
    let percent = if raw <= 1.0 { raw * 100.0 } else { raw };
    Some(percent.clamp(0.0, 100.0))
}

fn extract_reset_at(obj: &serde_json::Map<String, serde_json::Value>, prefix: &str) -> Option<DateTime<Utc>> {
    for (key, value) in obj {
        if key.to_lowercase().contains("reset") && key.to_lowercase().contains(prefix) {
            if let Some(date) = parse_date(value) {
                return Some(date);
            }
        }
    }
    None
}

fn parse_date(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    if let Some(raw) = value.as_str() {
        if let Ok(date) = DateTime::parse_from_rfc3339(raw) {
            return Some(date.with_timezone(&Utc));
        }
        if let Ok(number) = raw.parse::<f64>() {
            return parse_timestamp(number);
        }
    }
    if let Some(number) = value.as_f64() {
        return parse_timestamp(number);
    }
    None
}

fn parse_timestamp(number: f64) -> Option<DateTime<Utc>> {
    if number <= 0.0 {
        return None;
    }
    // Distinguish seconds from milliseconds
    let secs = if number > 10_000_000_000.0 {
        number / 1000.0
    } else {
        number
    };
    DateTime::from_timestamp(secs as i64, 0)
}

fn find_nested_windows(
    value: &serde_json::Value,
) -> (
    Option<(f64, Option<DateTime<Utc>>)>,
    Option<(f64, Option<DateTime<Utc>>)>,
) {
    let daily = find_window(value, "daily");
    let weekly = find_window(value, "weekly");
    (daily, weekly)
}

fn find_window(value: &serde_json::Value, key_fragment: &str) -> Option<(f64, Option<DateTime<Utc>>)> {
    if let Some(obj) = value.as_object() {
        for (key, val) in obj {
            if key.to_lowercase().contains(key_fragment) && !key.to_lowercase().contains("hide") {
                if let Some(percent) = parse_percent_value(val) {
                    return Some((percent, None));
                }
                if let Some(inner) = val.as_object() {
                    if let Some(percent) = extract_percent(inner, &["used_percent", "usedPercent", "percent", "usage_percent"]) {
                        let reset = inner.iter().find(|(k, _)| k.to_lowercase().contains("reset")).and_then(|(_, v)| parse_date(v));
                        return Some((percent, reset));
                    }
                }
            }
        }
        for val in obj.values() {
            if let Some(found) = find_window(val, key_fragment) {
                return Some(found);
            }
        }
    }
    if let Some(arr) = value.as_array() {
        for val in arr {
            if let Some(found) = find_window(val, key_fragment) {
                return Some(found);
            }
        }
    }
    None
}

fn extract_plan_name(value: &serde_json::Value) -> Option<String> {
    find_string_field(value, &["plan_name", "planName", "plan", "tier", "subscription_tier"])
}

fn find_string_field(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    if let Some(obj) = value.as_object() {
        for key in keys {
            if let Some(val) = obj.get(*key) {
                if let Some(s) = val.as_str() {
                    let trimmed = s.trim();
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
            }
        }
        for val in obj.values() {
            if let Some(found) = find_string_field(val, keys) {
                return Some(found);
            }
        }
    }
    if let Some(arr) = value.as_array() {
        for val in arr {
            if let Some(found) = find_string_field(val, keys) {
                return Some(found);
            }
        }
    }
    None
}

fn display_organization(normalized: &str) -> String {
    if normalized.starts_with("org/") {
        normalized[4..].to_string()
    } else if normalized.starts_with("organizations/") {
        normalized["organizations/".len()..].to_string()
    } else {
        normalized.to_string()
    }
}

fn build_payload(snapshot: QuotaSnapshot) -> ProviderPayload {
    let primary = snapshot.daily_used_percent.map(|used| RateWindow {
        used_percent: output::clamp_percent(used),
        remaining_percent: (100.0 - used).max(0.0),
        window_minutes: Some(24 * 60),
        resets_at: snapshot.daily_resets_at,
        reset_description: Some("Daily".to_string()),
    });

    let secondary = snapshot.weekly_used_percent.map(|used| RateWindow {
        used_percent: output::clamp_percent(used),
        remaining_percent: (100.0 - used).max(0.0),
        window_minutes: Some(7 * 24 * 60),
        resets_at: snapshot.weekly_resets_at,
        reset_description: Some("Weekly".to_string()),
    });

    let identity = ProviderIdentitySnapshot {
        account_email: None,
        account_organization: Some(snapshot.organization),
        login_method: snapshot.plan_name,
    };

    ProviderPayload::ok(
        "devin",
        UsageSnapshot {
            primary,
            secondary,
            tertiary: None,
            usage_rows: None,
            provider_cost: None,
            cursor_requests: None,
            updated_at: Utc::now(),
            identity: Some(identity),
        },
        None,
        None,
        None,
    )
}
