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
) -> ProviderPayload {
    match provider {
        "antigravity" => antigravity::fetch(timeout),
        "cursor" => cursor::fetch(http, include_status),
        "opencode" => opencode::fetch(http),
        "opencodego" => opencodego::fetch(http, home),
        _ => ProviderPayload::error(provider, format!("Provider not supported by codexbar-plasmoid: {provider}")),
    }
}
