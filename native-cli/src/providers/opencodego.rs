use crate::config::workspace_id;
use crate::cookies::{resolve_all_cookie_headers, resolve_cookie_header};
use crate::http::HttpClient;
use crate::opencodego_local::can_read_local_usage;
use crate::output::{ProviderPayload, UsageSnapshot};
use crate::providers::opencode_shared::{
    extract_account_email, fetch_usage_page, fetch_workspace_id, parse_usage,
};
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

/// Fetch OpenCode Go **subscription** usage, discovering every signed-in account
/// across browser cookie stores. Returns one payload per account so the plasmoid
/// can render each account (with its email) separately.
///
/// Rate-limit bars (rolling / weekly / monthly) only come from the live Go plan
/// API. Local OpenCode SQLite history is pay-as-you-go spend and must not be
/// mapped onto the subscription $12/$30/$60 windows — that produced full "100%"
/// remaining bars for users with an API key but no Go plan. Local token spend is
/// still surfaced via the separate `cost` command.
pub fn fetch(http: &HttpClient, home: &Path) -> Vec<ProviderPayload> {
    let cookies = resolve_all_cookie_headers("opencodego");
    if !cookies.is_empty() {
        // A single manual config cookie may carry a configured workspace id;
        // auto-discovered browser sessions always resolve their own workspace.
        let use_config_workspace = cookies.len() == 1 && cookies[0].source == "config";
        let payloads: Vec<ProviderPayload> = cookies
            .iter()
            .map(|cookie| fetch_one_web(http, &cookie.header, &cookie.source, use_config_workspace))
            .collect();
        let successes: Vec<ProviderPayload> = payloads
            .iter()
            .filter(|payload| payload.error.is_none())
            .cloned()
            .collect();
        if !successes.is_empty() {
            return deduplicate_accounts(successes);
        }
        // Prefer the real web failure (plan not enabled, session expired, …)
        // over inventing subscription bars from local API spend.
        return payloads.into_iter().take(1).collect();
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

fn fetch_one_web(
    http: &HttpClient,
    cookie_header: &str,
    source_label: &str,
    use_config_workspace: bool,
) -> ProviderPayload {
    match fetch_one_web_inner(http, cookie_header, source_label, use_config_workspace) {
        Ok(payload) => payload,
        Err(error) => {
            let mut payload = ProviderPayload::error("opencodego", friendly_error(&error));
            payload.source = source_label.to_string();
            payload
        }
    }
}

/// Map raw reqwest/anyhow errors to short, user-readable messages so the
/// plasmoid doesn't leak `POST https://.../status` style strings.
fn friendly_error(error: &anyhow::Error) -> String {
    let raw = error.to_string();
    let lower = raw.to_lowercase();
    let trimmed = raw.trim();

    if lower.contains("missing opencode go usage fields")
        || lower.contains("no subscription usage data")
    {
        return "OpenCode Go plan not enabled for this account. Local API spend still shows under cost.".to_string();
    }
    if lower.contains("opencode session cookie is invalid")
        || lower.contains("session cookie is invalid or expired")
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

fn fetch_one_web_inner(
    http: &HttpClient,
    cookie_header: &str,
    source_label: &str,
    use_config_workspace: bool,
) -> Result<ProviderPayload> {
    let workspace = if use_config_workspace {
        match workspace_id("opencodego") {
            Some(id) => id,
            None => fetch_workspace_id(http, cookie_header)?,
        }
    } else {
        fetch_workspace_id(http, cookie_header)?
    };
    let page = fetch_usage_page(http, &workspace, cookie_header)?;
    let snapshot = parse_usage(&page, true)?;
    let account = extract_account_email(&page);
    let mut payload = ProviderPayload::ok(
        "opencodego",
        UsageSnapshot {
            primary: Some(snapshot.primary),
            secondary: Some(snapshot.secondary),
            tertiary: snapshot.tertiary,
            usage_rows: None,
            provider_cost: None,
            cursor_requests: None,
            updated_at: snapshot.updated_at,
            identity: None,
        },
        account,
        None,
        None,
    );
    payload.source = source_label.to_string();
    payload.site_url = Some(format!("https://opencode.ai/workspace/{workspace}/go"));
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::deduplicate_accounts;
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
}
