use crate::http::HttpClient;
use crate::output::ProviderPayload;

/// The OpenCode subscription card. OpenCode folded Lite/Go into the unified
/// console (see `opencode_console.rs`); the same `go/status` meters feed this
/// provider minus the monthly window, matching the long-standing two-bar
/// contract (rolling + weekly) this provider used to render.
pub fn fetch(http: &HttpClient, timeout_ms: u64) -> Vec<ProviderPayload> {
    crate::opencode_console::fetch_opencode(http, timeout_ms)
}
