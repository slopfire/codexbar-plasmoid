mod antigravity;
mod cursor;
mod devin;
mod grok;
mod opencode;
mod opencode_shared;
mod opencodego;

use crate::http::HttpClient;
use crate::output::ProviderPayload;
use std::path::Path;
use std::time::Duration;

pub const NATIVE_PROVIDERS: &[&str] = &[
    "antigravity",
    "cursor",
    "devin",
    "grok",
    "opencode",
    "opencodego",
];

pub fn fetch_provider(
    provider: &str,
    http: &HttpClient,
    home: &Path,
    include_status: bool,
    timeout: Duration,
    source: &str,
) -> Vec<ProviderPayload> {
    match provider {
        "antigravity" => {
            vec![antigravity::fetch(http, timeout, antigravity::method_for_source(source))]
        }
        "cursor" => vec![cursor::fetch(http, include_status)],
        "devin" => vec![devin::fetch(http)],
        "grok" => grok::fetch(http),
        "opencode" => vec![opencode::fetch(http)],
        "opencodego" => opencodego::fetch(http, home),
        _ => vec![ProviderPayload::error(
            provider,
            format!("Provider not supported by codexbar-plasmoid: {provider}"),
        )],
    }
}
