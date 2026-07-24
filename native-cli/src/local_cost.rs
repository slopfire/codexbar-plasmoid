//! Token + API cost snapshots for native providers.
//!
//! Sources:
//! - OpenCode / OpenCode Go: SQLite under `~/.local/share/opencode/`
//! - Cursor: dashboard usage events (`get-filtered-usage-events`) via session cookie
//! - Grok: local session `updates.jsonl` turn_completed usage (`costUsdTicks` when
//!   present; otherwise API-equivalent $ from per-model token rates)
//!
//! Antigravity and Devin only expose quota *percentages* — no absolute token/cost
//! history is available to aggregate into this shape.

use crate::cookies::resolve_cookie_header;
use crate::http::{cookie_header, HttpClient};
use crate::opencodego_local::local_paths;
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use reqwest::header::{HeaderValue, ORIGIN, REFERER};
use rusqlite::Connection;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostSnapshot {
    pub provider: String,
    pub source: String,
    // Match upstream codexbar cost keys (sessionCostUSD / last30DaysCostUSD).
    #[serde(rename = "sessionCostUSD")]
    pub session_cost_usd: f64,
    pub session_tokens: i64,
    #[serde(rename = "last30DaysCostUSD")]
    pub last30_days_cost_usd: f64,
    #[serde(rename = "last30DaysTokens")]
    pub last30_days_tokens: i64,
    pub currency_code: String,
    pub session_label: String,
    #[serde(rename = "last30DaysLabel")]
    pub last30_days_label: String,
    pub updated_at: DateTime<Utc>,
    pub daily: Vec<DailyCostPoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCostPoint {
    pub date: String,
    pub total_cost: f64,
    pub total_tokens: i64,
    #[serde(rename = "modelBreakdowns")]
    pub model_breakdowns: Vec<ModelCostPoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostPoint {
    pub model_name: String,
    pub cost: f64,
    pub total_tokens: i64,
}

#[derive(Clone)]
struct CostRow {
    created_ms: i64,
    cost: f64,
    tokens: i64,
    model: Option<String>,
}

/// Providers the native binary can produce cost/token history for.
pub fn supports_cost(provider: &str) -> bool {
    matches!(
        provider,
        "opencode" | "opencodego" | "cursor" | "grok" | "all"
    )
}

pub fn fetch_costs(provider: &str, home: &Path, http: &HttpClient) -> Result<Vec<CostSnapshot>> {
    match provider {
        "all" => {
            let mut out = Vec::new();
            if let Ok(snapshot) = fetch_opencode_cost("opencode", home, CostScope::All) {
                out.push(snapshot);
            }
            if let Ok(snapshot) = fetch_opencode_cost("opencodego", home, CostScope::OpenCodeGo) {
                out.push(snapshot);
            }
            if let Ok(snapshot) = fetch_cursor_cost(http) {
                out.push(snapshot);
            }
            if let Ok(snapshot) = fetch_grok_cost(home) {
                out.push(snapshot);
            }
            Ok(out)
        }
        "opencode" => Ok(vec![fetch_opencode_cost("opencode", home, CostScope::All)?]),
        "opencodego" => Ok(vec![fetch_opencode_cost(
            "opencodego",
            home,
            CostScope::OpenCodeGo,
        )?]),
        "cursor" => Ok(vec![fetch_cursor_cost(http)?]),
        "grok" => Ok(vec![fetch_grok_cost(home)?]),
        other => Err(anyhow!(
            "Native cost is supported for opencode, opencodego, cursor, and grok (got {other})."
        )),
    }
}

#[derive(Clone, Copy)]
enum CostScope {
    /// Every assistant message in the local OpenCode database.
    All,
    /// Only messages billed to the OpenCode Go provider id.
    OpenCodeGo,
}

fn fetch_opencode_cost(provider: &str, home: &Path, scope: CostScope) -> Result<CostSnapshot> {
    let (_, database_path) = local_paths(home);
    if !database_path.exists() {
        return Err(anyhow!(
            "OpenCode database not found under ~/.local/share/opencode/."
        ));
    }

    let rows = read_cost_rows(&database_path, scope)?;
    if rows.is_empty() {
        return Err(anyhow!(
            "No local token/cost history found for {provider} in OpenCode database."
        ));
    }

    Ok(snapshot_from_rows(provider, "local", &rows, Utc::now()))
}

fn read_cost_rows(database_path: &Path, scope: CostScope) -> Result<Vec<CostRow>> {
    let conn =
        Connection::open_with_flags(database_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let sql = match scope {
        CostScope::All => MESSAGE_COST_SQL_ALL,
        CostScope::OpenCodeGo => MESSAGE_COST_SQL_OPENCODE_GO,
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        let model: Option<String> = row.get::<_, Option<String>>(3)?;
        Ok(CostRow {
            created_ms: row.get(0)?,
            cost: row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
            tokens: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            model: model.and_then(|value| {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }),
        })
    })?;
    Ok(rows
        .flatten()
        .filter(|row| {
            row.created_ms > 0
                && row.cost.is_finite()
                && row.cost >= 0.0
                && (row.cost > 0.0 || row.tokens > 0)
        })
        .collect())
}

// --- Cursor dashboard events -------------------------------------------------

const CURSOR_EVENTS_URL: &str = "https://cursor.com/api/dashboard/get-filtered-usage-events";
const CURSOR_PAGE_SIZE: i64 = 200;
const CURSOR_MAX_PAGES: i64 = 40;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorUsageEventsResponse {
    total_usage_events_count: Option<i64>,
    usage_events_display: Option<Vec<CursorUsageEvent>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorUsageEvent {
    timestamp: Option<serde_json::Value>,
    charged_cents: Option<f64>,
    usage_based_costs: Option<String>,
    token_usage: Option<CursorTokenUsage>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    underlying_model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorTokenUsage {
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    /// Cache reads are high volume but usually discounted; not counted in tokens.
    #[allow(dead_code)]
    cache_read_tokens: Option<i64>,
    total_cents: Option<f64>,
}

fn fetch_cursor_cost(http: &HttpClient) -> Result<CostSnapshot> {
    let cookie = resolve_cookie_header("cursor").context("resolve Cursor session cookie")?;
    let mut headers = cookie_header(&cookie.header)?;
    headers.insert(ORIGIN, HeaderValue::from_static("https://cursor.com"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://cursor.com/dashboard"),
    );
    // Dashboard expects browser-ish Accept; cookie_header already sets application/json.

    let now = Utc::now();
    let end_ms = now.timestamp_millis();
    let start_ms = end_ms - 30 * 24 * 60 * 60 * 1000;

    let mut rows: Vec<CostRow> = Vec::new();
    let mut page: i64 = 1;
    let mut total_hint: Option<i64> = None;

    loop {
        let body = serde_json::json!({
            "pageSize": CURSOR_PAGE_SIZE,
            "page": page,
            "startDate": start_ms,
            "endDate": end_ms,
        });
        let payload = serde_json::to_vec(&body).context("encode Cursor usage events body")?;
        let text = http
            .post_text(CURSOR_EVENTS_URL, &headers, &payload)
            .with_context(|| format!("POST {CURSOR_EVENTS_URL} page {page}"))?;
        let response: CursorUsageEventsResponse =
            serde_json::from_str(&text).context("parse Cursor usage events JSON")?;
        if total_hint.is_none() {
            total_hint = response.total_usage_events_count;
        }
        let events = response.usage_events_display.unwrap_or_default();
        if events.is_empty() {
            break;
        }
        for event in &events {
            if let Some(row) = cost_row_from_cursor_event(event) {
                rows.push(row);
            }
        }
        let fetched = (page * CURSOR_PAGE_SIZE) as i64;
        let done = events.len() < CURSOR_PAGE_SIZE as usize
            || total_hint.is_some_and(|total| fetched >= total)
            || page >= CURSOR_MAX_PAGES;
        if done {
            break;
        }
        page += 1;
    }

    if rows.is_empty() {
        return Err(anyhow!(
            "No Cursor usage events found for the last 30 days (dashboard API)."
        ));
    }

    Ok(snapshot_from_rows("cursor", "dashboard", &rows, now))
}

fn cost_row_from_cursor_event(event: &CursorUsageEvent) -> Option<CostRow> {
    let created_ms = parse_event_timestamp(event.timestamp.as_ref())?;
    let tokens = event
        .token_usage
        .as_ref()
        .map(|usage| {
            usage.input_tokens.unwrap_or(0).max(0) + usage.output_tokens.unwrap_or(0).max(0)
        })
        .unwrap_or(0);
    let cost = cursor_event_cost_usd(event);
    if cost <= 0.0 && tokens <= 0 {
        return None;
    }
    let model = event
        .model
        .as_deref()
        .or(event.model_name.as_deref())
        .or(event.underlying_model.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    Some(CostRow {
        created_ms,
        cost,
        tokens,
        model,
    })
}

fn cursor_event_cost_usd(event: &CursorUsageEvent) -> f64 {
    if let Some(cents) = event.charged_cents {
        if cents.is_finite() && cents > 0.0 {
            return cents / 100.0;
        }
    }
    if let Some(usage) = event.token_usage.as_ref() {
        if let Some(cents) = usage.total_cents {
            if cents.is_finite() && cents > 0.0 {
                return cents / 100.0;
            }
        }
    }
    parse_dollar_amount(event.usage_based_costs.as_deref()).unwrap_or(0.0)
}

fn parse_dollar_amount(raw: Option<&str>) -> Option<f64> {
    let raw = raw?.trim();
    if raw.is_empty() || raw == "-" {
        return None;
    }
    let cleaned = raw.trim_start_matches('$').replace(',', "");
    let value: f64 = cleaned.parse().ok()?;
    if value.is_finite() && value >= 0.0 {
        Some(value)
    } else {
        None
    }
}

fn parse_event_timestamp(value: Option<&serde_json::Value>) -> Option<i64> {
    let value = value?;
    if let Some(number) = value.as_i64() {
        return normalize_epoch_ms(number as f64);
    }
    if let Some(number) = value.as_f64() {
        return normalize_epoch_ms(number);
    }
    if let Some(raw) = value.as_str() {
        let trimmed = raw.trim();
        if let Ok(number) = trimmed.parse::<f64>() {
            return normalize_epoch_ms(number);
        }
        if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
            return Some(dt.with_timezone(&Utc).timestamp_millis());
        }
    }
    None
}

/// Accept seconds or milliseconds epoch values.
fn normalize_epoch_ms(number: f64) -> Option<i64> {
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    // Distinguishes ms (≥ ~year 2001 in ms) from seconds.
    let ms = if number > 10_000_000_000.0 {
        number
    } else {
        number * 1000.0
    };
    Some(ms as i64)
}

// --- Grok local sessions -----------------------------------------------------

/// Grok session `costUsdTicks` scale: $1.00 == 10_000_000_000 ticks.
/// Verified against modelUsage token breakdown at xAI list rates ($2/$0.30/$6 for grok-4.5).
const GROK_COST_USD_TICKS_PER_DOLLAR: f64 = 10_000_000_000.0;

/// USD per 1M tokens for a model family (input / cached input / output).
#[derive(Clone, Copy)]
struct GrokModelRates {
    input_per_m: f64,
    cached_per_m: f64,
    output_per_m: f64,
}

fn fetch_grok_cost(home: &Path) -> Result<CostSnapshot> {
    let sessions_root = grok_sessions_dir(home);
    if !sessions_root.is_dir() {
        return Err(anyhow!(
            "Grok sessions directory not found under {}.",
            sessions_root.display()
        ));
    }

    let mut rows = Vec::new();
    collect_grok_updates(&sessions_root, &mut rows);
    if rows.is_empty() {
        return Err(anyhow!(
            "No Grok turn_completed usage found under {}.",
            sessions_root.display()
        ));
    }

    // Prefer costUsdTicks (API-billed). When ticks are 0 (SuperGrok free tier),
    // fall back to per-model API list rates so the UI still shows spend-equivalent $.
    Ok(snapshot_from_rows("grok", "local", &rows, Utc::now()))
}

fn grok_sessions_dir(home: &Path) -> PathBuf {
    std::env::var("GROK_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".grok"))
        .join("sessions")
}

fn collect_grok_updates(dir: &Path, out: &mut Vec<CostRow>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_grok_updates(&path, out);
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) != Some("updates.jsonl") {
            continue;
        }
        if let Ok(rows) = read_grok_updates_file(&path) {
            out.extend(rows);
        }
    }
}

fn read_grok_updates_file(path: &Path) -> Result<Vec<CostRow>> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut rows = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else {
            continue;
        };
        if !line.contains("turn_completed") || !line.contains("usage") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        rows.extend(cost_rows_from_grok_update(&value));
    }
    Ok(rows)
}

fn cost_rows_from_grok_update(value: &serde_json::Value) -> Vec<CostRow> {
    let Some(params) = value.get("params") else {
        return Vec::new();
    };
    let Some(update) = params.get("update") else {
        return Vec::new();
    };
    if update.get("sessionUpdate").and_then(|v| v.as_str()) != Some("turn_completed") {
        return Vec::new();
    }
    let Some(usage) = update.get("usage") else {
        return Vec::new();
    };
    let Some(created_ms) = parse_event_timestamp(value.get("timestamp"))
        .or_else(|| parse_event_timestamp(value.get("ts")))
        .or_else(|| {
            // Nested agent clock when top-level timestamp is missing.
            params
                .pointer("/_meta/agentTimestampMs")
                .and_then(|v| v.as_f64())
                .and_then(normalize_epoch_ms)
        })
    else {
        return Vec::new();
    };

    // Prefer per-model rows when modelUsage is present so the chart can break
    // spend down by model on hover.
    if let Some(map) = usage.get("modelUsage").and_then(|v| v.as_object()) {
        if !map.is_empty() {
            let mut rows = Vec::new();
            for (model_id, model_usage) in map {
                let tokens = grok_usage_total_tokens(model_usage);
                let cost = if let Some(cost) = grok_ticks_to_usd(model_usage.get("costUsdTicks")) {
                    cost
                } else {
                    estimate_grok_model_cost(model_id, model_usage)
                };
                if tokens <= 0 && cost <= 0.0 {
                    continue;
                }
                rows.push(CostRow {
                    created_ms,
                    cost,
                    tokens,
                    model: Some(model_id.clone()),
                });
            }
            if !rows.is_empty() {
                // If only top-level ticks are billed, re-scale model costs so
                // they still sum to the turn total when ticks are non-zero.
                if let Some(total_cost) = grok_ticks_to_usd(usage.get("costUsdTicks")) {
                    let model_sum: f64 = rows.iter().map(|row| row.cost).sum();
                    if model_sum > 0.0 && (model_sum - total_cost).abs() > 1e-9 {
                        let scale = total_cost / model_sum;
                        for row in &mut rows {
                            row.cost = round_money(row.cost * scale);
                        }
                    }
                }
                return rows;
            }
        }
    }

    let tokens = grok_usage_total_tokens(usage);
    let cost = grok_usage_cost_usd(usage);
    if tokens <= 0 && cost <= 0.0 {
        return Vec::new();
    }
    vec![CostRow {
        created_ms,
        cost,
        tokens,
        model: Some("grok-4.5".to_string()),
    }]
}

fn grok_usage_total_tokens(usage: &serde_json::Value) -> i64 {
    usage
        .get("totalTokens")
        .and_then(json_i64)
        .or_else(|| {
            let input = usage.get("inputTokens").and_then(json_i64).unwrap_or(0);
            let output = usage.get("outputTokens").and_then(json_i64).unwrap_or(0);
            // totalTokens already equals input+output; reasoning is part of output.
            Some(input + output)
        })
        .unwrap_or(0)
        .max(0)
}

/// Resolve USD for one turn_completed usage blob.
///
/// 1. Top-level `costUsdTicks` when > 0 (matches API billing).
/// 2. Else sum per-model ticks / estimated rates from `modelUsage`.
/// 3. Else estimate from top-level token fields with default (grok-4.5) rates.
fn grok_usage_cost_usd(usage: &serde_json::Value) -> f64 {
    if let Some(cost) = grok_ticks_to_usd(usage.get("costUsdTicks")) {
        return cost;
    }

    if let Some(map) = usage.get("modelUsage").and_then(|v| v.as_object()) {
        let mut total = 0.0;
        for (model_id, model_usage) in map {
            if let Some(cost) = grok_ticks_to_usd(model_usage.get("costUsdTicks")) {
                total += cost;
            } else {
                total += estimate_grok_model_cost(model_id, model_usage);
            }
        }
        if total > 0.0 {
            return round_money(total);
        }
    }

    round_money(estimate_grok_model_cost("grok-4.5", usage))
}

fn grok_ticks_to_usd(value: Option<&serde_json::Value>) -> Option<f64> {
    let ticks = json_f64(value?)?;
    if !ticks.is_finite() || ticks <= 0.0 {
        return None;
    }
    Some(round_money(ticks / GROK_COST_USD_TICKS_PER_DOLLAR))
}

fn estimate_grok_model_cost(model_id: &str, usage: &serde_json::Value) -> f64 {
    let rates = grok_model_rates(model_id);
    let input = usage.get("inputTokens").and_then(json_i64).unwrap_or(0).max(0) as f64;
    let output = usage.get("outputTokens").and_then(json_i64).unwrap_or(0).max(0) as f64;
    let cached = usage
        .get("cachedReadTokens")
        .and_then(json_i64)
        .unwrap_or(0)
        .max(0) as f64;
    // When only totalTokens is present (older events), treat as uncached input.
    let (input, output, cached) = if input <= 0.0 && output <= 0.0 {
        let total = usage.get("totalTokens").and_then(json_i64).unwrap_or(0).max(0) as f64;
        (total, 0.0, 0.0)
    } else {
        (input, output, cached)
    };
    let uncached_input = (input - cached).max(0.0);
    let cost = uncached_input * rates.input_per_m / 1_000_000.0
        + cached * rates.cached_per_m / 1_000_000.0
        + output * rates.output_per_m / 1_000_000.0;
    round_money(cost)
}

/// API list rates (USD / 1M tokens). Free-tier model ids use the same rates so
/// SuperGrok usage still reports API-equivalent spend for tracking.
fn grok_model_rates(model_id: &str) -> GrokModelRates {
    let id = model_id.to_ascii_lowercase();
    // Strip optional "-free" suffix used by subscription-billed variants.
    let id = id.strip_suffix("-free").unwrap_or(&id);

    // Flagship Grok 4.5 (+ build variant; matches observed costUsdTicks).
    if id.starts_with("grok-4.5") || id.starts_with("grok-4-5") {
        return GrokModelRates {
            input_per_m: 2.0,
            cached_per_m: 0.30,
            output_per_m: 6.0,
        };
    }
    // Grok 4.3 / 4.20 multi-agent / dated SKUs.
    if id.starts_with("grok-4.3")
        || id.starts_with("grok-4-3")
        || id.starts_with("grok-4.20")
        || id.starts_with("grok-4-20")
        || id.contains("multi-agent")
    {
        return GrokModelRates {
            input_per_m: 1.25,
            cached_per_m: 0.20,
            output_per_m: 2.50,
        };
    }
    // Fast / volume tiers.
    if id.contains("fast") || id.contains("code-fast") || id.contains("composer") {
        return GrokModelRates {
            input_per_m: 0.20,
            cached_per_m: 0.05,
            output_per_m: 0.50,
        };
    }
    // Build / coding-oriented mid tier.
    if id.contains("build") {
        return GrokModelRates {
            input_per_m: 1.0,
            cached_per_m: 0.20,
            output_per_m: 2.0,
        };
    }
    // Default: current flagship rates.
    GrokModelRates {
        input_per_m: 2.0,
        cached_per_m: 0.30,
        output_per_m: 6.0,
    }
}

fn json_i64(value: &serde_json::Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))
        .or_else(|| value.as_f64().map(|n| n as i64))
}

fn json_f64(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| value.as_u64().map(|n| n as f64))
}

// --- Aggregation -------------------------------------------------------------

fn snapshot_from_rows(
    provider: &str,
    source: &str,
    rows: &[CostRow],
    now: DateTime<Utc>,
) -> CostSnapshot {
    let now_ms = now.timestamp_millis();
    let day_start = start_of_utc_day(now).timestamp_millis();
    let window_start = now_ms - 30 * 24 * 60 * 60 * 1000;

    let mut session_cost = 0.0;
    let mut session_tokens: i64 = 0;
    let mut last30_cost = 0.0;
    let mut last30_tokens: i64 = 0;
    // day -> (total_cost, total_tokens, model -> (cost, tokens))
    let mut daily: BTreeMap<String, (f64, i64, BTreeMap<String, (f64, i64)>)> = BTreeMap::new();

    for row in rows {
        if row.created_ms >= day_start && row.created_ms <= now_ms {
            session_cost += row.cost;
            session_tokens += row.tokens;
        }
        if row.created_ms >= window_start && row.created_ms <= now_ms {
            last30_cost += row.cost;
            last30_tokens += row.tokens;
            let day_key = Utc
                .timestamp_millis_opt(row.created_ms)
                .single()
                .map(|dt| dt.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let entry = daily.entry(day_key).or_insert_with(|| (0.0, 0, BTreeMap::new()));
            entry.0 += row.cost;
            entry.1 += row.tokens;
            if let Some(model) = row.model.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
                let model_entry = entry.2.entry(model.to_string()).or_insert((0.0, 0));
                model_entry.0 += row.cost;
                model_entry.1 += row.tokens;
            }
        }
    }

    let daily_points: Vec<DailyCostPoint> = daily
        .into_iter()
        .map(|(date, (total_cost, total_tokens, models))| {
            let mut model_breakdowns: Vec<ModelCostPoint> = models
                .into_iter()
                .map(|(model_name, (cost, tokens))| ModelCostPoint {
                    model_name,
                    cost: round_money(cost),
                    total_tokens: tokens,
                })
                .collect();
            model_breakdowns.sort_by(|a, b| {
                b.cost
                    .partial_cmp(&a.cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.total_tokens.cmp(&a.total_tokens))
                    .then_with(|| a.model_name.cmp(&b.model_name))
            });
            DailyCostPoint {
                date,
                total_cost: round_money(total_cost),
                total_tokens,
                model_breakdowns,
            }
        })
        .collect();

    CostSnapshot {
        provider: provider.to_string(),
        source: source.to_string(),
        session_cost_usd: round_money(session_cost),
        session_tokens,
        last30_days_cost_usd: round_money(last30_cost),
        last30_days_tokens: last30_tokens,
        currency_code: "USD".to_string(),
        session_label: "Today".to_string(),
        last30_days_label: "30d".to_string(),
        updated_at: now,
        daily: daily_points,
    }
}

fn start_of_utc_day(now: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .unwrap_or(now)
}

fn round_money(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    (value * 1_000_000.0).round() / 1_000_000.0
}

const MESSAGE_COST_SQL_ALL: &str = r#"
SELECT
  CAST(COALESCE(json_extract(data, '$.time.created'), time_created) AS INTEGER) AS createdMs,
  CAST(json_extract(data, '$.cost') AS REAL) AS cost,
  CAST(COALESCE(json_extract(data, '$.tokens.total'), 0) AS INTEGER) AS tokens,
  COALESCE(json_extract(data, '$.modelID'), json_extract(data, '$.model'), json_extract(data, '$.modelName')) AS model
FROM message
WHERE json_valid(data)
  AND json_extract(data, '$.role') = 'assistant'
  AND (
    json_type(data, '$.cost') IN ('integer', 'real')
    OR json_type(data, '$.tokens.total') IN ('integer', 'real')
  )
"#;

const MESSAGE_COST_SQL_OPENCODE_GO: &str = r#"
SELECT
  CAST(COALESCE(json_extract(data, '$.time.created'), time_created) AS INTEGER) AS createdMs,
  CAST(json_extract(data, '$.cost') AS REAL) AS cost,
  CAST(COALESCE(json_extract(data, '$.tokens.total'), 0) AS INTEGER) AS tokens,
  COALESCE(json_extract(data, '$.modelID'), json_extract(data, '$.model'), json_extract(data, '$.modelName')) AS model
FROM message
WHERE json_valid(data)
  AND json_extract(data, '$.role') = 'assistant'
  AND json_extract(data, '$.providerID') = 'opencode-go'
  AND (
    json_type(data, '$.cost') IN ('integer', 'real')
    OR json_type(data, '$.tokens.total') IN ('integer', 'real')
  )
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn aggregates_today_and_window() {
        let now = Utc.with_ymd_and_hms(2026, 7, 23, 12, 0, 0).unwrap();
        let day_ms = start_of_utc_day(now).timestamp_millis();
        let rows = vec![
            CostRow {
                created_ms: day_ms + 60_000,
                cost: 1.25,
                tokens: 1000,
                model: Some("model-a".to_string()),
            },
            CostRow {
                created_ms: day_ms - 2 * 24 * 60 * 60 * 1000,
                cost: 2.5,
                tokens: 2000,
                model: Some("model-b".to_string()),
            },
            CostRow {
                created_ms: day_ms - 40 * 24 * 60 * 60 * 1000,
                cost: 9.0,
                tokens: 9000,
                model: None,
            },
        ];
        let snapshot = snapshot_from_rows("opencode", "local", &rows, now);
        assert_eq!(snapshot.session_cost_usd, 1.25);
        assert_eq!(snapshot.session_tokens, 1000);
        assert_eq!(snapshot.last30_days_cost_usd, 3.75);
        assert_eq!(snapshot.last30_days_tokens, 3000);
        assert_eq!(snapshot.daily.len(), 2);
        assert_eq!(snapshot.daily[1].model_breakdowns[0].model_name, "model-a");
        assert_eq!(snapshot.source, "local");
    }

    #[test]
    fn cursor_event_prefers_charged_cents() {
        let event = CursorUsageEvent {
            timestamp: Some(serde_json::json!("1782498274196")),
            charged_cents: Some(10.8),
            usage_based_costs: Some("-".to_string()),
            token_usage: Some(CursorTokenUsage {
                input_tokens: Some(100),
                output_tokens: Some(50),
                cache_read_tokens: Some(999),
                total_cents: Some(21.6),
            }),
            model: Some("gpt-5".to_string()),
            model_name: None,
            underlying_model: None,
        };
        let row = cost_row_from_cursor_event(&event).expect("row");
        assert_eq!(row.tokens, 150);
        assert!((row.cost - 0.108).abs() < 1e-9);
        assert_eq!(row.created_ms, 1782498274196);
        assert_eq!(row.model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn cursor_event_falls_back_to_dollar_string() {
        let event = CursorUsageEvent {
            timestamp: Some(serde_json::json!(1_700_000_000_000_i64)),
            charged_cents: None,
            usage_based_costs: Some("$1.25".to_string()),
            token_usage: Some(CursorTokenUsage {
                input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_tokens: None,
                total_cents: None,
            }),
            model: None,
            model_name: None,
            underlying_model: None,
        };
        let row = cost_row_from_cursor_event(&event).expect("row");
        assert!((row.cost - 1.25).abs() < 1e-9);
        assert_eq!(row.tokens, 15);
    }

    #[test]
    fn grok_turn_completed_prefers_cost_usd_ticks() {
        // 447_724_000 ticks == $0.0447724 at 1e10 ticks/$ (observed for grok-4.5-build).
        let value = serde_json::json!({
            "timestamp": 1784139060,
            "method": "_x.ai/session/update",
            "params": {
                "update": {
                    "sessionUpdate": "turn_completed",
                    "usage": {
                        "inputTokens": 62353,
                        "outputTokens": 858,
                        "totalTokens": 63211,
                        "cachedReadTokens": 50048,
                        "reasoningTokens": 375,
                        "costUsdTicks": 447_724_000_i64,
                        "modelUsage": {
                            "grok-4.5-build": {
                                "inputTokens": 62353,
                                "outputTokens": 858,
                                "totalTokens": 63211,
                                "cachedReadTokens": 50048,
                                "costUsdTicks": 447_724_000_i64
                            }
                        }
                    }
                }
            }
        });
        let rows = cost_rows_from_grok_update(&value);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.tokens, 63211);
        assert!((row.cost - 0.044772).abs() < 1e-6);
        assert_eq!(row.created_ms, 1784139060 * 1000);
        assert_eq!(row.model.as_deref(), Some("grok-4.5-build"));
    }

    #[test]
    fn grok_turn_completed_estimates_zero_ticks_from_model_rates() {
        // SuperGrok free tier often reports costUsdTicks=0; price via model rates.
        let value = serde_json::json!({
            "timestamp": 1784139060,
            "params": {
                "update": {
                    "sessionUpdate": "turn_completed",
                    "usage": {
                        "inputTokens": 10_000,
                        "outputTokens": 1_000,
                        "totalTokens": 11_000,
                        "cachedReadTokens": 4_000,
                        "costUsdTicks": 0,
                        "modelUsage": {
                            "grok-4.5": {
                                "inputTokens": 10_000,
                                "outputTokens": 1_000,
                                "totalTokens": 11_000,
                                "cachedReadTokens": 4_000,
                                "costUsdTicks": 0
                            }
                        }
                    }
                }
            }
        });
        let rows = cost_rows_from_grok_update(&value);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        // uncached 6000 * $2/M + cached 4000 * $0.30/M + output 1000 * $6/M
        // = 0.012 + 0.0012 + 0.006 = 0.0192
        assert_eq!(row.tokens, 11_000);
        assert!((row.cost - 0.0192).abs() < 1e-9);
        assert_eq!(row.model.as_deref(), Some("grok-4.5"));
    }

    #[test]
    fn grok_fast_model_uses_volume_rates() {
        let cost = estimate_grok_model_cost(
            "grok-composer-2.5-fast",
            &serde_json::json!({
                "inputTokens": 1_000_000,
                "outputTokens": 1_000_000,
                "cachedReadTokens": 0
            }),
        );
        // $0.20 + $0.50 = $0.70 per 1M in + 1M out
        assert!((cost - 0.70).abs() < 1e-9);
    }

    #[test]
    fn grok_updates_file_scans_turn_completed() {
        let dir = std::env::temp_dir().join(format!(
            "codexbar-grok-cost-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("updates.jsonl");
        let mut file = fs::File::create(&path).expect("create");
        writeln!(
            file,
            r#"{{"timestamp":1784139060,"params":{{"update":{{"sessionUpdate":"turn_completed","usage":{{"totalTokens":42,"costUsdTicks":100000000}}}}}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"timestamp":1784139061,"params":{{"update":{{"sessionUpdate":"agent_message_chunk"}}}}}}"#
        )
        .unwrap();
        let rows = read_grok_updates_file(&path).expect("read");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tokens, 42);
        assert!((rows[0].cost - 0.01).abs() < 1e-9);
    }

    #[test]
    fn normalize_epoch_seconds_and_ms() {
        assert_eq!(normalize_epoch_ms(1_700_000_000.0), Some(1_700_000_000_000));
        assert_eq!(
            normalize_epoch_ms(1_700_000_000_000.0),
            Some(1_700_000_000_000)
        );
    }
}
