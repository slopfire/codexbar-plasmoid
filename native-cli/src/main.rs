mod antigravity_auth;
mod antigravity_oauth_discover;
mod config;
mod cookies;
mod http;
mod local_cost;
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
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("help");

    // Interactive auth commands print human-readable text (not usage JSON).
    if matches!(command, "login" | "logout") {
        return match run_auth_command(command, &args[2..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error:#}");
                ExitCode::FAILURE
            }
        };
    }

    match run_usage(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let payload = vec![ProviderPayload::error("cli", error.to_string())];
            println!("{}", serde_json::to_string(&payload).unwrap_or_else(|_| "[]".to_string()));
            ExitCode::FAILURE
        }
    }
}

fn run_auth_command(command: &str, args: &[String]) -> anyhow::Result<()> {
    let parsed = parse_args(args);
    let provider = parsed
        .get("provider")
        .map(|value| normalize_provider_id(value))
        .unwrap_or_else(|| "antigravity".to_string());
    if provider != "antigravity" {
        anyhow::bail!(
            "Browser OAuth login is only implemented for --provider antigravity (got {provider})."
        );
    }

    match command {
        "login" => {
            let timeout_secs = parsed
                .get("timeout")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(300)
                .clamp(30, 900);
            let options = antigravity_auth::LoginOptions {
                manual: parsed.contains_key("manual"),
                no_browser: parsed.contains_key("no-browser"),
                port: parsed
                    .get("port")
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|port| *port > 0),
                timeout: Duration::from_secs(timeout_secs),
            };
            // Token exchange is quick; the long wait is the localhost callback.
            let http = HttpClient::new(Duration::from_secs(45))?;
            let email = antigravity_auth::login(&http, options)?;
            match email {
                Some(email) => println!("Logged in as {email}."),
                None => println!("Logged in (email unknown)."),
            }
            println!("Native Auth will use tokens under ~/.config/antigravity-usage.");
            Ok(())
        }
        "logout" => {
            let all = parsed.contains_key("all");
            let email = parsed.get("account").map(String::as_str);
            let message = antigravity_auth::logout(email, all)?;
            println!("{message}");
            Ok(())
        }
        _ => unreachable!(),
    }
}

fn run_usage(args: &[String]) -> anyhow::Result<()> {
    if args.len() < 2 || matches!(args[1].as_str(), "--help" | "-h" | "help") {
        print_help();
        return Ok(());
    }

    let command = args[1].as_str();
    if command == "cost" {
        return run_cost(&args[2..]);
    }
    if command != "usage" {
        anyhow::bail!("Unknown command: {command}");
    }

    let parsed = parse_args(&args[2..]);
    let source = parsed
        .get("source")
        .map(String::as_str)
        .unwrap_or("native");
    if source != "native" && source != "native-auth" {
        anyhow::bail!("--source must be native or native-auth for codexbar-plasmoid.");
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
            .flat_map(|provider_id| {
                fetch_provider(provider_id, &http, &home, include_status, timeout, source)
            })
            .collect::<Vec<_>>()
    } else if NATIVE_PROVIDERS.contains(&provider.as_str()) {
        fetch_provider(&provider, &http, &home, include_status, timeout, source)
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

fn run_cost(args: &[String]) -> anyhow::Result<()> {
    let parsed = parse_args(args);
    let provider = parsed
        .get("provider")
        .map(|value| normalize_provider_id(value))
        .unwrap_or_else(|| "all".to_string());
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
    let timeout_secs = parsed
        .get("web-timeout")
        .or(parsed.get("timeout"))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(45)
        .clamp(5, 300);
    let http = HttpClient::new(Duration::from_secs(timeout_secs))?;

    if !local_cost::supports_cost(&provider) {
        // Match upstream codexbar's graceful empty result for unsupported ids
        // when the helper probes the native binary first.
        // Devin / Antigravity only expose quota percentages — no token history.
        println!("[]");
        return Ok(());
    }

    match local_cost::fetch_costs(&provider, &home, &http) {
        Ok(snapshots) => {
            println!("{}", serde_json::to_string(&snapshots)?);
            Ok(())
        }
        Err(error) => {
            // Soft-fail so a missing OpenCode install / cookie does not fail
            // the whole multi-provider cost pass.
            let payload = serde_json::json!([{
                "provider": "cost",
                "error": {
                    "code": 1,
                    "kind": "provider",
                    "message": error.to_string(),
                }
            }]);
            println!("{}", payload);
            Ok(())
        }
    }
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
        "codexbar-plasmoid — Linux-native usage fetcher for Antigravity, Cursor, Devin, Grok, OpenCode, and OpenCode Go

Usage:
  codexbar-plasmoid usage --format json --json-only --provider <id> --source native|native-auth [--status] [--web-timeout <seconds>]
  codexbar-plasmoid cost --format json --json-only --provider <id>
  codexbar-plasmoid login --provider antigravity [--manual] [--no-browser] [--port <n>] [--timeout <seconds>]
  codexbar-plasmoid logout --provider antigravity [--account <email>] [--all]

Providers:
  antigravity, cursor, devin, grok, opencode, opencodego, all

Cost (token spend / API $):
  opencode, opencodego — ~/.local/share/opencode/*.db
  cursor — dashboard usage events (session cookie)
  grok — ~/.grok/sessions/**/updates.jsonl (costUsdTicks or per-model API rates)
  all — soft-merge of the above
  (antigravity / devin: quota % only, no absolute token history)

Authentication:
  - Antigravity (--source native): running agy/IDE first, then Cloud Code OAuth fallback
  - Antigravity (--source native-auth): Cloud Code API using tokens under
    ~/.config/antigravity-usage from browser OAuth:
      codexbar-plasmoid login --provider antigravity
    (compatible with tokens from `antigravity-usage login`)
  - Browser login and token refresh use the desktop OAuth *app* client from the
    local `agy` binary (override: ANTIGRAVITY_OAUTH_CLIENT_ID/SECRET or oauth_client_*)
  - ~/.codexbar/config.json provider cookieHeader / oauth_client_*
  - CODEXBAR_PLASMOID_CURSOR_COOKIE / CODEXBAR_PLASMOID_OPENCODE_COOKIE / CODEXBAR_PLASMOID_OPENCODEGO_COOKIE (or older SPLAZMA_* fallback)
  - Chrome/Chromium/Helium/Firefox/Zen cookie import (secret-tool required for encrypted Chromium cookies)
  - OpenCode Go subscription rate limits from opencode.ai session cookies
  - OpenCode / OpenCode Go local token spend from ~/.local/share/opencode/*.db (cost)"
    );
}
