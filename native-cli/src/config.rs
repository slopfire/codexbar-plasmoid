use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct RootConfig {
    #[serde(default)]
    providers: Vec<ProviderConfig>,
}

#[derive(Debug, Deserialize)]
struct ProviderConfig {
    id: String,
    #[serde(default)]
    cookie_header: Option<String>,
    #[serde(default)]
    cookie_source: Option<String>,
    #[serde(default)]
    workspace_id: Option<String>,
}

pub fn normalize_provider_id(provider_id: &str) -> String {
    let normalized = provider_id.trim().to_lowercase().replace(['-', '_'], "");
    static ALIASES: &[(&str, &str)] = &[
        ("azureopenai", "azureopenai"),
        ("alibabacodingplan", "alibaba"),
        ("opencodego", "opencodego"),
    ];
    for (from, to) in ALIASES {
        if normalized == *from {
            return (*to).to_string();
        }
    }
    normalized
}

pub fn config_path() -> PathBuf {
    env::var("CODEXBAR_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".codexbar")
                .join("config.json")
        })
}

fn load_config() -> RootConfig {
    let path = config_path();
    let Ok(raw) = fs::read_to_string(path) else {
        return RootConfig {
            providers: Vec::new(),
        };
    };
    serde_json::from_str(&raw).unwrap_or(RootConfig {
        providers: Vec::new(),
    })
}

fn provider_settings(provider_id: &str) -> Option<ProviderConfig> {
    let normalized = normalize_provider_id(provider_id);
    load_config()
        .providers
        .into_iter()
        .find(|item| normalize_provider_id(&item.id) == normalized)
}

pub fn cookie_env_name(provider_id: &str) -> Option<&'static str> {
    match normalize_provider_id(provider_id).as_str() {
        "cursor" => Some("CODEXBAR_PLASMOID_CURSOR_COOKIE"),
        "opencode" => Some("CODEXBAR_PLASMOID_OPENCODE_COOKIE"),
        "opencodego" => Some("CODEXBAR_PLASMOID_OPENCODEGO_COOKIE"),
        _ => None,
    }
}

pub fn fallback_cookie_env_name(provider_id: &str) -> Option<&'static str> {
    match normalize_provider_id(provider_id).as_str() {
        "cursor" => Some("SPLAZMA_CURSOR_COOKIE"),
        "opencode" => Some("SPLAZMA_OPENCODE_COOKIE"),
        "opencodego" => Some("SPLAZMA_OPENCODEGO_COOKIE"),
        _ => None,
    }
}

pub fn manual_cookie_header(provider_id: &str) -> Option<String> {
    if let Some(env_name) = cookie_env_name(provider_id) {
        if let Ok(value) = env::var(env_name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    if let Some(env_name) = fallback_cookie_env_name(provider_id) {
        if let Ok(value) = env::var(env_name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    let settings = provider_settings(provider_id)?;
    if settings.cookie_source.as_deref() == Some("off") {
        return None;
    }
    settings
        .cookie_header
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn workspace_id(provider_id: &str) -> Option<String> {
    if let Some(settings) = provider_settings(provider_id) {
        if let Some(id) = settings.workspace_id.as_deref().and_then(normalize_workspace_id) {
            return Some(id);
        }
    }
    let env_key = if normalize_provider_id(provider_id) == "opencodego" {
        "CODEXBAR_OPENCODEGO_WORKSPACE_ID"
    } else {
        "CODEXBAR_OPENCODE_WORKSPACE_ID"
    };
    env::var(env_key).ok().and_then(|value| normalize_workspace_id(&value))
}

pub fn normalize_workspace_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("wrk_") && trimmed.len() > 4 {
        return Some(trimmed.to_string());
    }
    if let Ok(url) = url::Url::parse(trimmed) {
        let parts: Vec<_> = url.path_segments()?.collect();
        if let Some(index) = parts.iter().position(|part| *part == "workspace") {
            if let Some(candidate) = parts.get(index + 1) {
                if candidate.starts_with("wrk_") {
                    return Some((*candidate).to_string());
                }
            }
        }
    }
    regex::Regex::new(r"wrk_[A-Za-z0-9]+")
        .ok()?
        .find(trimmed)
        .map(|m| m.as_str().to_string())
}
