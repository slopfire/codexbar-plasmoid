//! Token + API cost snapshots for native providers.
//!
//! Sources:
//! - OpenCode / OpenCode Go: SQLite under `~/.local/share/opencode/`
//! - Cursor: dashboard usage events (`get-filtered-usage-events`) via session cookie
//! - Grok: local session `updates.jsonl` turn_completed usage (tokens only; SuperGrok has no $)
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
}

#[derive(Clone)]
struct CostRow {
    created_ms: i64,
    cost: f64,
    tokens: i64,
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
        Ok(CostRow {
            created_ms: row.get(0)?,
            cost: row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
            tokens: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
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
    Some(CostRow {
        created_ms,
        cost,
        tokens,
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

    // SuperGrok is subscription-billed; cost stays 0 and tokens drive the UI.
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
        if let Some(row) = cost_row_from_grok_update(&value) {
            rows.push(row);
        }
    }
    Ok(rows)
}

fn cost_row_from_grok_update(value: &serde_json::Value) -> Option<CostRow> {
    let params = value.get("params")?;
    let update = params.get("update")?;
    if update.get("sessionUpdate").and_then(|v| v.as_str()) != Some("turn_completed") {
        return None;
    }
    let usage = update.get("usage")?;
    let tokens = usage
        .get("totalTokens")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            let input = usage.get("inputTokens").and_then(|v| v.as_i64()).unwrap_or(0);
            let output = usage
                .get("outputTokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let reasoning = usage
                .get("reasoningTokens")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Some(input + output + reasoning)
        })
        .unwrap_or(0);
    if tokens <= 0 {
        return None;
    }
    let created_ms = parse_event_timestamp(value.get("timestamp"))
        .or_else(|| parse_event_timestamp(value.get("ts")))
        .or_else(|| {
            // Nested agent clock when top-level timestamp is missing.
            params
                .pointer("/_meta/agentTimestampMs")
                .and_then(|v| v.as_f64())
                .and_then(normalize_epoch_ms)
        })?;
    Some(CostRow {
        created_ms,
        cost: 0.0,
        tokens,
    })
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
    let mut daily: BTreeMap<String, (f64, i64)> = BTreeMap::new();

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
            let entry = daily.entry(day_key).or_insert((0.0, 0));
            entry.0 += row.cost;
            entry.1 += row.tokens;
        }
    }

    let daily_points: Vec<DailyCostPoint> = daily
        .into_iter()
        .map(|(date, (total_cost, total_tokens))| DailyCostPoint {
            date,
            total_cost: round_money(total_cost),
            total_tokens,
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
  CAST(COALESCE(json_extract(data, '$.tokens.total'), 0) AS INTEGER) AS tokens
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
  CAST(COALESCE(json_extract(data, '$.tokens.total'), 0) AS INTEGER) AS tokens
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
            },
            CostRow {
                created_ms: day_ms - 2 * 24 * 60 * 60 * 1000,
                cost: 2.5,
                tokens: 2000,
            },
            CostRow {
                created_ms: day_ms - 40 * 24 * 60 * 60 * 1000,
                cost: 9.0,
                tokens: 9000,
            },
        ];
        let snapshot = snapshot_from_rows("opencode", "local", &rows, now);
        assert_eq!(snapshot.session_cost_usd, 1.25);
        assert_eq!(snapshot.session_tokens, 1000);
        assert_eq!(snapshot.last30_days_cost_usd, 3.75);
        assert_eq!(snapshot.last30_days_tokens, 3000);
        assert_eq!(snapshot.daily.len(), 2);
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
        };
        let row = cost_row_from_cursor_event(&event).expect("row");
        assert_eq!(row.tokens, 150);
        assert!((row.cost - 0.108).abs() < 1e-9);
        assert_eq!(row.created_ms, 1782498274196);
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
        };
        let row = cost_row_from_cursor_event(&event).expect("row");
        assert!((row.cost - 1.25).abs() < 1e-9);
        assert_eq!(row.tokens, 15);
    }

    #[test]
    fn grok_turn_completed_row() {
        let value = serde_json::json!({
            "timestamp": 1784139060,
            "method": "_x.ai/session/update",
            "params": {
                "update": {
                    "sessionUpdate": "turn_completed",
                    "usage": {
                        "inputTokens": 100,
                        "outputTokens": 20,
                        "totalTokens": 120,
                        "reasoningTokens": 5
                    }
                }
            }
        });
        let row = cost_row_from_grok_update(&value).expect("row");
        assert_eq!(row.tokens, 120);
        assert_eq!(row.cost, 0.0);
        assert_eq!(row.created_ms, 1784139060 * 1000);
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
            r#"{{"timestamp":1784139060,"params":{{"update":{{"sessionUpdate":"turn_completed","usage":{{"totalTokens":42}}}}}}}}"#
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
