//! OpenCode subscription meters from the unified console API.
//!
//! OpenCode replaced the SolidStart `/workspace/*` pages and the `/_server`
//! server functions with a console SPA at `opencode.ai/console/<workspace>/go`,
//! authenticated by the `__Host-console_session` cookie and backed by a small
//! HTTP API:
//!
//!   GET /console/api/orgs       — organisations (workspaces) for the session
//!   GET /console/api/go/status  — Go rate-limit meters (`fiveHour`/`week`/`month`)
//!   GET /console/api/members    — member emails, used for the account label
//!
//! Org-scoped routes need the workspace id in `x-org-id` (`org_required`
//! otherwise); an invalid session answers 401/403. Each meter reports spend in
//! micro-cents against a per-window limit, which maps onto the plasmoid's
//! primary (5-hour) / secondary (weekly) / tertiary (monthly) bars.
//!
//! `opencode` and `opencodego` render the same meters; the former keeps its
//! two-bar contract (rolling + weekly). Local OpenCode SQLite history is
//! pay-as-you-go spend and never maps onto subscription bars — that produced
//! full "100%" remaining bars for users with an API key but no Go plan. Local
//! token spend still surfaces through the separate `cost` command.

use crate::config::workspace_id;
use crate::cookies::{
    opencode_workspace_ids_from_browsers, resolve_all_cookie_headers, resolve_cookie_header,
};
use crate::http::{console_headers, HttpClient};
use crate::opencodego_local::can_read_local_usage;
use crate::output::{clamp_percent, rate_window, ProviderPayload, RateWindow, UsageSnapshot};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

const CONSOLE_API_BASE: &str = "https://opencode.ai/console/api";
const CONSOLE_BASE: &str = "https://opencode.ai/console";

/// Window lengths used only when the API reports neither a meter `startsAt`
/// nor an access period to measure. Deriving a window from `resetsAt - now`
/// instead would shrink it to the remaining time and make the pace line
/// report a freshly reset window as wildly over pace.
const NOMINAL_FIVE_HOUR_MINUTES: i64 = 5 * 60;
const NOMINAL_WEEK_MINUTES: i64 = 7 * 24 * 60;
const NOMINAL_MONTH_MINUTES: i64 = 30 * 24 * 60;

const MISSING_WORKSPACE_MESSAGE: &str = "OpenCode workspace id not found. Open opencode.ai/console in a browser once (or set CODEXBAR_OPENCODE_GO_WORKSPACE_ID / CODEXBAR_OPENCODE_WORKSPACE_ID), so the widget can read the current workspace.";
const NO_SESSION_MESSAGE: &str = "No OpenCode console session (__Host-console_session cookie) found. Sign in at opencode.ai/console, then retry.";
const NO_PLAN_MESSAGE: &str =
    "OpenCode Go plan not enabled for this account. Local API spend still shows under cost.";
const EXPIRED_SESSION_MESSAGE: &str = "Session expired. Re-login to OpenCode Go in this browser.";

#[derive(Debug, Deserialize)]
struct ConsoleOrg {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoStatus {
    #[serde(default)]
    subscriber_user_id: Option<String>,
    #[serde(default)]
    access: Option<GoAccess>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoAccess {
    #[serde(default)]
    starts_at: Option<DateTime<Utc>>,
    #[serde(default)]
    ends_at: Option<DateTime<Utc>>,
    #[serde(default)]
    meters: Option<GoMeters>,
}

impl GoAccess {
    /// Billing period in minutes, used as the month window when that meter
    /// omits its own `startsAt`.
    fn period_minutes(&self) -> Option<i64> {
        let minutes = (self.ends_at? - self.starts_at?).num_minutes();
        (minutes > 0).then_some(minutes)
    }
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
    starts_at: Option<DateTime<Utc>>,
    #[serde(default)]
    resets_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "de_micro_cents")]
    limit_micro_cents: Option<String>,
    #[serde(default, deserialize_with = "de_micro_cents")]
    used_micro_cents: Option<String>,
}

impl GoMeter {
    /// Measured window length when the backend reports both ends of it.
    fn window_minutes(&self) -> Option<i64> {
        let minutes = (self.resets_at? - self.starts_at?).num_minutes();
        (minutes > 0).then_some(minutes)
    }
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

/// Micro-cent fields are JSON strings today; accept numbers as well so a
/// backend change degrades to a parsed value instead of a dropped bar.
fn de_micro_cents<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value =
        Option::<serde_json::Value>::deserialize(deserializer).map_err(serde::de::Error::custom)?;
    Ok(match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => Some(text),
        Some(other) => Some(other.to_string()),
    })
}

fn micro_cents(value: Option<&str>) -> Option<f64> {
    let parsed = value?.trim().parse::<f64>().ok()?;
    parsed.is_finite().then_some(parsed)
}

/// Fetch subscription usage for `opencodego` (`include_monthly`) or `opencode`,
/// discovering every signed-in account across browser cookie stores. Returns one
/// payload per account so the plasmoid can render each account separately.
pub fn fetch(
    http: &HttpClient,
    provider: &str,
    include_monthly: bool,
    home: &Path,
) -> Vec<ProviderPayload> {
    let cookies = resolve_all_cookie_headers(provider);
    if !cookies.is_empty() {
        let mut successes = Vec::new();
        let mut first_error: Option<ProviderPayload> = None;
        for cookie in &cookies {
            let Some(session) = console_session_value(&cookie.header) else {
                if first_error.is_none() {
                    let mut payload = ProviderPayload::error(provider, NO_SESSION_MESSAGE);
                    payload.source = cookie.source.clone();
                    first_error = Some(payload);
                }
                continue;
            };
            match fetch_all_orgs(http, &session, provider, include_monthly, &cookie.source) {
                Ok(payloads) => successes.extend(payloads),
                Err(error) => {
                    if first_error.is_none() {
                        let mut payload = ProviderPayload::error(provider, friendly_error(&error));
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
        let mut payload = ProviderPayload::error(provider, NO_PLAN_MESSAGE);
        payload.source = cookies[0].source.clone();
        return vec![payload];
    }

    // No browser session → no subscription rate limits. The plasmoid helper
    // still attaches local `opencodego` cost when spend exists in SQLite.
    if provider == "opencodego" && can_read_local_usage(home) {
        return vec![ProviderPayload::error(
            provider,
            "No OpenCode Go subscription session found. Rate limits need an active Go plan (sign in at opencode.ai). Local token spend still shows when cost is enabled.",
        )];
    }

    vec![ProviderPayload::error(
        provider,
        resolve_cookie_header(provider)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "OpenCode session not detected.".to_string()),
    )]
}

/// The console session value from a provider cookie header. Only this cookie is
/// forwarded to `/console/api/*`; the legacy sealed `auth` cookie stays local.
fn console_session_value(header: &str) -> Option<String> {
    let mut fallback = None;
    for part in header.split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match name.trim() {
            "__Host-console_session" => return Some(value.to_string()),
            "console_session" => fallback = Some(value.to_string()),
            _ => {}
        }
    }
    fallback
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
    session: &str,
    provider: &str,
    include_monthly: bool,
    source_label: &str,
) -> Result<Vec<ProviderPayload>> {
    let org_ids = resolve_org_ids(http, session)?;
    if org_ids.is_empty() {
        return Err(anyhow!(MISSING_WORKSPACE_MESSAGE));
    }
    let mut payloads = Vec::new();
    for org_id in &org_ids {
        match fetch_one_org(http, session, provider, include_monthly, source_label, org_id) {
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

/// Workspaces to try: the id pinned in config/env first, then the orgs the
/// console reports for this session, then the ids the console persisted in
/// browser localStorage (which still works when `/orgs` is unavailable).
fn resolve_org_ids(http: &HttpClient, session: &str) -> Result<Vec<String>> {
    match fetch_org_ids(http, session) {
        Ok(ids) if !ids.is_empty() => Ok(merge_ids(pinned_workspace_id(), ids)),
        Ok(_) => Ok(fallback_org_ids()),
        Err(error) if is_auth_error(&error) => Err(error),
        Err(_) => Ok(fallback_org_ids()),
    }
}

fn pinned_workspace_id() -> Option<String> {
    workspace_id("opencodego").or_else(|| workspace_id("opencode"))
}

fn fallback_org_ids() -> Vec<String> {
    merge_ids(
        pinned_workspace_id(),
        opencode_workspace_ids_from_browsers(),
    )
}

fn merge_ids(head: Option<String>, rest: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for id in head.into_iter().chain(rest) {
        let id = id.trim().to_string();
        if !id.is_empty() && seen.insert(id.clone()) {
            out.push(id);
        }
    }
    out
}

fn fetch_org_ids(http: &HttpClient, session: &str) -> Result<Vec<String>> {
    let (status, body) = console_get(http, session, None, "orgs")?;
    if status != 200 {
        return Err(anyhow!(friendly_console_error(status, &body, true)));
    }
    if looks_signed_out(&body) {
        return Err(anyhow!("OpenCode session cookie is invalid or expired."));
    }
    let orgs: Vec<ConsoleOrg> = serde_json::from_str(&body)
        .map_err(|_| anyhow!("Missing OpenCode workspace id."))?;
    Ok(orgs.into_iter().map(|org| org.id).collect())
}

fn fetch_one_org(
    http: &HttpClient,
    session: &str,
    provider: &str,
    include_monthly: bool,
    source_label: &str,
    org_id: &str,
) -> Result<Option<ProviderPayload>> {
    let (status, body) = console_get(http, session, Some(org_id), "go/status")?;
    if status != 200 {
        return Err(anyhow!(friendly_console_error(status, &body, true)));
    }
    if looks_signed_out(&body) {
        return Err(anyhow!("OpenCode session cookie is invalid or expired."));
    }
    let status: GoStatus =
        serde_json::from_str(&body).map_err(|_| anyhow!("Missing OpenCode Go usage fields."))?;
    let Some(access) = status.access.as_ref() else {
        return Ok(None);
    };
    let Some(meters) = access.meters.as_ref() else {
        return Ok(None);
    };

    let primary = meter_window(meters.five_hour.as_ref(), NOMINAL_FIVE_HOUR_MINUTES);
    let secondary = meter_window(meters.week.as_ref(), NOMINAL_WEEK_MINUTES);
    let tertiary = if include_monthly {
        let month_fallback = access.period_minutes().unwrap_or(NOMINAL_MONTH_MINUTES);
        meter_window(meters.month.as_ref(), month_fallback)
    } else {
        None
    };
    if primary.is_none() && secondary.is_none() && tertiary.is_none() {
        return Err(anyhow!("Missing OpenCode Go usage fields."));
    }

    let account =
        fetch_org_email(http, session, org_id).or_else(|| status.subscriber_user_id.clone());
    let mut payload = ProviderPayload::ok(
        provider,
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
    payload.site_url = Some(format!("{CONSOLE_BASE}/{org_id}/go"));
    Ok(Some(payload))
}

/// Best-effort account email for an org; a missing address must not fail the
/// usage fetch.
fn fetch_org_email(http: &HttpClient, session: &str, org_id: &str) -> Option<String> {
    let (status, body) = console_get(http, session, Some(org_id), "members").ok()?;
    if status != 200 {
        return None;
    }
    let members: MembersResponse = serde_json::from_str(&body).ok()?;
    members
        .items
        .iter()
        .find(|item| item.role.as_deref() == Some("owner"))
        .or_else(|| members.items.first())
        .and_then(|item| item.email.clone())
}

/// GET a `/console/api/*` route with the console session + optional `x-org-id`,
/// returning the status and body so callers can branch on org-required /
/// unauthorized / rollups-pending instead of guessing from an error string.
fn console_get(
    http: &HttpClient,
    session: &str,
    org_id: Option<&str>,
    path: &str,
) -> Result<(u16, String)> {
    let headers = console_headers(session, org_id)?;
    http.client_get(&format!("{CONSOLE_API_BASE}/{path}"), &headers)
}

/// A meter reports spend against a limit in micro-cents; convert to a
/// remaining-percent rate window.
fn meter_window(meter: Option<&GoMeter>, fallback_minutes: i64) -> Option<RateWindow> {
    let meter = meter?;
    let limit = micro_cents(meter.limit_micro_cents.as_deref())?;
    let used = micro_cents(meter.used_micro_cents.as_deref())?;
    if !(limit > 0.0) || used < 0.0 {
        return None;
    }
    let window_minutes = meter.window_minutes().unwrap_or(fallback_minutes);
    Some(rate_window(
        clamp_percent(used / limit * 100.0),
        Some(window_minutes),
        meter.resets_at,
    ))
}

pub fn is_org_required(status: u16, body: &str) -> bool {
    status == 400 && (body.contains("OrgRequired") || body.contains("org_required"))
}

pub fn friendly_console_error(status: u16, body: &str, has_cookie: bool) -> String {
    if is_org_required(status, body) {
        return MISSING_WORKSPACE_MESSAGE.to_string();
    }
    if status == 401 || status == 403 {
        return if has_cookie {
            EXPIRED_SESSION_MESSAGE.to_string()
        } else {
            NO_SESSION_MESSAGE.to_string()
        };
    }
    if status == 404 {
        return "OpenCode workspace not found for this console session. Open opencode.ai/console and select the workspace, then retry.".to_string();
    }
    if status == 503 {
        return "OpenCode usage rollups are temporarily unavailable, try again later.".to_string();
    }
    format!("OpenCode console request failed (HTTP {status}).")
}

fn is_auth_error(error: &anyhow::Error) -> bool {
    let lower = error.to_string().to_lowercase();
    lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("not authenticated")
        || lower.contains("session cookie is invalid")
        || lower.contains("session is invalid")
}

/// Map raw reqwest/anyhow errors to short, user-readable messages so the
/// plasmoid doesn't leak `GET https://...` style strings.
fn friendly_error(error: &anyhow::Error) -> String {
    let raw = error.to_string();
    let lower = raw.to_lowercase();
    let trimmed = raw.trim();

    if lower.contains("missing opencode go usage fields")
        || lower.contains("no subscription usage data")
        || lower.contains("go plan not enabled")
    {
        return NO_PLAN_MESSAGE.to_string();
    }
    if lower.contains("opencode session cookie is invalid")
        || lower.contains("session cookie is invalid or expired")
        || lower.contains("session is invalid")
        || lower.contains("not authenticated")
    {
        return EXPIRED_SESSION_MESSAGE.to_string();
    }
    if lower.contains("opencode workspace id") || lower.contains("no opencode workspace") {
        return MISSING_WORKSPACE_MESSAGE.to_string();
    }
    if lower.contains("no opencode console session") {
        return NO_SESSION_MESSAGE.to_string();
    }
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
    {
        return EXPIRED_SESSION_MESSAGE.to_string();
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
    if trimmed.is_empty() {
        return "OpenCode Go fetch failed.".to_string();
    }
    trimmed.to_string()
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
    use super::{
        console_session_value, de_micro_cents, deduplicate_accounts, meter_window, GoAccess,
        GoMeter, GoStatus,
    };
    use crate::output::ProviderPayload;
    use chrono::{DateTime, Utc};
    use serde::Deserialize;

    fn payload(account: Option<&str>, source: &str, site_url: Option<&str>) -> ProviderPayload {
        let mut payload = ProviderPayload::error("opencodego", "unused");
        payload.error = None;
        payload.account = account.map(str::to_string);
        payload.source = source.to_string();
        payload.site_url = site_url.map(str::to_string);
        payload
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct MeterFixture {
        #[serde(default, deserialize_with = "de_micro_cents")]
        limit_micro_cents: Option<String>,
    }

    fn at(value: &str) -> Option<DateTime<Utc>> {
        Some(value.parse::<DateTime<Utc>>().expect("timestamp"))
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
            starts_at: None,
            limit_micro_cents: Some("6000000000".to_string()),
            used_micro_cents: Some("2603968926".to_string()),
            resets_at: at("2026-11-04T06:05:05.000Z"),
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
            starts_at: None,
            limit_micro_cents: None,
            used_micro_cents: Some("10".to_string()),
            resets_at: None,
        };
        assert!(meter_window(Some(&missing), 300).is_none());

        let zero = super::GoMeter {
            starts_at: None,
            limit_micro_cents: Some("0".to_string()),
            used_micro_cents: Some("0".to_string()),
            resets_at: None,
        };
        assert!(meter_window(Some(&zero), 300).is_none());
    }

    #[test]
    fn meter_window_tolerates_missing_reset_time() {
        let meter = super::GoMeter {
            starts_at: None,
            limit_micro_cents: Some("1200000000".to_string()),
            used_micro_cents: Some("0".to_string()),
            resets_at: None,
        };

        let window = meter_window(Some(&meter), 300).expect("window");
        assert_eq!(window.remaining_percent, 100.0);
        assert_eq!(window.resets_at, None);
    }

    /// A measured window must win over the nominal fallback, otherwise a
    /// freshly reset month meter is paced as if the window were 30 days long.
    #[test]
    fn measured_meter_window_beats_nominal_fallback() {
        let meter = GoMeter {
            starts_at: at("2026-10-14T19:10:26.000Z"),
            resets_at: at("2026-10-15T00:10:26.000Z"),
            limit_micro_cents: Some("1200000000".to_string()),
            used_micro_cents: Some("1373795".to_string()),
        };

        let window = meter_window(Some(&meter), super::NOMINAL_MONTH_MINUTES).expect("window");
        assert_eq!(window.window_minutes, Some(300));
    }

    /// The live `month` meter carries `resetsAt` but no `startsAt`; the billing
    /// period from `access` is the only honest window for it.
    #[test]
    fn month_window_falls_back_to_access_period() {
        let access = GoAccess {
            starts_at: at("2026-10-02T13:14:20.000Z"),
            ends_at: at("2026-11-02T13:14:20.000Z"),
            meters: None,
        };
        let meter = GoMeter {
            starts_at: None,
            resets_at: at("2026-11-02T13:14:20.000Z"),
            limit_micro_cents: Some("6000000000".to_string()),
            used_micro_cents: Some("1955706864".to_string()),
        };

        let fallback = access.period_minutes().expect("period");
        assert_eq!(fallback, 31 * 24 * 60);
        let window = meter_window(Some(&meter), fallback).expect("window");
        assert_eq!(window.window_minutes, Some(31 * 24 * 60));
    }

    #[test]
    fn access_period_needs_both_ends() {
        let access = GoAccess {
            starts_at: at("2026-10-02T13:14:20.000Z"),
            ends_at: None,
            meters: None,
        };
        assert_eq!(access.period_minutes(), None);
    }

    #[test]
    fn micro_cents_accepts_numbers_and_strings() {
        let numeric: MeterFixture =
            serde_json::from_str(r#"{"limitMicroCents": 6000000000}"#).expect("numeric limit");
        let text: MeterFixture =
            serde_json::from_str(r#"{"limitMicroCents": "6000000000"}"#).expect("string limit");
        let absent: MeterFixture = serde_json::from_str(r#"{"limitMicroCents": null}"#).expect("null");

        assert_eq!(numeric.limit_micro_cents.as_deref(), Some("6000000000"));
        assert_eq!(text.limit_micro_cents.as_deref(), Some("6000000000"));
        assert_eq!(absent.limit_micro_cents, None);
    }

    #[test]
    fn go_status_decodes_live_shape() {
        let json = r#"{
            "subscriberUserId": "acc_01KPKF0YAEVMRZCW7QXFA4W4YN",
            "product": "go",
            "access": {
                "startsAt": "2026-10-02T13:14:20.000Z",
                "endsAt": "2026-11-02T13:14:20.000Z",
                "meters": {
                    "fiveHour": {
                        "startsAt": "2026-10-14T19:10:26.000Z",
                        "resetsAt": "2026-10-15T00:10:26.000Z",
                        "limitMicroCents": "1200000000",
                        "usedMicroCents": "1373795"
                    },
                    "month": {
                        "resetsAt": "2026-11-02T13:14:20.000Z",
                        "limitMicroCents": "6000000000",
                        "usedMicroCents": "1955706864"
                    }
                }
            }
        }"#;

        let status: GoStatus = serde_json::from_str(json).expect("decode go/status");
        let access = status.access.expect("access");
        assert_eq!(access.period_minutes(), Some(31 * 24 * 60));
        let meters = access.meters.expect("meters");
        assert!(meter_window(meters.five_hour.as_ref(), 300).is_some());
        assert!(meter_window(meters.month.as_ref(), 43200).is_some());
        assert_eq!(status.subscriber_user_id.as_deref(), Some("acc_01KPKF0YAEVMRZCW7QXFA4W4YN"));
    }

    #[test]
    fn console_session_value_prefers_host_cookie() {
        assert_eq!(
            console_session_value("auth=sealed; __Host-console_session=st_1").as_deref(),
            Some("st_1")
        );
        assert_eq!(
            console_session_value("console_session=st_2").as_deref(),
            Some("st_2")
        );
        assert_eq!(console_session_value("auth=sealed"), None);
    }
}
