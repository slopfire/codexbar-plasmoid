use crate::config::{manual_cookie_header, normalize_provider_id};
use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use anyhow::Result;
use cbc::Decryptor;
use rusqlite::Connection;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

type Aes128CbcDec = Decryptor<aes::Aes128>;

const CURSOR_DOMAINS: &[&str] = &[
    "cursor.com",
    "www.cursor.com",
    "cursor.sh",
    "authenticator.cursor.sh",
];
const CURSOR_COOKIE_NAMES: &[&str] = &[
    "WorkosCursorSessionToken",
    "__Secure-next-auth.session-token",
    "next-auth.session-token",
    "wos-session",
    "__Secure-wos-session",
    "authjs.session-token",
    "__Secure-authjs.session-token",
];
const OPENCODE_DOMAINS: &[&str] = &["opencode.ai", "app.opencode.ai"];
const OPENCODE_COOKIE_NAMES: &[&str] = &["auth", "__Host-auth"];

pub struct CookieResolution {
    pub header: String,
    pub source: String,
}

pub fn resolve_cookie_header(provider_id: &str) -> Result<CookieResolution> {
    if let Some(header) = manual_cookie_header(provider_id) {
        return Ok(CookieResolution {
            header: filter_provider_cookies(&header, provider_id),
            source: "config".to_string(),
        });
    }

    for store in browser_cookie_stores() {
        if let Some(header) = read_cookie_header_from_store(&store, provider_id, true) {
            return Ok(CookieResolution {
                header: filter_provider_cookies(&header, provider_id),
                source: store.label.clone(),
            });
        }
    }

    for store in browser_cookie_stores() {
        if let Some(header) = read_cookie_header_from_store(&store, provider_id, false) {
            return Ok(CookieResolution {
                header: filter_provider_cookies(&header, provider_id),
                source: store.label.clone(),
            });
        }
    }

    Err(anyhow::anyhow!(missing_cookie_message(provider_id)))
}

/// Resolve every distinct opencode.ai session cookie across all browser stores.
///
/// Unlike `resolve_cookie_header` (first match), this collects one entry per
/// distinct cookie value so a user logged into several OpenCode Go accounts
/// (e.g. different Zen/Chromium profiles) gets one payload per account.
/// Cookies are deduplicated by their filtered value so the same account found
/// in multiple profiles is fetched only once. A manual config cookie always
/// wins and is returned as the sole entry.
pub fn resolve_all_cookie_headers(provider_id: &str) -> Vec<CookieResolution> {
    if let Some(header) = manual_cookie_header(provider_id) {
        return vec![CookieResolution {
            header: filter_provider_cookies(&header, provider_id),
            source: "config".to_string(),
        }];
    }

    let stores = browser_cookie_stores();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // Strict name match first (proper `auth`/`__Host-auth` cookies).
    for store in &stores {
        if let Some(header) = read_cookie_header_from_store(store, provider_id, true) {
            let filtered = filter_provider_cookies(&header, provider_id);
            if !filtered.is_empty() && seen.insert(filtered.clone()) {
                out.push(CookieResolution {
                    header: filtered,
                    source: store.label.clone(),
                });
            }
        }
    }

    // Loose fallback for stores that had no strict match.
    for store in &stores {
        if let Some(header) = read_cookie_header_from_store(store, provider_id, false) {
            let filtered = filter_provider_cookies(&header, provider_id);
            if !filtered.is_empty() && seen.insert(filtered.clone()) {
                out.push(CookieResolution {
                    header: filtered,
                    source: store.label.clone(),
                });
            }
        }
    }

    out
}

pub fn filter_provider_cookies(header: &str, provider_id: &str) -> String {
    if normalize_provider_id(provider_id) == "cursor" {
        return header.to_string();
    }
    header
        .split(';')
        .map(str::trim)
        .filter_map(|part| {
            let (name, value) = part.split_once('=')?;
            if OPENCODE_COOKIE_NAMES.contains(&name.trim()) {
                Some(format!("{}={}", name.trim(), value.trim()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn missing_cookie_message(provider_id: &str) -> String {
    match normalize_provider_id(provider_id).as_str() {
        "cursor" => "No Cursor session found. Paste a Cookie header into ~/.codexbar/config.json, set CODEXBAR_PLASMOID_CURSOR_COOKIE, or log in to cursor.com in Chrome/Chromium.".to_string(),
        "opencode" => "No OpenCode session found. Paste a Cookie header into ~/.codexbar/config.json, set CODEXBAR_PLASMOID_OPENCODE_COOKIE, or log in to opencode.ai in Chrome/Chromium.".to_string(),
        "opencodego" => "No OpenCode Go session found. Paste a Cookie header into ~/.codexbar/config.json, set CODEXBAR_PLASMOID_OPENCODEGO_COOKIE, log in to opencode.ai, or use OpenCode Go locally.".to_string(),
        _ => "No session cookie found.".to_string(),
    }
}

struct CookieStore {
    browser: ChromiumBrowser,
    firefox: bool,
    label: String,
    cookies_path: PathBuf,
}

#[derive(Clone, Copy)]
enum ChromiumBrowser {
    Chrome,
    Chromium,
    Brave,
    Edge,
    Helium,
}

impl ChromiumBrowser {
    fn application_name(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::Brave => "brave",
            Self::Edge => "msedge",
            Self::Helium => "helium",
        }
    }

    fn kwallet_folder(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome Keys",
            Self::Chromium => "Chromium Keys",
            Self::Brave => "Brave Keys",
            Self::Edge => "Edge Keys",
            Self::Helium => "Helium Keys",
        }
    }

    fn kwallet_entry(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome Safe Storage",
            Self::Chromium => "Chromium Safe Storage",
            Self::Brave => "Brave Safe Storage",
            Self::Edge => "Edge Safe Storage",
            Self::Helium => "Helium Safe Storage",
        }
    }

    fn service_name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome Safe Storage",
            Self::Chromium => "Chromium Safe Storage",
            Self::Brave => "Brave Safe Storage",
            Self::Edge => "Edge Safe Storage",
            Self::Helium => "Helium Safe Storage",
        }
    }

    fn account_name(self) -> &'static str {
        match self {
            Self::Chrome => "Chrome",
            Self::Chromium => "Chromium",
            Self::Brave => "Brave",
            Self::Edge => "Edge",
            Self::Helium => "Helium",
        }
    }
}

fn browser_cookie_stores() -> Vec<CookieStore> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    let mut stores = Vec::new();

    let chromium_roots = [
        (home.join(".config/google-chrome"), ChromiumBrowser::Chrome, "Chrome"),
        (home.join(".config/chromium"), ChromiumBrowser::Chromium, "Chromium"),
        (
            home.join(".config/BraveSoftware/Brave-Browser"),
            ChromiumBrowser::Brave,
            "Brave",
        ),
        (
            home.join(".config/microsoft-edge"),
            ChromiumBrowser::Edge,
            "Edge",
        ),
        (home.join(".config/helium"), ChromiumBrowser::Helium, "Helium"),
    ];

    for (root, browser, label) in chromium_roots {
        if !root.exists() {
            continue;
        }
        for profile in list_chromium_profiles(&root) {
            let cookies_path = root.join(&profile).join("Cookies");
            if cookies_path.exists() {
                stores.push(CookieStore {
                    browser,
                    firefox: false,
                    label: format!("{label} ({profile})"),
                    cookies_path,
                });
            }
        }
    }

    let firefox_root = home.join(".mozilla/firefox");
    if firefox_root.exists() {
        if let Ok(entries) = fs::read_dir(&firefox_root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "Crash Reports" || name == "Pending Pings" {
                    continue;
                }
                let cookies_path = entry.path().join("cookies.sqlite");
                if cookies_path.exists() {
                    stores.push(CookieStore {
                        browser: ChromiumBrowser::Chrome,
                        firefox: true,
                        label: format!("Firefox ({name})"),
                        cookies_path,
                    });
                }
            }
        }
    }

    let zen_roots = [
        (home.join(".zen"), "Zen"),
        (home.join(".config/zen"), "Zen"),
        (
            home.join(".var/app/app.zen_browser.zen/zen"),
            "Zen Flatpak",
        ),
    ];
    for (root, label) in zen_roots {
        if !root.exists() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "Crash Reports" || name == "Pending Pings" {
                    continue;
                }
                let cookies_path = entry.path().join("cookies.sqlite");
                if cookies_path.exists() {
                    stores.push(CookieStore {
                        browser: ChromiumBrowser::Chrome,
                        firefox: true,
                        label: format!("{label} ({name})"),
                        cookies_path,
                    });
                }
            }
        }
    }

    stores
}

fn list_chromium_profiles(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return vec!["Default".to_string()];
    };
    let profiles: Vec<_> = entries
        .flatten()
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name == "Default" || name.starts_with("Profile "))
        .collect();
    if profiles.is_empty() {
        vec!["Default".to_string()]
    } else {
        profiles
    }
}

fn read_cookie_header_from_store(
    store: &CookieStore,
    provider_id: &str,
    strict_names: bool,
) -> Option<String> {
    if store.firefox {
        return read_firefox_cookies(&store.cookies_path, provider_id, strict_names);
    }
    let passwords = chromium_passwords(store.browser);
    read_chromium_cookies(&store.cookies_path, provider_id, &passwords, strict_names)
}

fn domains_for(provider_id: &str) -> &'static [&'static str] {
    if normalize_provider_id(provider_id) == "cursor" {
        CURSOR_DOMAINS
    } else {
        OPENCODE_DOMAINS
    }
}

fn names_for(provider_id: &str) -> &'static [&'static str] {
    if normalize_provider_id(provider_id) == "cursor" {
        CURSOR_COOKIE_NAMES
    } else {
        OPENCODE_COOKIE_NAMES
    }
}

fn domain_matches(host: &str, domains: &[&str]) -> bool {
    let normalized = host.trim_start_matches('.').to_lowercase();
    domains.iter().any(|domain| normalized == *domain || normalized.ends_with(&format!(".{domain}")))
}

fn read_chromium_cookies(
    cookies_path: &Path,
    provider_id: &str,
    passwords: &[String],
    strict_names: bool,
) -> Option<String> {
    let copied = copy_to_temp(cookies_path)?;
    let conn = Connection::open_with_flags(&copied, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let domains = domains_for(provider_id);
    let names = names_for(provider_id);
    let mut stmt = conn
        .prepare("SELECT host_key, name, encrypted_value, value FROM cookies WHERE host_key NOT LIKE '%.deleted'")
        .ok()?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .ok()?;

    let mut parts = Vec::new();
    for row in rows.flatten() {
        let (host, name, encrypted_value, plain_value) = row;
        if !domain_matches(&host, domains) {
            continue;
        }
        if strict_names && !names.contains(&name.as_str()) {
            continue;
        }
        let value = if !plain_value.is_empty() {
            plain_value
        } else {
            decrypt_chromium_value(&encrypted_value, passwords)?
        };
        if value.is_empty() {
            continue;
        }
        parts.push(format!("{name}={value}"));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn read_firefox_cookies(cookies_path: &Path, provider_id: &str, strict_names: bool) -> Option<String> {
    let copied = copy_to_temp(cookies_path)?;
    let conn = Connection::open_with_flags(&copied, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let domains = domains_for(provider_id);
    let names = names_for(provider_id);
    let mut stmt = conn
        .prepare("SELECT host, name, value FROM moz_cookies")
        .ok()?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .ok()?;

    let mut parts = Vec::new();
    for row in rows.flatten() {
        let (host, name, value) = row;
        if !domain_matches(&host, domains) || value.is_empty() {
            continue;
        }
        if strict_names && !names.contains(&name.as_str()) {
            continue;
        }
        parts.push(format!("{name}={value}"));
    }
    if parts.is_empty() && strict_names {
        return read_firefox_cookies(cookies_path, provider_id, false);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

fn chromium_passwords(browser: ChromiumBrowser) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut passwords = Vec::new();
    let mut push = |candidate: String| {
        let trimmed = candidate.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_string()) {
            return;
        }
        passwords.push(trimmed.to_string());
    };

    if let Some(value) = secret_tool_lookup(&[
        "lookup",
        "service",
        browser.service_name(),
        "account",
        browser.account_name(),
    ]) {
        push(value);
    }
    if let Some(value) = secret_tool_lookup(&["lookup", "application", browser.application_name()]) {
        push(value);
    }
    if let Some(value) = kwallet_password(browser) {
        push(value);
    }
    if matches!(browser, ChromiumBrowser::Helium) {
        // Helium is ungoogled-chromium-based and may reuse Chromium's keyring entries.
        if let Some(value) = secret_tool_lookup(&[
            "lookup",
            "service",
            "Chromium Safe Storage",
            "account",
            "Chromium",
        ]) {
            push(value);
        }
        if let Some(value) = secret_tool_lookup(&["lookup", "application", "chromium"]) {
            push(value);
        }
    }
    push("peanuts".to_string());
    push(String::new());

    passwords
}

fn secret_tool_lookup(args: &[&str]) -> Option<String> {
    let output = Command::new("secret-tool")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn kwallet_password(browser: ChromiumBrowser) -> Option<String> {
    let wallet = kwallet_name()?;
    let output = Command::new("kwallet-query")
        .args([
            "-r",
            browser.kwallet_entry(),
            "-f",
            browser.kwallet_folder(),
            &wallet,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn kwallet_name() -> Option<String> {
    for service in ["org.kde.kwalletd6", "org.kde.kwalletd5"] {
        let output = Command::new("qdbus")
            .args([service, "/modules/kwalletd5", "networkWallet"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            continue;
        }
        let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    Some("kdewallet".to_string())
}

fn decrypt_chromium_value(encrypted_value: &[u8], passwords: &[String]) -> Option<String> {
    if encrypted_value.len() < 4 {
        return None;
    }
    let prefix = &encrypted_value[..3];
    if prefix != b"v10" && prefix != b"v11" {
        return String::from_utf8(encrypted_value.to_vec()).ok();
    }

    let ciphertext = &encrypted_value[3..];
    if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return None;
    }

    let iv = [b' '; 16];
    for password in passwords {
        let key = derive_chromium_key(password);
        let mut key_array = [0u8; 16];
        key_array.copy_from_slice(&key);
        let cipher = Aes128CbcDec::new(&key_array.into(), &iv.into());
        let mut buffer = ciphertext.to_vec();
        let Ok(plain) = cipher.decrypt_padded_mut::<Pkcs7>(&mut buffer) else {
            continue;
        };
        if let Ok(text) = decode_chromium_plaintext(plain) {
            return Some(text);
        }
        if plain.len() > 32 {
            if let Ok(text) = String::from_utf8(plain[32..].to_vec()) {
                if looks_like_cookie_value(&text) {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn derive_chromium_key(password: &str) -> [u8; 16] {
    use pbkdf2::pbkdf2_hmac;
    use sha1::Sha1;
    let mut output = [0u8; 16];
    pbkdf2_hmac::<Sha1>(password.as_bytes(), b"saltysalt", 1, &mut output);
    output
}

fn decode_chromium_plaintext(plain: &[u8]) -> Result<String, ()> {
    if plain.len() > 32 {
        if let Ok(text) = String::from_utf8(plain[32..].to_vec()) {
            if looks_like_cookie_value(&text) {
                return Ok(text);
            }
        }
    }
    let text = String::from_utf8(plain.to_vec()).map_err(|_| ())?;
    if looks_like_cookie_value(&text) {
        Ok(text)
    } else {
        Err(())
    }
}

fn looks_like_cookie_value(value: &str) -> bool {
    !value.is_empty() && !value.bytes().any(|byte| byte < 9 || (byte > 13 && byte < 32))
}

fn copy_to_temp(path: &Path) -> Option<PathBuf> {
    let temp_dir = std::env::temp_dir().join(format!(
        "codexbar-plasmoid-cookies-{}",
        std::process::id()
    ));
    fs::create_dir_all(&temp_dir).ok()?;
    let copied = temp_dir.join(path.file_name()?);
    fs::copy(path, &copied).ok()?;
    Some(copied)
}
