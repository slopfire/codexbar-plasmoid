use crate::config::workspace_id;
use crate::cookies::resolve_cookie_header;
use crate::http::HttpClient;
use crate::opencodego_local::{can_read_local_usage, fetch_local_usage};
use crate::output::{ProviderPayload, UsageSnapshot};
use crate::providers::opencode_shared::{fetch_usage_page, fetch_workspace_id, parse_usage};
use anyhow::Result;
use std::path::Path;

pub fn fetch(http: &HttpClient, home: &Path) -> ProviderPayload {
    match fetch_inner(http, home) {
        Ok(payload) => payload,
        Err(error) => ProviderPayload::error("opencodego", error.to_string()),
    }
}

fn fetch_inner(http: &HttpClient, home: &Path) -> Result<ProviderPayload> {
    if let Ok(cookie) = resolve_cookie_header("opencodego") {
        if let Ok(payload) = fetch_web(http, &cookie.header) {
            return Ok(payload);
        }
    }

    if !can_read_local_usage(home) {
        return Err(match resolve_cookie_header("opencodego") {
            Err(error) => error,
            Ok(_) => anyhow::anyhow!("OpenCode Go not detected."),
        });
    }

    let local = fetch_local_usage(home)?;
    Ok(ProviderPayload::ok(
        "opencodego",
        UsageSnapshot {
            primary: Some(local.primary),
            secondary: Some(local.secondary),
            tertiary: Some(local.tertiary),
            provider_cost: None,
            cursor_requests: None,
            updated_at: local.updated_at,
            identity: None,
        },
        None,
        None,
        None,
    ))
}

fn fetch_web(http: &HttpClient, cookie_header: &str) -> Result<ProviderPayload> {
    let workspace = match workspace_id("opencodego") {
        Some(id) => id,
        None => fetch_workspace_id(http, cookie_header)?,
    };
    let page = fetch_usage_page(http, &workspace, cookie_header)?;
    let snapshot = parse_usage(&page, true)?;
    Ok(ProviderPayload::ok(
        "opencodego",
        UsageSnapshot {
            primary: Some(snapshot.primary),
            secondary: Some(snapshot.secondary),
            tertiary: snapshot.tertiary,
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
