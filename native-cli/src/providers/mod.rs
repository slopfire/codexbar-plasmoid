mod antigravity;
mod cursor;
mod opencode;
mod opencode_shared;
mod opencodego;

use crate::http::HttpClient;
use crate::output::ProviderPayload;
use std::path::Path;
use std::time::Duration;

pub const NATIVE_PROVIDERS: &[&str] = &["antigravity", "cursor", "opencode", "opencodego"];

pub fn fetch_provider(
    provider: &str,
    http: &HttpClient,
    home: &Path,
    include_status: bool,
    timeout: Duration,
) -> Vec<ProviderPayload> {
    match provider {
        "antigravity" => vec![antigravity::fetch(timeout)],
        "cursor" => vec![cursor::fetch(http, include_status)],
        "opencode" => vec![opencode::fetch(http)],
        "opencodego" => opencodego::fetch(http, home),
        _ => vec![ProviderPayload::error(
            provider,
            format!("Provider not supported by codexbar-plasmoid: {provider}"),
        )],
    }
}
