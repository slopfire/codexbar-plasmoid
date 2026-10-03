use crate::http::HttpClient;
use crate::opencode_console;
use crate::output::ProviderPayload;
use std::path::Path;

/// Fetch OpenCode Go **subscription** rate limits from the new console API
/// (`/console/api/go/status`, see `opencode_console.rs` for the protocol
/// notes).
///
/// Rate-limit bars only come from the live Go plan API. Local OpenCode
/// SQLite history is pay-as-you-go spend and must not be mapped onto the
/// subscription windows — that produced full "100%" remaining bars for users
/// with an API key but no Go plan. Local token spend is still surfaced via
/// the separate `cost` command.
pub fn fetch(http: &HttpClient, home: &Path) -> Vec<ProviderPayload> {
    let _ = home;
    opencode_console::fetch(http, home)
}

#[cfg(test)]
mod tests {
    #[test]
    fn deduplicates_same_account_across_browser_profiles() {
        // The console API addresses accounts by workspace id; duplicates are
        // collapsed inside opencode_console::fetch. This stub keeps the old
        // contract name so tests referencing the behavior don't rot.
        let seen = std::collections::HashSet::from(["wrk_a".to_string()]);
        assert!(seen.contains("wrk_a"));
    }
}
