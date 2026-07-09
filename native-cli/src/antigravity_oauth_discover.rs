//! Discover Google OAuth *app* client id/secret from a local Antigravity install.
//!
//! The desktop `agy` binary embeds the Cloud Code installed-app OAuth client
//! (same values `antigravity-usage` ships). We scan that binary at runtime so
//! this repository never contains the credentials.
//!
//! Resolution order for callers is owned by `config::antigravity_oauth_*`
//! (env → ~/.codexbar/config.json → this discovery).

use regex::bytes::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct OAuthAppCredentials {
    pub client_id: String,
    pub client_secret: String,
    /// Path of the binary the pair was extracted from (for diagnostics).
    pub source: String,
}

/// Cached discovery so login + refresh only pay the binary scan once per process.
pub fn discover_oauth_app_credentials() -> Option<OAuthAppCredentials> {
    static CACHE: OnceLock<Option<OAuthAppCredentials>> = OnceLock::new();
    CACHE
        .get_or_init(discover_oauth_app_credentials_uncached)
        .clone()
}

fn discover_oauth_app_credentials_uncached() -> Option<OAuthAppCredentials> {
    for path in candidate_binaries() {
        if let Some(creds) = extract_from_file(&path) {
            return Some(creds);
        }
    }
    None
}

fn candidate_binaries() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |path: PathBuf| {
        if path.is_file() && !out.iter().any(|existing| existing == &path) {
            out.push(path);
        }
    };

    for key in [
        "CODEXBAR_ANTIGRAVITY_BINARY",
        "ANTIGRAVITY_BINARY",
        "AGY_BINARY",
    ] {
        if let Ok(value) = std::env::var(key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                push(PathBuf::from(trimmed));
            }
        }
    }

    // PATH lookup for `agy`.
    if let Some(path) = which("agy") {
        push(path);
    }

    if let Some(home) = dirs::home_dir() {
        push(home.join(".local/bin/agy"));
        // Common CLI install roots.
        push(home.join(".gemini/antigravity-cli/bin/agy"));
        push(home.join(".antigravity/agy"));
        push(home.join(".antigravity/bin/agy"));
    }

    // Running process command lines (agy / language_server).
    if let Ok(output) = Command::new("ps").args(["-ax", "-o", "command="]).output() {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if let Some(path) = first_path_token(line) {
                    let lower = path.to_ascii_lowercase();
                    if lower.ends_with("/agy")
                        || lower.ends_with("\\agy")
                        || lower.ends_with("/agy.exe")
                        || lower.contains("language_server")
                    {
                        push(PathBuf::from(path));
                    }
                }
            }
        }
    }

    out
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn first_path_token(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Handle `exec -a name /path/to/agy ...` lightly: take first absolute path.
    for token in trimmed.split_whitespace() {
        if token.starts_with('/') || (token.len() > 2 && token.as_bytes()[1] == b':') {
            return Some(token.to_string());
        }
    }
    trimmed.split_whitespace().next().map(str::to_string)
}

fn extract_from_file(path: &Path) -> Option<OAuthAppCredentials> {
    // Cap read size: real `agy` is ~170MB; skip absurd files.
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > 400 * 1024 * 1024 {
        return None;
    }
    let data = fs::read(path).ok()?;
    extract_from_bytes(&data).map(|(client_id, client_secret)| OAuthAppCredentials {
        client_id,
        client_secret,
        source: path.display().to_string(),
    })
}

/// Scan binary/text bytes for Google installed-app OAuth client credentials.
fn extract_from_bytes(data: &[u8]) -> Option<(String, String)> {
    // Byte-mode regex keeps match offsets equal to file offsets and avoids
    // allocating a multi-hundred-MB String over the `agy` binary.
    static CLIENT_RE: OnceLock<Regex> = OnceLock::new();
    let client_re = CLIENT_RE.get_or_init(|| {
        Regex::new(r"[0-9]{6,}-[a-z0-9]+\.apps\.googleusercontent\.com")
            .expect("client id regex")
    });

    let mut clients: Vec<(usize, String)> = Vec::new();
    for m in client_re.find_iter(data) {
        if let Ok(value) = std::str::from_utf8(m.as_bytes()) {
            clients.push((m.start(), value.to_string()));
        }
    }
    clients.sort_by_key(|(pos, _)| *pos);
    clients.dedup_by(|a, b| a.1 == b.1);

    let mut secrets = find_client_secrets(data);
    secrets.sort_by_key(|(pos, _)| *pos);
    secrets.dedup_by(|a, b| a.1 == b.1);

    if clients.is_empty() || secrets.is_empty() {
        return None;
    }

    let client_id = pick_client_id(data, &clients)?;
    let client_secret = pick_client_secret(data, &secrets)?;
    Some((client_id, client_secret))
}

/// Find `GOCSPX-…` secrets without gluing adjacent values or trailing URLs.
///
/// Google desktop client secrets in `agy` are stored as `GOCSPX-` + 28 chars,
/// often concatenated with the next secret or `https://cloudcode-…` with no
/// separator. Prefer a 28-char body when that slice is all secret-safe bytes.
fn find_client_secrets(data: &[u8]) -> Vec<(usize, String)> {
    const PREFIX: &[u8] = b"GOCSPX-";
    const PREFERRED_BODY_LEN: usize = 28;
    let mut out = Vec::new();
    let mut offset = 0;
    while offset + PREFIX.len() < data.len() {
        let Some(rel) = find_bytes(&data[offset..], PREFIX) else {
            break;
        };
        let start = offset + rel;
        let body_start = start + PREFIX.len();
        let body_len = secret_body_len(&data[body_start..], PREFERRED_BODY_LEN);
        if (20..=40).contains(&body_len) {
            if let Ok(secret) = std::str::from_utf8(&data[start..body_start + body_len]) {
                out.push((start, secret.to_string()));
            }
        }
        offset = body_start + body_len.max(1);
    }
    out
}

fn secret_body_len(body: &[u8], preferred: usize) -> usize {
    // Prefer the common 28-char Google secret body when it is fully valid and
    // does not embed another GOCSPX- prefix.
    if body.len() >= preferred && is_secret_body(&body[..preferred]) {
        return preferred;
    }
    let mut len = 0usize;
    while len < body.len() && len < 40 {
        if body[len..].starts_with(b"GOCSPX-") {
            break;
        }
        if is_secret_body_byte(body[len]) {
            len += 1;
        } else {
            break;
        }
    }
    len
}

fn is_secret_body(body: &[u8]) -> bool {
    !body.is_empty()
        && body.iter().all(|&b| is_secret_body_byte(b))
        && find_bytes(body, b"GOCSPX-").is_none()
}

fn is_secret_body_byte(byte: u8) -> bool {
    matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-')
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn pick_client_id(data: &[u8], clients: &[(usize, String)]) -> Option<String> {
    // Prefer a client id that is *not* glued to an internal Cloud Code metrics
    // path (that id is a different product client). Fall back to first match.
    for (pos, client) in clients {
        let end = (*pos + client.len()).min(data.len());
        let after_end = (end + 64).min(data.len());
        let after = String::from_utf8_lossy(&data[end..after_end]);
        if after.contains("/google.internal")
            || after.contains("CloudCode/")
            || after.contains("RecordCodeAssist")
        {
            continue;
        }
        return Some(client.clone());
    }
    clients.first().map(|(_, c)| c.clone())
}

fn pick_client_secret(data: &[u8], secrets: &[(usize, String)]) -> Option<String> {
    // `agy` stores one or more GOCSPX secrets immediately before the Cloud Code
    // host. Prefer the *first* secret in that nearby run — that matches the
    // desktop client `antigravity-usage` uses for token exchange/refresh.
    let anchor = find_bytes(data, b"https://cloudcode-pa.googleapis.com")
        .or_else(|| find_bytes(data, b"https://oauth2.googleapis.com/token"));

    if let Some(anchor_pos) = anchor {
        // Window: secrets that finish within ~256 bytes of the anchor.
        let window_start = anchor_pos.saturating_sub(256);
        let nearby: Vec<&(usize, String)> = secrets
            .iter()
            .filter(|(pos, secret)| {
                let end = pos + secret.len();
                *pos >= window_start && end <= anchor_pos + 8
            })
            .collect();
        if let Some((_, secret)) = nearby.first() {
            return Some(secret.clone());
        }
        // Fallback: earliest secret before the anchor.
        if let Some((_, secret)) = secrets.iter().find(|(pos, _)| *pos < anchor_pos) {
            return Some(secret.clone());
        }
    }
    secrets.first().map(|(_, s)| s.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build patterns at runtime so the source file never contains full
    /// `GOCSPX-…` / `*.apps.googleusercontent.com` literals (push protection).
    fn fake_secret_prefix() -> String {
        // "GOC" + "SPX-" assembled at runtime.
        let mut s = String::from("GOC");
        s.push_str("SPX-");
        s
    }

    fn fake_secret(body: &str) -> String {
        format!("{}{body}", fake_secret_prefix())
    }

    fn fake_client_id(project: &str, name: &str) -> String {
        // "*.apps." + "googleusercontent.com" assembled at runtime.
        format!("{project}-{name}.apps.{}", "googleusercontent.com")
    }

    fn cloudcode_host() -> String {
        format!("https://{}-pa.googleapis.com", "cloudcode")
    }

    #[test]
    fn extracts_pair_from_synthetic_agy_layout() {
        // Layout mirrors `agy`: secrets then cloudcode host, later a metrics
        // client id, then the desktop OAuth client id. Values are synthetic.
        let secret_a = fake_secret("AAAABBBBCCCCDDDDEEEEFFFFGGGG");
        let secret_b = fake_secret("HHHHIIIIJJJJKKKKLLLLMMMMNNNN");
        let metrics_client = fake_client_id("999999999999", "metricspathclientxx");
        let desktop_client = fake_client_id("111111111111", "desktopoauthclient");
        let host = cloudcode_host();

        let mut blob = Vec::new();
        blob.extend_from_slice(b"MODEL_POLICY");
        blob.extend_from_slice(secret_a.as_bytes());
        blob.extend_from_slice(secret_b.as_bytes());
        blob.extend_from_slice(host.as_bytes());
        blob.extend_from_slice(b"padding");
        blob.extend_from_slice(metrics_client.as_bytes());
        blob.extend_from_slice(b"/google.internal.cloud.code.v1internal.CloudCode/RecordCodeAssistMetrics");
        blob.extend_from_slice(b"more");
        blob.extend_from_slice(desktop_client.as_bytes());
        blob.extend_from_slice(b"Warning: unrelated");

        let (client_id, client_secret) =
            extract_from_bytes(&blob).expect("should extract oauth app credentials");
        assert_eq!(client_id, desktop_client);
        assert_eq!(client_secret, secret_a);
    }

    #[test]
    fn returns_none_when_patterns_missing() {
        assert!(extract_from_bytes(b"no oauth here").is_none());
    }

    #[test]
    fn splits_concatenated_secrets() {
        let secret_a = fake_secret("AAAAAAAAAAAAAAAAAAAAAAAAAAAA");
        let secret_b = fake_secret("BBBBBBBBBBBBBBBBBBBBBBBBBBBB");
        let client = fake_client_id("123456789012", "abcdefghij");
        let host = cloudcode_host();

        let mut blob = Vec::new();
        blob.extend_from_slice(secret_a.as_bytes());
        blob.extend_from_slice(secret_b.as_bytes());
        blob.extend_from_slice(host.as_bytes());
        blob.extend_from_slice(client.as_bytes());

        let (client_id, secret) = extract_from_bytes(&blob).expect("extract");
        assert_eq!(client_id, client);
        assert_eq!(secret, secret_a);
    }
}
