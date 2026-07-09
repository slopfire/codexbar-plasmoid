//! Browser OAuth login for Antigravity Cloud Code (native-auth).
//!
//! Opens a Google OAuth consent page, captures the redirect on localhost,
//! exchanges the code for tokens, and writes them in the same on-disk layout
//! as `antigravity-usage` (`~/.config/antigravity-usage/...`) so native-auth
//! usage fetching works without the npm CLI.
//!
//! OAuth *app* client id/secret are never shipped in this binary — supply them
//! via `ANTIGRAVITY_OAUTH_CLIENT_ID` / `ANTIGRAVITY_OAUTH_CLIENT_SECRET` or
//! `oauth_client_*` on the antigravity entry in `~/.codexbar/config.json`.

use crate::config;
use crate::http::HttpClient;
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

const OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const OAUTH_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const OAUTH_SCOPES: &str =
    "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email";
const DEFAULT_LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone)]
pub struct LoginOptions {
    /// Prefer manual URL paste instead of a localhost callback server.
    pub manual: bool,
    /// Print the auth URL but do not open a browser (still wait for callback).
    pub no_browser: bool,
    /// Preferred localhost port; `None` picks an ephemeral free port.
    pub port: Option<u16>,
    /// How long to wait for the browser redirect / pasted URL.
    pub timeout: Duration,
}

impl Default for LoginOptions {
    fn default() -> Self {
        Self {
            manual: false,
            no_browser: false,
            port: None,
            timeout: DEFAULT_LOGIN_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTokens {
    access_token: String,
    refresh_token: String,
    /// Unix timestamp in milliseconds.
    expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    #[serde(default)]
    email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalConfig {
    #[serde(default = "default_config_version")]
    version: String,
    #[serde(default)]
    active_account: Option<String>,
    #[serde(default)]
    preferences: ConfigPreferences,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ConfigPreferences {
    /// Matches antigravity-usage's on-disk key (`cacheTTL`).
    #[serde(default = "default_cache_ttl", rename = "cacheTTL")]
    cache_ttl: u64,
}

fn default_config_version() -> String {
    "2.0".to_string()
}

fn default_cache_ttl() -> u64 {
    300
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AccountMetadata {
    email: String,
    added_at: String,
    last_used: String,
}

/// Run the Google browser OAuth flow and persist tokens for native-auth.
/// Returns the signed-in email when known.
pub fn login(http: &HttpClient, options: LoginOptions) -> Result<Option<String>> {
    let client_id = require_client_id()?;
    let client_secret = require_client_secret()?;
    if let Some(creds) = crate::antigravity_oauth_discover::discover_oauth_app_credentials() {
        if creds.client_id == client_id {
            eprintln!("Using OAuth app client extracted from {}", creds.source);
        }
    }

    let (code, redirect_uri) = if options.manual {
        obtain_code_manual(&client_id, options.timeout)?
    } else {
        obtain_code_callback(&client_id, &options)?
    };

    let tokens = exchange_code(http, &client_id, &client_secret, &code, &redirect_uri)?;
    let email = fetch_email(http, &tokens.access_token).ok().flatten();

    let mut stored = StoredTokens {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token.unwrap_or_default(),
        expires_at: Utc::now().timestamp_millis() + tokens.expires_in.saturating_mul(1000),
        email: email.clone(),
        project_id: None,
    };

    if stored.refresh_token.is_empty() {
        eprintln!(
            "warning: Google did not return a refresh_token. Re-run login with consent \
             (or revoke prior app access) if token refresh fails later."
        );
    }

    save_login_tokens(&mut stored)?;
    Ok(stored.email)
}

/// Remove stored native-auth credentials.
pub fn logout(email: Option<&str>, all: bool) -> Result<String> {
    let config_dir = antigravity_usage_config_dir();
    if all {
        let accounts_dir = config_dir.join("accounts");
        let mut count = 0usize;
        if accounts_dir.is_dir() {
            for entry in fs::read_dir(&accounts_dir).context("list accounts directory")? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    fs::remove_dir_all(entry.path())
                        .with_context(|| format!("remove {}", entry.path().display()))?;
                    count += 1;
                }
            }
        }
        let legacy = config_dir.join("tokens.json");
        if legacy.exists() {
            fs::remove_file(&legacy).context("remove legacy tokens.json")?;
            count = count.saturating_add(1);
        }
        write_active_account(None)?;
        return Ok(format!("Logged out of {count} account(s)."));
    }

    if let Some(email) = email.filter(|value| !value.trim().is_empty()) {
        let path = account_dir(email);
        if !path.exists() {
            bail!("Account '{email}' not found under {}", path.display());
        }
        fs::remove_dir_all(&path).with_context(|| format!("remove {}", path.display()))?;
        let mut active = load_global_config();
        if active.active_account.as_deref() == Some(email) {
            active.active_account = first_remaining_account();
            save_global_config(&active)?;
        }
        return Ok(format!("Logged out of {email}."));
    }

    // Default: active account, else legacy tokens.json.
    let mut active = load_global_config();
    if let Some(email) = active.active_account.clone() {
        let path = account_dir(&email);
        if path.exists() {
            fs::remove_dir_all(&path).with_context(|| format!("remove {}", path.display()))?;
        }
        active.active_account = first_remaining_account();
        save_global_config(&active)?;
        return Ok(format!("Logged out of {email}."));
    }

    let legacy = config_dir.join("tokens.json");
    if legacy.exists() {
        fs::remove_file(&legacy).context("remove legacy tokens.json")?;
        return Ok("Logged out (legacy tokens.json removed).".to_string());
    }

    bail!("Not logged in. Nothing to remove.");
}

fn require_client_id() -> Result<String> {
    config::antigravity_oauth_client_id().ok_or_else(|| {
        anyhow!(
            "Missing Antigravity OAuth client id. Install/run the local `agy` Antigravity CLI \
             (credentials are read from that binary), or set ANTIGRAVITY_OAUTH_CLIENT_ID \
             / oauth_client_id in ~/.codexbar/config.json."
        )
    })
}

fn require_client_secret() -> Result<String> {
    config::antigravity_oauth_client_secret().ok_or_else(|| {
        anyhow!(
            "Missing Antigravity OAuth client secret. Install/run the local `agy` Antigravity CLI \
             (credentials are read from that binary), or set ANTIGRAVITY_OAUTH_CLIENT_SECRET \
             / oauth_client_secret in ~/.codexbar/config.json."
        )
    })
}

fn build_auth_url(client_id: &str, redirect_uri: &str, state: &str) -> Result<String> {
    let mut url = Url::parse(OAUTH_AUTH_URL).context("parse OAuth auth URL")?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("client_id", client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("response_type", "code");
        query.append_pair("scope", OAUTH_SCOPES);
        query.append_pair("access_type", "offline");
        query.append_pair("prompt", "consent");
        query.append_pair("state", state);
    }
    Ok(url.into())
}

fn obtain_code_manual(client_id: &str, _timeout: Duration) -> Result<(String, String)> {
    // Pick a free loopback port for redirect_uri so the pasted browser URL
    // matches the token exchange (Google installed apps allow loopback ports).
    let listener = TcpListener::bind("127.0.0.1:0").context("bind loopback for redirect URI")?;
    let port = listener
        .local_addr()
        .context("read bound redirect port")?
        .port();
    drop(listener);
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let state = random_state();
    let auth_url = build_auth_url(client_id, &redirect_uri, &state)?;

    eprintln!();
    eprintln!("MANUAL LOGIN MODE");
    eprintln!("1. Copy this URL and open it in your browser:");
    eprintln!("{auth_url}");
    eprintln!();
    eprintln!("2. Sign in with your Google account.");
    eprintln!("3. You will be redirected to a localhost URL (it may fail to load).");
    eprintln!("4. Copy that ENTIRE localhost URL and paste it below.");
    eprintln!();
    eprint!("Paste the full redirect URL here: ");
    let _ = std::io::stderr().flush();

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .context("read pasted redirect URL from stdin")?;
    let pasted = line.trim();
    if pasted.is_empty() {
        bail!("No redirect URL pasted.");
    }
    let code = extract_code_from_redirect(pasted, &state)?;
    Ok((code, redirect_uri))
}

fn obtain_code_callback(client_id: &str, options: &LoginOptions) -> Result<(String, String)> {
    let listener = match options.port {
        Some(port) => TcpListener::bind(("127.0.0.1", port))
            .with_context(|| format!("bind OAuth callback on 127.0.0.1:{port}"))?,
        None => TcpListener::bind("127.0.0.1:0").context("bind OAuth callback on ephemeral port")?,
    };
    listener
        .set_nonblocking(true)
        .context("set callback listener non-blocking")?;
    let port = listener
        .local_addr()
        .context("read OAuth callback port")?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let state = random_state();
    let auth_url = build_auth_url(client_id, &redirect_uri, &state)?;

    eprintln!("Starting Google OAuth login for Antigravity native-auth.");
    eprintln!("Listening for callback on {redirect_uri}");
    eprintln!("Waiting up to {}s for browser sign-in…", options.timeout.as_secs());

    if options.no_browser {
        eprintln!();
        eprintln!("Open this URL in a browser:");
        eprintln!("{auth_url}");
    } else {
        match open_browser(&auth_url) {
            Ok(()) => eprintln!("Opened browser for Google login."),
            Err(error) => {
                eprintln!("Could not open browser automatically ({error}).");
                eprintln!("Open this URL manually:");
                eprintln!("{auth_url}");
            }
        }
    }

    let code = wait_for_callback_code(&listener, &state, options.timeout)?;
    Ok((code, redirect_uri))
}

fn wait_for_callback_code(
    listener: &TcpListener,
    expected_state: &str,
    timeout: Duration,
) -> Result<String> {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            bail!(
                "Timed out waiting for Google OAuth callback after {}s. \
                 Re-run with --manual if the browser redirect did not reach localhost.",
                timeout.as_secs()
            );
        }

        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut buf = [0u8; 8192];
                // Brief block so the request body/headers arrive.
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let first_line = request.lines().next().unwrap_or("");
                // e.g. GET /callback?code=...&state=... HTTP/1.1
                let path_and_query = first_line
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/");
                let full = format!("http://127.0.0.1{path_and_query}");

                match extract_code_from_redirect(&full, expected_state) {
                    Ok(code) => {
                        let body = "<!doctype html><html><body style=\"font-family:sans-serif;padding:2rem\">\
                            <h2>Signed in</h2>\
                            <p>You can close this tab and return to the terminal.</p>\
                            </body></html>";
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                        return Ok(code);
                    }
                    Err(error) => {
                        let message = html_escape(&error.to_string());
                        let body = format!(
                            "<!doctype html><html><body style=\"font-family:sans-serif;padding:2rem\">\
                             <h2>Login failed</h2><p>{message}</p>\
                             <p>You can close this tab and try again in the terminal.</p>\
                             </body></html>"
                        );
                        let response = format!(
                            "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                        // Keep listening for a later valid redirect.
                        eprintln!("Ignoring bad callback: {error}");
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(error).context("accept OAuth callback connection");
            }
        }
    }
}

fn extract_code_from_redirect(redirect_url: &str, expected_state: &str) -> Result<String> {
    let url = Url::parse(redirect_url.trim()).context("parse OAuth redirect URL")?;
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;
    let mut error_description: Option<String> = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = error {
        if let Some(description) = error_description {
            bail!("OAuth error: {error} ({description})");
        }
        bail!("OAuth error: {error}");
    }
    let code = code.ok_or_else(|| anyhow!("Redirect URL is missing code parameter"))?;
    let state = state.ok_or_else(|| anyhow!("Redirect URL is missing state parameter"))?;
    if state != expected_state {
        bail!("OAuth state mismatch (possible CSRF). Try logging in again.");
    }
    Ok(code)
}

fn exchange_code(
    http: &HttpClient,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<TokenExchangeResponse> {
    let body = format!(
        "code={}&client_id={}&client_secret={}&redirect_uri={}&grant_type=authorization_code",
        form_urlencode(code),
        form_urlencode(client_id),
        form_urlencode(client_secret),
        form_urlencode(redirect_uri),
    );
    let text = http
        .post_form(OAUTH_TOKEN_URL, &body)
        .context("exchange OAuth authorization code for tokens")?;
    serde_json::from_str(&text).with_context(|| {
        format!(
            "parse OAuth token exchange response (HTTP body {} bytes)",
            text.len()
        )
    })
}

fn fetch_email(http: &HttpClient, access_token: &str) -> Result<Option<String>> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {access_token}"))
            .context("invalid Authorization header")?,
    );
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    let text = http
        .fetch_text(OAUTH_USERINFO_URL, &headers)
        .context("fetch Google userinfo")?;
    let info: UserInfoResponse = serde_json::from_str(&text).context("parse Google userinfo")?;
    Ok(info.email.filter(|value| !value.is_empty()))
}

fn save_login_tokens(tokens: &mut StoredTokens) -> Result<()> {
    let config_dir = antigravity_usage_config_dir();
    fs::create_dir_all(&config_dir).with_context(|| format!("create {}", config_dir.display()))?;

    if let Some(email) = tokens.email.clone() {
        let dir = account_dir(&email);
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        let path = dir.join("tokens.json");
        write_tokens_file(&path, tokens)?;

        let now = Utc::now().to_rfc3339();
        let metadata = AccountMetadata {
            email: email.clone(),
            added_at: now.clone(),
            last_used: now,
        };
        let meta_path = dir.join("metadata.json");
        let raw = serde_json::to_string_pretty(&metadata).context("serialize account metadata")?;
        fs::write(&meta_path, raw).with_context(|| format!("write {}", meta_path.display()))?;
        // Best-effort restrictive perms on Unix.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _ = fs::set_permissions(&meta_path, fs::Permissions::from_mode(0o600));
        }

        write_active_account(Some(&email))?;
        eprintln!("Saved tokens for {email} under {}", dir.display());
    } else {
        let path = config_dir.join("tokens.json");
        write_tokens_file(&path, tokens)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        eprintln!("Saved tokens to {} (email unknown)", path.display());
    }
    Ok(())
}

fn write_tokens_file(path: &Path, tokens: &StoredTokens) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let raw = serde_json::to_string_pretty(tokens).context("serialize tokens")?;
    fs::write(path, raw).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn write_active_account(email: Option<&str>) -> Result<()> {
    let mut config = load_global_config();
    config.active_account = email.map(str::to_string);
    if config.version.is_empty() {
        config.version = default_config_version();
    }
    if config.preferences.cache_ttl == 0 {
        config.preferences.cache_ttl = default_cache_ttl();
    }
    save_global_config(&config)
}

fn load_global_config() -> GlobalConfig {
    let path = antigravity_usage_config_dir().join("config.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return GlobalConfig {
            version: default_config_version(),
            active_account: None,
            preferences: ConfigPreferences {
                cache_ttl: default_cache_ttl(),
            },
        };
    };
    serde_json::from_str(&raw).unwrap_or(GlobalConfig {
        version: default_config_version(),
        active_account: None,
        preferences: ConfigPreferences {
            cache_ttl: default_cache_ttl(),
        },
    })
}

fn save_global_config(config: &GlobalConfig) -> Result<()> {
    let dir = antigravity_usage_config_dir();
    fs::create_dir_all(&dir).ok();
    let path = dir.join("config.json");
    let raw = serde_json::to_string_pretty(config).context("serialize antigravity-usage config")?;
    fs::write(&path, raw).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn first_remaining_account() -> Option<String> {
    let accounts_dir = antigravity_usage_config_dir().join("accounts");
    let entries = fs::read_dir(accounts_dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let tokens = entry.path().join("tokens.json");
            if tokens.exists() {
                return Some(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn antigravity_usage_config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".config")
        })
        .join("antigravity-usage")
}

fn account_dir(email: &str) -> PathBuf {
    antigravity_usage_config_dir()
        .join("accounts")
        .join(sanitize_account_name(email))
}

fn sanitize_account_name(email: &str) -> String {
    email
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn open_browser(url: &str) -> Result<()> {
    let candidates = ["xdg-open", "gio", "kde-open6", "kde-open5", "sensible-browser"];
    for command in candidates {
        let mut cmd = Command::new(command);
        if command == "gio" {
            cmd.arg("open");
        }
        match cmd.arg(url).spawn() {
            Ok(_) => return Ok(()),
            Err(_) => continue,
        }
    }
    bail!("no browser opener found (tried xdg-open, gio, kde-open6, kde-open5, sensible-browser)");
}

fn random_state() -> String {
    // uuid is already a dependency of the crate.
    uuid::Uuid::new_v4().simple().to_string()
}

fn form_urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_code_and_validates_state() {
        let state = "abc123";
        let url = format!("http://127.0.0.1:1234/callback?code=tok&state={state}");
        let code = extract_code_from_redirect(&url, state).expect("code");
        assert_eq!(code, "tok");
    }

    #[test]
    fn rejects_state_mismatch() {
        let url = "http://127.0.0.1:1234/callback?code=tok&state=other";
        assert!(extract_code_from_redirect(url, "expected").is_err());
    }

    #[test]
    fn builds_auth_url_with_offline_consent() {
        let url = build_auth_url(
            "client.apps.googleusercontent.com",
            "http://127.0.0.1:9/callback",
            "state1",
        )
        .expect("auth url");
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("state=state1"));
    }
}
