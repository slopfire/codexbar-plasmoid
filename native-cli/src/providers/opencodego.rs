use crate::config::workspace_id;
use crate::cookies::{resolve_all_cookie_headers, resolve_cookie_header};
use crate::http::{console_api_headers, HttpClient};
use crate::opencodego_local::can_read_local_usage;
use crate::output::{clamp_percent, rate_window, ProviderPayload, RateWindow, UsageSnapshot};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

const CONSOLE_BASE: &str = "https://opencode.ai/console/api";

#[derive(Debug, Deserialize)]
struct ConsoleOrg {
    id: String,
}

#[derive(Debug, Deserialize)]
struct GoStatus {
    #[serde(default)]
    access: Option<GoAccess>,
}

#[derive(Debug, Deserialize)]
struct GoAccess {
    #[serde(default)]
    meters: Option<GoMeters>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoMeters {
    #[serde(default)]
    five_hour: Option<GoMeter>,
    #[serde(default)]
    week: Option<GoMeter>,
    #[serde(default)]
    month: Option<GoMeter>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoMeter {
    #[serde(default)]
    limit_micro_cents: Option<String>,
    #[serde(default)]
    used_micro_cents: Option<String>,
    #[serde(default)]
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MembersResponse {
    #[serde(default)]
    items: Vec<MemberItem>,
}

#[derive(Debug, Deserialize)]
struct MemberItem {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    role: Option<String>,
}

/// Fetch OpenCode Go **subscription** usage, discovering every signed-in account
/// across browser cookie stores. Returns one payload per account so the plasmoid
/// can render each account (with its email) separately.
///
/// Rate-limit meters (5-hour / weekly / monthly) only come from the live Go
/// plan API (`/console/api/go/status`, scoped per organisation via `x-org-id`).
/// Local OpenCode SQLite history is pay-as-you-go spend and must not be
/// mapped onto the subscription windows — that produced full "100%"
/// remaining bars for users with an API key but no Go plan. Local token spend is
/// still surfaced via the separate `cost` command.
pub fn fetch(http: &HttpClient, home: &Path) -> Vec<ProviderPayload> {
    let cookies = resolve_all_cookie_headers("opencodego");
    if !cookies.is_empty() {
        // A single manual config cookie may carry a configured workspace id;
        // auto-discovered browser sessions always resolve their own orgs.
        let use_config_workspace = cookies.len() == 1 && cookies[0].source == "config";
        let mut successes = Vec::new();
        let mut first_error: Option<ProviderPayload> = None;
        for cookie in &cookies {
            match fetch_all_orgs(http, &cookie.header, &cookie.source, use_config_workspace) {
                Ok(payloads) => successes.extend(payloads),
                Err(error) => {
                    if first_error.is_none() {
                        let mut payload =
                            ProviderPayload::error("opencodego", friendly_error(&error));
                        payload.source = cookie.source.clone();
                        first_error = Some(payload);
                    }
                }
            }
        }
        let successes = deduplicate_accounts(successes);
        if !successes.is_empty() {
            return successes;
        }
        if let Some(payload) = first_error {
            // Prefer the real web failure (session expired, …) over inventing
            // subscription bars from local API spend.
            return vec![payload];
        }
        // Signed in, but no organisation has an active Go subscription.
        let mut payload = ProviderPayload::error(
            "opencodego",
            "OpenCode Go plan not enabled for this account. Local API spend still shows under cost.",
        );
        payload.source = cookies[0].source.clone();
        return vec![payload];
    }

    // No browser session → no subscription rate limits. The plasmoid helper
    // still attaches local `opencodego` cost when spend exists in SQLite.
    if can_read_local_usage(home) {
        return vec![ProviderPayload::error(
            "opencodego",
            "No OpenCode Go subscription session found. Rate limits need an active Go plan (sign in at opencode.ai). Local token spend still shows when cost is enabled.",
        )];
    }

    vec![ProviderPayload::error(
        "opencodego",
        resolve_cookie_header("opencodego")
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "OpenCode Go not detected.".to_string()),
    )]
}

/// Browser discovery can find the same signed-in account in several cookie
/// stores. Keep one payload per account (or workspace when the email is absent)
/// so a Chrome + Zen login does not create duplicate provider cards.
fn deduplicate_accounts(payloads: Vec<ProviderPayload>) -> Vec<ProviderPayload> {
    let mut seen = HashSet::new();
    payloads
        .into_iter()
        .filter(|payload| {
            let key = payload
                .account
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| format!("account:{}", value.to_lowercase()))
                .or_else(|| {
                    payload
                        .site_url
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(|value| format!("workspace:{}", value.to_lowercase()))
                })
                .unwrap_or_else(|| format!("source:{}", payload.source.to_lowercase()));
            seen.insert(key)
        })
        .collect()
}

fn fetch_all_orgs(
    http: &HttpClient,
    cookie_header: &str,
    source_label: &str,
    use_config_workspace: bool,
) -> Result<Vec<ProviderPayload>> {
    let mut org_ids = fetch_org_ids(http, cookie_header)?;
    if use_config_workspace {
        // A pinned `wrk_…` id predates the console org model; try it first and
        // fall back to discovered orgs when the backend rejects it.
        if let Some(pinned) = workspace_id("opencodego") {
            if !org_ids.contains(&pinned) {
                org_ids.insert(0, pinned);
            }
        }
    }
    let mut payloads = Vec::new();
    for org_id in &org_ids {
        match fetch_one_org(http, cookie_header, source_label, org_id) {
            Ok(Some(payload)) => payloads.push(payload),
            // Organisation without Go access (pay-as-you-go): not an error,
            // just no subscription bars for this org.
            Ok(None) => {}
            Err(error) if is_auth_error(&error) => return Err(error),
            // One org failing must not hide a sibling org with Go access.
            Err(_) => {}
        }
    }
    Ok(payloads)
}

fn fetch_org_ids(http: &HttpClient, cookie_header: &str) -> Result<Vec<String>> {
    let headers = console_api_headers(cookie_header, None)?;
    let text = http
        .fetch_text(&format!("{CONSOLE_BASE}/orgs"), &headers)
        .map_err(|error| map_console_error(error))?;
    if looks_signed_out(&text) {
        return Err(anyhow!("OpenCode session cookie is invalid or expired."));
    }
    let orgs: Vec<ConsoleOrg> = serde_json::from_str(&text)
        .map_err(|_| anyhow!("Missing OpenCode workspace id."))?;
    Ok(orgs.into_iter().map(|org| org.id).collect())
}

fn fetch_one_org(
    http: &HttpClient,
    cookie_header: &str,
    source_label: &str,
    org_id: &str,
) -> Result<Option<ProviderPayload>> {
    let headers = console_api_headers(cookie_header, Some(org_id))?;
    let text = http
        .fetch_text(&format!("{CONSOLE_BASE}/go/status"), &headers)
        .map_err(|error| map_console_error(error))?;
    if looks_signed_out(&text) {
        return Err(anyhow!("OpenCode session cookie is invalid or expired."));
    }
    let status: GoStatus =
        serde_json::from_str(&text).map_err(|_| anyhow!("Missing OpenCode Go usage fields."))?;
    let meters = status.access.as_ref().and_then(|access| access.meters.as_ref());
    let Some(meters) = meters else {
        return Ok(None);
    };
    let primary = meter_window(meters.five_hour.as_ref(), 5 * 60);
    let secondary = meter_window(meters.week.as_ref(), 7 * 24 * 60);
    let tertiary = meter_window(meters.month.as_ref(), 30 * 24 * 60);
    if primary.is_none() && secondary.is_none() && tertiary.is_none() {
        return Err(anyhow!("Missing OpenCode Go usage fields."));
    }

    let account = fetch_org_email(http, cookie_header, org_id);
    let mut payload = ProviderPayload::ok(
        "opencodego",
        UsageSnapshot {
            primary,
            secondary,
            tertiary,
            usage_rows: None,
            provider_cost: None,
            cursor_requests: None,
            updated_at: Utc::now(),
            identity: None,
        },
        account,
        None,
        None,
    );
    payload.source = source_label.to_string();
    payload.site_url = Some(format!("https://opencode.ai/console/{org_id}/go"));
    Ok(Some(payload))
}

/// Best-effort account email for an org; a missing address must not fail the
/// usage fetch.
fn fetch_org_email(http: &HttpClient, cookie_header: &str, org_id: &str) -> Option<String> {
    let headers = console_api_headers(cookie_header, Some(org_id)).ok()?;
    let text = http
        .fetch_text(&format!("{CONSOLE_BASE}/members"), &headers)
        .ok()?;
    let members: MembersResponse = serde_json::from_str(&text).ok()?;
    members
        .items
        .iter()
        .find(|item| item.role.as_deref() == Some("owner"))
        .or_else(|| members.items.first())
        .and_then(|item| item.email.clone())
}

/// A meter reports spend against a limit in micro-cents; convert to a
/// remaining-percent rate window.
fn meter_window(meter: Option<&GoMeter>, window_minutes: i64) -> Option<RateWindow> {
    let meter = meter?;
    let limit: f64 = meter.limit_micro_cents.as_deref()?.trim().parse().ok()?;
    let used: f64 = meter.used_micro_cents.as_deref()?.trim().parse().ok()?;
    if !(limit > 0.0) || used < 0.0 {
        return None;
    }
    let resets_at = meter
        .resets_at
        .as_deref()
        .and_then(|value| value.parse::<DateTime<Utc>>().ok());
    Some(rate_window(
        clamp_percent(used / limit * 100.0),
        Some(window_minutes),
        resets_at,
    ))
}

fn is_auth_error(error: &anyhow::Error) -> bool {
    let lower = error.to_string().to_lowercase();
    lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("not authenticated")
        || lower.contains("session cookie is invalid")
}

/// Map raw reqwest/anyhow errors to short, user-readable messages so the
/// plasmoid doesn't leak `POST https://.../status` style strings.
fn friendly_error(error: &anyhow::Error) -> String {
    let raw = error.to_string();
    let lower = raw.to_lowercase();
    let trimmed = raw.trim();

    if lower.contains("missing opencode go usage fields")
        || lower.contains("no subscription usage data")
        || lower.contains("go plan not enabled")
    {
        return "OpenCode Go plan not enabled for this account. Local API spend still shows under cost.".to_string();
    }
    if lower.contains("opencode session cookie is invalid")
        || lower.contains("session cookie is invalid or expired")
        || lower.contains("not authenticated")
    {
        return "Session expired. Re-login to OpenCode Go in this browser.".to_string();
    }
    if lower.contains("opencode workspace id") {
        return "No OpenCode workspace found for this account.".to_string();
    }
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
    {
        return "Session expired. Re-login to OpenCode Go in this browser.".to_string();
    }
    if lower.contains("404") || lower.contains("not found") {
        return "OpenCode Go not available for this account.".to_string();
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return "Request timed out.".to_string();
    }
    if lower.contains("connection refused")
        || lower.contains("dns")
        || lower.contains("network")
        || lower.contains("unreachable")
    {
        return "Connection failed.".to_string();
    }
    if lower.contains("post https://") || lower.contains("status server error") {
        return "OpenCode Go is unavailable. Try again later.".to_string();
    }
    if trimmed.is_empty() {
        return "OpenCode Go fetch failed.".to_string();
    }
    trimmed.to_string()
}

fn map_console_error(error: anyhow::Error) -> anyhow::Error {
    // `fetch_text` bakes the URL into GET/POST context strings; keep them but
    // surface auth failures with the canonical message.
    let lower = format!("{error:#}").to_lowercase();
    if lower.contains("401") || lower.contains("403") {
        return anyhow!("OpenCode session cookie is invalid or expired.");
    }
    error
}

fn looks_signed_out(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "login",
        "sign in",
        "auth/authorize",
        "not associated with an account",
        "actor of type \"public\"",
        "not authenticated",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::{deduplicate_accounts, meter_window};
    use crate::output::ProviderPayload;

    fn payload(account: Option<&str>, source: &str, site_url: Option<&str>) -> ProviderPayload {
        let mut payload = ProviderPayload::error("opencodego", "unused");
        payload.error = None;
        payload.account = account.map(str::to_string);
        payload.source = source.to_string();
        payload.site_url = site_url.map(str::to_string);
        payload
    }

    #[test]
    fn deduplicates_same_account_across_browser_profiles() {
        let entries = deduplicate_accounts(vec![
            payload(Some("user@example.com"), "Chrome (Default)", None),
            payload(Some("USER@example.com"), "Zen (profile)", None),
        ]);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, "Chrome (Default)");
    }

    #[test]
    fn keeps_distinct_accounts() {
        let entries = deduplicate_accounts(vec![
            payload(Some("one@example.com"), "Chrome (Default)", None),
            payload(Some("two@example.com"), "Zen (profile)", None),
        ]);

        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn falls_back_to_workspace_when_account_is_missing() {
        let entries = deduplicate_accounts(vec![
            payload(
                None,
                "Chrome (Default)",
                Some("https://opencode.ai/workspace/wrk_1/go"),
            ),
            payload(
                None,
                "Zen (profile)",
                Some("https://opencode.ai/workspace/wrk_1/go"),
            ),
        ]);

        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn meter_window_reports_remaining_percent() {
        let meter = super::GoMeter {
            limit_micro_cents: Some("6000000000".to_string()),
            used_micro_cents: Some("2603968926".to_string()),
            resets_at: Some("2026-11-04T06:05:05.000Z".to_string()),
        };

        let window = meter_window(Some(&meter), 30 * 24 * 60).expect("window");
        assert!((window.used_percent - 43.3994821).abs() < 0.001);
        assert!((window.remaining_percent - 56.6005179).abs() < 0.001);
        assert_eq!(window.window_minutes, Some(30 * 24 * 60));
        assert!(window.resets_at.is_some());
    }

    #[test]
    fn meter_window_rejects_missing_or_invalid_limits() {
        let missing = super::GoMeter {
            limit_micro_cents: None,
            used_micro_cents: Some("10".to_string()),
            resets_at: None,
        };
        assert!(meter_window(Some(&missing), 300).is_none());

        let zero = super::GoMeter {
            limit_micro_cents: Some("0".to_string()),
            used_micro_cents: Some("0".to_string()),
            resets_at: None,
        };
        assert!(meter_window(Some(&zero), 300).is_none());
    }

    #[test]
    fn meter_window_tolerates_missing_reset_time() {
        let meter = super::GoMeter {
            limit_micro_cents: Some("1200000000".to_string()),
            used_micro_cents: Some("0".to_string()),
            resets_at: None,
        };

        let window = meter_window(Some(&meter), 300).expect("window");
        assert_eq!(window.remaining_percent, 100.0);
        assert_eq!(window.resets_at, None);
    }
}
