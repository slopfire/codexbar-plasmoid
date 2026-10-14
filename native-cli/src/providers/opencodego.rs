use crate::http::HttpClient;
use crate::opencode_console;
use crate::output::ProviderPayload;
use std::path::Path;

/// Fetch OpenCode Go **subscription** rate limits from the console API
/// (`/console/api/go/status`, see `opencode_console.rs` for the protocol).
///
/// Rate-limit bars only come from the live Go plan API. Local OpenCode SQLite
/// history is pay-as-you-go spend and must not be mapped onto the subscription
/// windows — that produced full "100%" remaining bars for users with an API key
/// but no Go plan. Local token spend still shows under the separate `cost`
/// command.
pub fn fetch(http: &HttpClient, home: &Path) -> Vec<ProviderPayload> {
    opencode_console::fetch(http, "opencodego", true, home)
}
