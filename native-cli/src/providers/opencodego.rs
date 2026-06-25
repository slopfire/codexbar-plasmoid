use crate::config::workspace_id;
use crate::cookies::{resolve_all_cookie_headers, resolve_cookie_header};
use crate::http::HttpClient;
use crate::opencodego_local::{can_read_local_usage, fetch_local_usage};
use crate::output::{ProviderPayload, UsageSnapshot};
use crate::providers::opencode_shared::{extract_account_email, fetch_usage_page, fetch_workspace_id, parse_usage};
use anyhow::Result;
use std::path::Path;

/// Fetch OpenCode Go usage, discovering every signed-in account across browser
/// cookie stores. Returns one payload per account so the plasmoid can render
/// each account (with its email) separately.
pub fn fetch(http: &HttpClient, home: &Path) -> Vec<ProviderPayload> {
    let cookies = resolve_all_cookie_headers("opencodego");
    if !cookies.is_empty() {
        // A single manual config cookie may carry a configured workspace id;
        // auto-discovered browser sessions always resolve their own workspace.
        let use_config_workspace = cookies.len() == 1 && cookies[0].source == "config";
        return cookies
            .iter()
            .map(|cookie| fetch_one_web(http, &cookie.header, &cookie.source, use_config_workspace))
            .collect();
    }

    if can_read_local_usage(home) {
        return vec![fetch_local(home)];
    }

    vec![ProviderPayload::error(
        "opencodego",
        resolve_cookie_header("opencodego")
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "OpenCode Go not detected.".to_string()),
    )]
}

fn fetch_one_web(http: &HttpClient, cookie_header: &str, source_label: &str, use_config_workspace: bool) -> ProviderPayload {
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

    if lower.contains("missing opencode go usage fields") {
        return "OpenCode Go not enabled for this account.".to_string();
    }
    if lower.contains("opencode session cookie is invalid") || lower.contains("session cookie is invalid or expired") {
        return "Session expired. Re-login to OpenCode Go in this browser.".to_string();
    }
    if lower.contains("opencode workspace id") {
        return "No OpenCode workspace found for this account.".to_string();
    }
    if lower.contains("401") || lower.contains("403") || lower.contains("unauthorized") || lower.contains("forbidden") {
        return "Session expired. Re-login to OpenCode Go in this browser.".to_string();
    }
    if lower.contains("404") || lower.contains("not found") {
        return "OpenCode Go not available for this account.".to_string();
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return "Request timed out.".to_string();
    }
    if lower.contains("connection refused") || lower.contains("dns") || lower.contains("network") || lower.contains("unreachable") {
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
    Ok(payload)
}

fn fetch_local(home: &Path) -> ProviderPayload {
    match fetch_local_usage(home) {
        Ok(local) => ProviderPayload::ok(
            "opencodego",
            UsageSnapshot {
                primary: Some(local.primary),
                secondary: Some(local.secondary),
                tertiary: Some(local.tertiary),
                usage_rows: None,
                provider_cost: None,
                cursor_requests: None,
                updated_at: local.updated_at,
                identity: None,
            },
            None,
            None,
            None,
        ),
        Err(error) => ProviderPayload::error("opencodego", error.to_string()),
    }
}
