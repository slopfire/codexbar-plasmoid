use crate::config::workspace_id;
use crate::cookies::resolve_cookie_header;
use crate::http::HttpClient;
use crate::output::{ProviderPayload, UsageSnapshot};
use crate::providers::opencode_shared::{fetch_subscription, fetch_workspace_id, parse_usage};
use anyhow::Result;

pub fn fetch(http: &HttpClient) -> ProviderPayload {
    match fetch_inner(http) {
        Ok(payload) => payload,
        Err(error) => ProviderPayload::error("opencode", error.to_string()),
    }
}

fn fetch_inner(http: &HttpClient) -> Result<ProviderPayload> {
    let cookie = resolve_cookie_header("opencode")?;
    let workspace = match workspace_id("opencode") {
        Some(id) => id,
        None => fetch_workspace_id(http, &cookie.header)?,
    };
    let subscription = fetch_subscription(http, &workspace, &cookie.header)?;
    let snapshot = parse_usage(&subscription, false)?;

    Ok(ProviderPayload::ok(
        "opencode",
        UsageSnapshot {
            primary: Some(snapshot.primary),
            secondary: Some(snapshot.secondary),
            tertiary: None,
            usage_rows: None,
            provider_cost: None,
            cursor_requests: None,
            updated_at: snapshot.updated_at,
            identity: None,
        },
        None,
        None,
        None,
    ))
}
