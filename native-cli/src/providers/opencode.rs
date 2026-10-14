use crate::http::HttpClient;
use crate::opencode_console;
use crate::output::ProviderPayload;
use std::path::Path;

/// The OpenCode subscription card. OpenCode folded its workspace pages into the
/// unified console (see `opencode_console.rs`); the same `go/status` meters feed
/// this provider minus the monthly window, matching the long-standing two-bar
/// contract (rolling + weekly) this provider renders.
pub fn fetch(http: &HttpClient, timeout_ms: u64) -> Vec<ProviderPayload> {
    let _ = timeout_ms;
    opencode_console::fetch(http, "opencode", false, Path::new("/"))
}
