use crate::cookies::resolve_cookie_header;
use crate::http::{cookie_header, HttpClient};
use crate::output::{
    clamp_percent, rate_window, CreditsSnapshot, CursorRequestUsage, ProviderCostSnapshot,
    ProviderIdentitySnapshot, ProviderPayload, ProviderStatusPayload, UsageSnapshot,
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;
pub fn fetch(http: &HttpClient, include_status: bool) -> ProviderPayload {
    match fetch_inner(http, include_status) {
        Ok(payload) => payload,
        Err(error) => ProviderPayload::error("cursor", error.to_string()),
    }
}

fn fetch_inner(http: &HttpClient, include_status: bool) -> Result<ProviderPayload> {
    let cookie = resolve_cookie_header("cursor")?;
    let headers = cookie_header(&cookie.header)?;

    let usage_summary: CursorUsageSummary = serde_json::from_value(http.fetch_json_value(
        "https://cursor.com/api/usage-summary",
        &headers,
    )?)?;
    let user_info: Option<CursorUserInfo> = http
        .fetch_json_value("https://cursor.com/api/auth/me", &headers)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok());

    let request_usage = user_info
        .as_ref()
        .and_then(|info| info.sub.as_ref())
        .and_then(|user_id| {
            http.fetch_json_value(
                &format!("https://cursor.com/api/usage?user={}", urlencoding(user_id)),
                &headers,
            )
            .ok()
            .and_then(|value| serde_json::from_value::<CursorUsageResponse>(value).ok())
        });

    let usage = cursor_usage_snapshot(&usage_summary, request_usage.as_ref());
    let status = if include_status {
        fetch_status(http).ok()
    } else {
        None
    };

    Ok(ProviderPayload::ok(
        "cursor",
        UsageSnapshot {
            primary: Some(usage.primary),
            secondary: usage.secondary,
            tertiary: usage.tertiary,
            provider_cost: usage.provider_cost,
            cursor_requests: usage.cursor_requests,
            updated_at: Utc::now(),
            identity: Some(ProviderIdentitySnapshot {
                account_email: user_info.as_ref().and_then(|info| info.email.clone()),
                account_organization: None,
                login_method: usage_summary
                    .membership_type
                    .as_deref()
                    .map(format_membership),
            }),
        },
        user_info.as_ref().and_then(|info| info.email.clone()),
        cursor_credits_snapshot(&usage_summary),
        status,
    ))
}

struct CursorUsageParts {
    primary: crate::output::RateWindow,
    secondary: Option<crate::output::RateWindow>,
    tertiary: Option<crate::output::RateWindow>,
    provider_cost: Option<ProviderCostSnapshot>,
    cursor_requests: Option<CursorRequestUsage>,
}

fn cursor_usage_snapshot(
    summary: &CursorUsageSummary,
    request_usage: Option<&CursorUsageResponse>,
) -> CursorUsageParts {
    let billing_cycle_end = summary
        .billing_cycle_end
        .as_deref()
        .and_then(parse_iso_date);
    let billing_cycle_start = summary
        .billing_cycle_start
        .as_deref()
        .and_then(parse_iso_date);
    let window_minutes = match (billing_cycle_start, billing_cycle_end) {
        (Some(start), Some(end)) => {
            let minutes = ((end - start).num_minutes()).max(1);
            Some(minutes)
        }
        _ => None,
    };

    let plan = summary.individual_usage.as_ref().and_then(|usage| usage.plan.as_ref());
    let overall = summary
        .individual_usage
        .as_ref()
        .and_then(|usage| usage.overall.as_ref());
    let pooled = summary.team_usage.as_ref().and_then(|usage| usage.pooled.as_ref());

    let plan_used_raw = plan.and_then(|p| p.used).unwrap_or(0) as f64;
    let plan_limit_raw = plan.and_then(|p| p.limit).unwrap_or(0) as f64;
    let auto_percent = plan.and_then(|p| p.auto_percent_used).map(clamp_percent);
    let api_percent = plan.and_then(|p| p.api_percent_used).map(clamp_percent);

    let plan_percent_used = if let Some(total) = plan.and_then(|p| p.total_percent_used) {
        clamp_percent(total)
    } else if let (Some(auto), Some(api)) = (auto_percent, api_percent) {
        clamp_percent((auto + api) / 2.0)
    } else if let Some(api) = api_percent {
        api
    } else if let Some(auto) = auto_percent {
        auto
    } else if plan_limit_raw > 0.0 {
        clamp_percent((plan_used_raw / plan_limit_raw) * 100.0)
    } else if let (Some(used), Some(limit)) = (overall.and_then(|o| o.used), overall.and_then(|o| o.limit)) {
        if limit > 0 {
            clamp_percent((used as f64 / limit as f64) * 100.0)
        } else {
            0.0
        }
    } else if let (Some(used), Some(limit)) = (pooled.and_then(|o| o.used), pooled.and_then(|o| o.limit)) {
        if limit > 0 {
            clamp_percent((used as f64 / limit as f64) * 100.0)
        } else {
            0.0
        }
    } else {
        0.0
    };

    let is_legacy_request_plan = request_usage
        .and_then(|usage| usage.gpt4.as_ref())
        .and_then(|gpt4| gpt4.max_request_usage)
        .is_some();

    let has_token_plan_data = plan
        .and_then(|p| p.total_percent_used)
        .is_some()
        || auto_percent.is_some()
        || api_percent.is_some();

    let primary_used = if is_legacy_request_plan && !has_token_plan_data {
        request_usage
            .and_then(|usage| usage.gpt4.as_ref())
            .and_then(|gpt4| {
                let used = gpt4.num_requests_total.or(gpt4.num_requests)?;
                let limit = gpt4.max_request_usage?;
                if limit > 0 {
                    Some(clamp_percent((used as f64 / limit as f64) * 100.0))
                } else {
                    None
                }
            })
            .unwrap_or(plan_percent_used)
    } else {
        plan_percent_used
    };

    let cursor_requests = request_usage.and_then(|usage| usage.gpt4.as_ref()).and_then(|gpt4| {
        gpt4.max_request_usage.map(|limit| CursorRequestUsage {
            used: gpt4.num_requests_total.or(gpt4.num_requests).unwrap_or(0) as i64,
            limit: limit as i64,
        })
    });

    CursorUsageParts {
        primary: rate_window(primary_used, window_minutes, billing_cycle_end),
        secondary: auto_percent.map(|value| rate_window(value, window_minutes, billing_cycle_end)),
        tertiary: api_percent.map(|value| rate_window(value, window_minutes, billing_cycle_end)),
        provider_cost: cursor_provider_cost(summary),
        cursor_requests,
    }
}

fn cursor_credits_snapshot(summary: &CursorUsageSummary) -> Option<CreditsSnapshot> {
    let on_demand = summary.individual_usage.as_ref()?.on_demand.as_ref()?;
    let used = on_demand.used.unwrap_or(0) as f64 / 100.0;
    let limit = on_demand.limit.map(|value| value as f64 / 100.0);
    let remaining = on_demand.remaining.map(|value| value as f64 / 100.0);
    if used <= 0.0 && limit.unwrap_or(0.0) <= 0.0 {
        return None;
    }
    Some(CreditsSnapshot {
        remaining,
        used,
        limit,
        updated_at: Utc::now(),
    })
}

fn cursor_provider_cost(summary: &CursorUsageSummary) -> Option<ProviderCostSnapshot> {
    let on_demand = summary.individual_usage.as_ref()?.on_demand.as_ref()?;
    let used = on_demand.used.unwrap_or(0) as f64 / 100.0;
    let limit = on_demand.limit.map(|value| value as f64 / 100.0).unwrap_or(0.0);
    if used <= 0.0 && limit <= 0.0 {
        return None;
    }
    Some(ProviderCostSnapshot {
        used,
        limit,
        currency_code: "USD".to_string(),
        period: "Monthly".to_string(),
        resets_at: summary
            .billing_cycle_end
            .as_deref()
            .and_then(parse_iso_date),
        updated_at: Utc::now(),
    })
}

fn fetch_status(http: &HttpClient) -> Result<ProviderStatusPayload> {
    let payload = http.fetch_json_value(
        "https://status.cursor.com/api/v2/status.json",
        &reqwest::header::HeaderMap::new(),
    )?;
    Ok(ProviderStatusPayload {
        indicator: map_status_indicator(
            payload
                .pointer("/status/indicator")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown"),
        ),
        description: payload
            .pointer("/status/description")
            .and_then(|value| value.as_str())
            .unwrap_or("Operational")
            .to_string(),
        updated_at: payload
            .pointer("/page/updated_at")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        url: "https://status.cursor.com".to_string(),
    })
}

fn map_status_indicator(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "none" => "none",
        "minor" => "minor",
        "major" => "major",
        "critical" => "critical",
        "maintenance" => "maintenance",
        _ => "unknown",
    }
    .to_string()
}

fn format_membership(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "enterprise" => "Cursor Enterprise".to_string(),
        "pro" => "Cursor Pro".to_string(),
        "hobby" => "Cursor Hobby".to_string(),
        "team" => "Cursor Team".to_string(),
        _ => format!(
            "Cursor {}{}",
            value.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or_default(),
            &value[1..]
        ),
    }
}

fn parse_iso_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            value
                .parse::<DateTime<Utc>>()
                .ok()
        })
}

fn urlencoding(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || "-_.~".contains(ch) {
                ch.to_string()
            } else {
                format!("%{:02X}", ch as u8)
            }
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorUsageSummary {
    billing_cycle_start: Option<String>,
    billing_cycle_end: Option<String>,
    membership_type: Option<String>,
    individual_usage: Option<CursorIndividualUsage>,
    team_usage: Option<CursorTeamUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorIndividualUsage {
    plan: Option<CursorPlanUsage>,
    on_demand: Option<CursorOnDemandUsage>,
    overall: Option<CursorOverallUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorTeamUsage {
    pooled: Option<CursorOverallUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorPlanUsage {
    used: Option<i64>,
    limit: Option<i64>,
    auto_percent_used: Option<f64>,
    api_percent_used: Option<f64>,
    total_percent_used: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorOnDemandUsage {
    used: Option<i64>,
    limit: Option<i64>,
    remaining: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorOverallUsage {
    used: Option<i64>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorUserInfo {
    email: Option<String>,
    sub: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorUsageResponse {
    #[serde(rename = "gpt-4")]
    gpt4: Option<CursorModelUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorModelUsage {
    num_requests: Option<i64>,
    num_requests_total: Option<i64>,
    max_request_usage: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_camel_case_usage_summary_lanes() {
        let summary: CursorUsageSummary = serde_json::from_str(
            r#"{
              "membershipType": "pro",
              "billingCycleEnd": "2026-07-10T00:00:00.000Z",
              "individualUsage": {
                "plan": {
                  "used": 0,
                  "limit": 2000,
                  "autoPercentUsed": 91,
                  "apiPercentUsed": 100,
                  "totalPercentUsed": 96
                }
              }
            }"#,
        )
        .expect("usage summary JSON should deserialize");

        let usage = cursor_usage_snapshot(&summary, None);
        assert_eq!(usage.primary.used_percent, 96.0);
        assert_eq!(usage.secondary.as_ref().map(|w| w.used_percent), Some(91.0));
        assert_eq!(usage.tertiary.as_ref().map(|w| w.used_percent), Some(100.0));
    }

    #[test]
    fn token_plan_primary_is_not_overwritten_by_legacy_requests() {
        let summary: CursorUsageSummary = serde_json::from_str(
            r#"{
              "individualUsage": {
                "plan": {
                  "totalPercentUsed": 96,
                  "autoPercentUsed": 91,
                  "apiPercentUsed": 100
                }
              }
            }"#,
        )
        .expect("usage summary JSON should deserialize");
        let request_usage: CursorUsageResponse = serde_json::from_str(
            r#"{
              "gpt-4": {
                "numRequestsTotal": 500,
                "maxRequestUsage": 500
              }
            }"#,
        )
        .expect("legacy usage JSON should deserialize");

        let usage = cursor_usage_snapshot(&summary, Some(&request_usage));
        assert_eq!(usage.primary.used_percent, 96.0);
        assert_eq!(usage.secondary.as_ref().map(|w| w.used_percent), Some(91.0));
        assert_eq!(usage.tertiary.as_ref().map(|w| w.used_percent), Some(100.0));
    }

    #[test]
    fn legacy_request_plan_uses_request_ratio_for_primary() {
        let summary: CursorUsageSummary = serde_json::from_str(r#"{ "individualUsage": {} }"#)
            .expect("usage summary JSON should deserialize");
        let request_usage: CursorUsageResponse = serde_json::from_str(
            r#"{
              "gpt-4": {
                "numRequestsTotal": 250,
                "maxRequestUsage": 500
              }
            }"#,
        )
        .expect("legacy usage JSON should deserialize");

        let usage = cursor_usage_snapshot(&summary, Some(&request_usage));
        assert_eq!(usage.primary.used_percent, 50.0);
        assert!(usage.secondary.is_none());
        assert!(usage.tertiary.is_none());
    }
}
