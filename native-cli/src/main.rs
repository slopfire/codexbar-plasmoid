mod config;
mod cookies;
mod http;
mod opencodego_local;
mod output;
mod providers;

use crate::config::normalize_provider_id;
use crate::http::HttpClient;
use crate::output::ProviderPayload;
use crate::providers::{fetch_provider, NATIVE_PROVIDERS};
use std::env;
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let payload = vec![ProviderPayload::error("cli", error.to_string())];
            println!("{}", serde_json::to_string(&payload).unwrap_or_else(|_| "[]".to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || matches!(args[1].as_str(), "--help" | "-h" | "help") {
        print_help();
        return Ok(());
    }

    let command = args[1].as_str();
    if command == "cost" {
        println!("[]");
        return Ok(());
    }
    if command != "usage" {
        anyhow::bail!("Unknown command: {command}");
    }

    let parsed = parse_args(&args[2..]);
    if parsed.get("source").map(String::as_str) != Some("native") {
        anyhow::bail!("--source must be native for codexbar-plasmoid.");
    }

    let timeout_secs = parsed
        .get("web-timeout")
        .or(parsed.get("timeout"))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(45)
        .clamp(5, 300);
    let include_status = parsed.contains_key("status");
    let provider = parsed
        .get("provider")
        .map(|value| normalize_provider_id(value))
        .unwrap_or_else(|| "all".to_string());
    let anonymize = parsed
        .get("anonymize-emails")
        .or_else(|| parsed.get("anonymize-email"))
        .map(|value| value != "false")
        .unwrap_or(true);

    let timeout = Duration::from_secs(timeout_secs);
    let http = HttpClient::new(timeout)?;
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));

    let mut payloads: Vec<ProviderPayload> = if provider == "all" {
        NATIVE_PROVIDERS
            .iter()
            .flat_map(|provider_id| fetch_provider(provider_id, &http, &home, include_status, timeout))
            .collect::<Vec<_>>()
    } else if NATIVE_PROVIDERS.contains(&provider.as_str()) {
        fetch_provider(&provider, &http, &home, include_status, timeout)
    } else {
        anyhow::bail!("Provider not supported by codexbar-plasmoid: {provider}");
    };

    if anonymize {
        for payload in &mut payloads {
            payload.anonymize_emails();
        }
    }

    println!("{}", serde_json::to_string(&payloads)?);
    Ok(())
}

fn parse_args(args: &[String]) -> std::collections::HashMap<String, String> {
    let mut parsed = std::collections::HashMap::new();
    let mut index = 0;
    while index < args.len() {
        let token = &args[index];
        if !token.starts_with("--") {
            index += 1;
            continue;
        }
        let key = token.trim_start_matches("--").to_string();
        let next = args.get(index + 1);
        if next.is_none() || next.is_some_and(|value| value.starts_with("--")) {
            parsed.insert(key, "true".to_string());
        } else {
            parsed.insert(key, next.unwrap().clone());
            index += 1;
        }
        index += 1;
    }
    parsed
}

fn print_help() {
    println!(
        "codexbar-plasmoid — Linux-native usage fetcher for Antigravity, Cursor, OpenCode, and OpenCode Go

Usage:
  codexbar-plasmoid usage --format json --json-only --provider <id> --source native [--status] [--web-timeout <seconds>]

Providers:
  antigravity, cursor, opencode, opencodego, all

Authentication:
  - Antigravity: running agy or Antigravity IDE language server (local HTTPS probe)
  - ~/.codexbar/config.json provider cookieHeader
  - CODEXBAR_PLASMOID_CURSOR_COOKIE / CODEXBAR_PLASMOID_OPENCODE_COOKIE / CODEXBAR_PLASMOID_OPENCODEGO_COOKIE (or older SPLAZMA_* fallback)
  - Chrome/Chromium/Helium/Firefox/Zen cookie import (secret-tool required for encrypted Chromium cookies)
  - OpenCode Go local usage from ~/.local/share/opencode/opencode.db"
    );
}
