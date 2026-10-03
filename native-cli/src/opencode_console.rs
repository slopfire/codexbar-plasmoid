use crate::config::workspace_id;
use crate::cookies::{opencode_workspace_ids_from_browsers, resolve_cookie_header};
use crate::http::HttpClient;
use crate::output::{ProviderPayload, RateWindow, UsageSnapshot};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// OpenCode moved the workspace/go pages into the unified console
/// (opencode.ai/console/<workspace>/go, authenticated by the
/// `__Host-console_session` cookie) and deprecated the SolidStart
/// /workspace/* pages and `/_server` server functions this fetcher used to
/// scrape. The new console exposes a small Effect HTTP API under
/// `https://opencode.ai/console/api`:
///
///   GET /console/api/go/status — Go / Go Plus rate-limit meters
///     (fiveHour / week / month windows with used/limit micro-cents)
///
/// Every org-scoped call requires the workspace id in an `x-org-id` header
/// (`org_required` otherwise) plus a valid console session cookie
/// (`Unauthorized` for an invalid session).
///
/// Workspaces are stable `wrk_...` ids, so every account is addressed by id;
/// cookie discovery only decides which browser session authenticates the
/// requests.

const CONSOLE_API_BASE: &str = "https://opencode.ai/console/api";
const CONSOLE_BASE: &str = "https://opencode.ai/console";

const MISSING_WORKSPACE_MESSAGE: &str =
    "OpenCode workspace id not found. Open opencode.ai/console in a browser once (or set CODEXBAR_OPENCODE_GO_WORKSPACE_ID / CODEXBAR_OPENCODE_WORKSPACE_ID), so the widget can read the current workspace.";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoMeter {
    #[serde(default)]
    pub starts_at: Option<DateTime<Utc>>,
    pub resets_at: Option<DateTime<Utc>>,
    #[serde(deserialize_with = "de_string_or_number")]
    pub limit_micro_cents: f64,
    #[serde(deserialize_with = "de_string_or_number")]
    pub used_micro_cents: f64,
}

impl GoMeter {
    pub fn used_percent(&self) -> Option<f64> {
        if self.limit_micro_cents <= 0.0 {
            None
        } else {
            Some((self.used_micro_cents / self.limit_micro_cents * 100.0).clamp(0.0, 100.0))
        }
    }

    pub fn window_minutes(&self) -> Option<i64> {
        if let (Some(start), Some(reset)) = (self.starts_at, self.resets_at) {
            let minutes = (reset - start).num_minutes();
            if minutes > 0 {
                return Some(minutes);
            }
        }
        let reset = self.resets_at?;
        let now = Utc::now();
        if reset > now {
            Some((reset - now).num_minutes().max(1))
        } else {
            None
        }
    }
}

fn de_string_or_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct NumericOrString;
    impl serde::de::Visitor<'_> for NumericOrString {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("number or numeric string")
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(v as f64)
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(v as f64)
        }
        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Ok(v)
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
            v.trim().parse::<f64>().map_err(serde::de::Error::custom)
        }
    }
    deserializer.deserialize_any(NumericOrString)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoMeters {
    #[serde(default)]
    pub five_hour: Option<GoMeter>,
    #[serde(default)]
    pub week: Option<GoMeter>,
    #[serde(default)]
    pub month: Option<GoMeter>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoAccess {
    #[serde(default)]
    pub meters: Option<GoMeters>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoStatus {
    /// `go` (base tier) or `go-plus`; documented but not yet surfaced in the
    /// payload rendering.
    #[allow(dead_code)]
    #[serde(default)]
    pub product: Option<String>,
    #[serde(default)]
    pub subscriber_user_id: Option<String>,
    #[serde(default)]
    pub access: Option<GoAccess>,
}

impl GoStatus {
    /// Access window may be absent for accounts without an active Go plan.
    pub fn has_active_access(&self) -> bool {
        self.access
            .as_ref()
            .map(|access| access.meters.is_some())
            .unwrap_or(false)
    }
}

/// Resolve workspace ids: explicit config/env first, then browser localStorage
/// (`lastSeenWorkspaceID`), deduplicated.
pub fn resolve_workspaces() -> Vec<String> {
    if let Some(id) = workspace_id("opencodego").or_else(|| workspace_id("opencode")) {
        return vec![id];
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for candidate in opencode_workspace_ids_from_browsers() {
        if seen.insert(candidate.clone()) {
            out.push(candidate);
        }
    }
    out
}

fn console_session_cookie(resolution: &crate::cookies::CookieResolution) -> Result<String> {
    for part in resolution.header.split(';') {
        let trimmed = part.trim();
        if let Some((name, value)) = trimmed.split_once('=') {
            if name.trim() == "__Host-console_session" && !value.trim().is_empty() {
                return Ok(value.trim().to_string());
            }
        }
    }
    Err(anyhow!(
        "No OpenCode console session (__Host-console_session cookie) found in this browser. Sign in at opencode.ai/console, then retry."
    ))
}

/// GET `<CONSOLE_API_BASE>/<path>` with the console session + x-org-id header.
/// Returns (status, body) so callers can branch on org-required/unauthorized.
pub fn console_get(
    http: &HttpClient,
    session_value: &str,
    org_id: &str,
    path: &str,
) -> Result<(u16, String)> {
    let url = format!("{CONSOLE_API_BASE}/{path}");
    let headers = crate::http::console_headers(session_value, org_id)?;
    http.client_get(&url, &headers)
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
            "OpenCode console session is invalid for this workspace. Sign in at opencode.ai/console, then retry.".to_string()
        } else {
            "No OpenCode console session (__Host-console_session cookie) found. Sign in at opencode.ai/console in Chromium, then retry.".to_string()
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

pub fn site_url_for(workspace: &str, page: &str) -> String {
    format!("{CONSOLE_BASE}/{workspace}/{page}")
}

/// Fetch `go/status` and decode it, mapping HTTP failures to friendly errors.
pub fn fetch_go_status(
    http: &HttpClient,
    session_value: &str,
    workspace: &str,
) -> Result<GoStatus> {
    let (status, body) = console_get(http, session_value, workspace, "go/status")?;
    if status != 200 {
        return Err(anyhow!(friendly_console_error(status, &body, true)));
    }
    serde_json::from_str(&body).map_err(|error| anyhow!("decode go/status JSON: {error}"))
}

fn fetch_one_console(
    http: &HttpClient,
    session_value: &str,
    workspace: &str,
    provider: &str,
    include_monthly: bool,
    source_label: &str,
) -> Result<ProviderPayload> {
    let status = fetch_go_status(http, session_value, workspace)?;
    if !status.has_active_access() {
        return Err(anyhow!(
            "No active OpenCode Go plan on workspace {workspace}. Local API spend still shows under cost."
        ));
    }
    let access = status
        .access
        .as_ref()
        .and_then(|access| access.meters.as_ref());
    let meters =
        access.ok_or_else(|| anyhow!("OpenCode Go subscription has no rate-limit meters."))?;

    let now = Utc::now();
    let primary = meters.five_hour.as_ref().map(|m| window_for(m, &now));
    let secondary = meters.week.as_ref().map(|m| window_for(m, &now));
    let tertiary = if include_monthly {
        meters.month.as_ref().map(|m| window_for(m, &now))
    } else {
        None
    };

    let mut payload = ProviderPayload::ok(
        provider,
        UsageSnapshot {
            primary,
            secondary,
            tertiary,
            usage_rows: None,
            provider_cost: None,
            cursor_requests: None,
            updated_at: now,
            identity: None,
        },
        None,
        None,
        None,
    );
    payload.account = status.subscriber_user_id.clone();
    payload.source = source_label.to_string();
    payload.site_url = Some(site_url_for(workspace, "go"));
    Ok(payload)
}

fn window_for(meter: &GoMeter, _now: &DateTime<Utc>) -> RateWindow {
    let used = meter.used_percent().unwrap_or(0.0);
    RateWindow {
        used_percent: used,
        remaining_percent: (100.0 - used).max(0.0),
        window_minutes: meter.window_minutes(),
        resets_at: meter.resets_at,
        reset_description: None,
    }
}

/// Fetch OpenCode Go subscription usage via the console API for every
/// discovered workspace × resolved browser session. One payload per account.
pub fn fetch(http: &HttpClient, _home: &Path) -> Vec<ProviderPayload> {
    let workspaces = resolve_workspaces();
    if workspaces.is_empty() {
        return vec![ProviderPayload::error(
            "opencodego",
            MISSING_WORKSPACE_MESSAGE,
        )];
    }
    // The old `opencode` flow keyed accounts by the `auth` cookie; the new
    // console session cookie lives under the opencodego discovery path too
    // (shared names_for list), so reuse it and fall back to the opencode key.
    let resolution =
        match resolve_cookie_header("opencodego").or_else(|_| resolve_cookie_header("opencode")) {
            Ok(resolution) => resolution,
            Err(error) => return vec![ProviderPayload::error("opencodego", format!("{error:#}"))],
        };
    let session_value = match console_session_cookie(&resolution) {
        Ok(value) => value,
        Err(error) => return vec![ProviderPayload::error("opencodego", format!("{error:#}"))],
    };

    let mut seen = HashSet::new();
    let mut outputs = Vec::new();
    for workspace in &workspaces {
        let mut payload = match fetch_one_console(
            http,
            &session_value,
            workspace,
            "opencodego",
            true,
            &format!("{} ({workspace})", resolution.source),
        ) {
            Ok(payload) => payload,
            Err(error) => {
                let mut payload = ProviderPayload::error("opencodego", format!("{error:#}"));
                payload.source = resolution.source.clone();
                payload
            }
        };
        let key = payload.account.clone().unwrap_or_else(|| workspace.clone());
        if seen.insert(key) {
            payload.account = payload.account.or(None);
            outputs.push(payload);
        }
    }
    let successes: Vec<ProviderPayload> = outputs
        .iter()
        .filter(|payload| payload.error.is_none())
        .cloned()
        .collect();
    if !successes.is_empty() {
        return successes;
    }
    // Prefer the real web failure over inventing bars from local API spend.
    outputs.into_iter().take(1).collect()
}

/// The `opencode` provider card: same Go subscription meters, without the
/// monthly window (pre-2026-10 contract showed rolling + weekly only).
pub fn fetch_opencode(http: &HttpClient, timeout_ms: u64) -> Vec<ProviderPayload> {
    let _ = timeout_ms;
    let mut payloads = fetch(http, Path::new("/"));
    for payload in payloads.iter_mut() {
        payload.provider = "opencode".to_string();
        if let Some(usage) = payload.usage.as_mut() {
            usage.tertiary = None;
        }
    }
    payloads
}
