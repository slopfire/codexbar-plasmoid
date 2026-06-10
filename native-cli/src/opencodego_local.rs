use crate::output::{clamp_percent, rate_window};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

const FIVE_HOURS_MS: i64 = 5 * 60 * 60 * 1000;
const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const LIMITS: (f64, f64, f64) = (12.0, 30.0, 60.0);

#[derive(Clone)]
struct UsageRow {
    created_ms: i64,
    cost: f64,
}

pub struct LocalUsageSnapshot {
    pub primary: crate::output::RateWindow,
    pub secondary: crate::output::RateWindow,
    pub tertiary: crate::output::RateWindow,
    pub updated_at: DateTime<Utc>,
}

pub fn local_paths(home: &Path) -> (PathBuf, PathBuf) {
    let root = home.join(".local/share/opencode");
    (
        root.join("auth.json"),
        root.join("opencode.db"),
    )
}

pub fn can_read_local_usage(home: &Path) -> bool {
    let (_, database_path) = local_paths(home);
    database_path.exists()
}

pub fn fetch_local_usage(home: &Path) -> Result<LocalUsageSnapshot> {
    let (auth_path, database_path) = local_paths(home);
    let has_auth = has_auth_key(&auth_path);
    if !database_path.exists() {
        if has_auth {
            return Err(anyhow!(
                "OpenCode Go database not found at ~/.local/share/opencode/opencode.db"
            ));
        }
        return Err(anyhow!(
            "OpenCode Go not detected. Log in with OpenCode Go or use it locally first."
        ));
    }

    let rows = read_rows(&database_path)?;
    if !has_auth && rows.is_empty() {
        return Err(anyhow!(
            "OpenCode Go not detected. Log in with OpenCode Go or use it locally first."
        ));
    }
    if rows.is_empty() {
        return Err(anyhow!(
            "OpenCode Go local usage history is unavailable: no local usage rows"
        ));
    }
    Ok(snapshot_from_rows(&rows, Utc::now()))
}

fn has_auth_key(auth_path: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(auth_path) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    json.pointer("/opencode-go/key")
        .and_then(|value| value.as_str())
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn read_rows(database_path: &Path) -> Result<Vec<UsageRow>> {
    let conn = Connection::open_with_flags(database_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_part = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'part' LIMIT 1",
            [],
            |row| row.get::<_, i32>(0),
        )
        .unwrap_or(0)
        == 1;
    let sql = if has_part {
        MESSAGE_AND_PART_USAGE_SQL
    } else {
        MESSAGE_USAGE_SQL
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageRow {
            created_ms: row.get(0)?,
            cost: row.get(1)?,
        })
    })?;
    Ok(rows
        .flatten()
        .filter(|row| row.created_ms > 0 && row.cost.is_finite() && row.cost >= 0.0)
        .collect())
}

fn snapshot_from_rows(rows: &[UsageRow], now: DateTime<Utc>) -> LocalUsageSnapshot {
    let now_ms = now.timestamp_millis();
    let session_start = now_ms - FIVE_HOURS_MS;
    let week_start = start_of_utc_week(now).timestamp_millis();
    let week_end = week_start + WEEK_MS;
    let earliest_ms = rows.iter().map(|row| row.created_ms).min().unwrap_or(now_ms);
    let month_bounds = month_bounds_for(now, earliest_ms);

    let session_cost = sum_rows(rows, session_start, now_ms);
    let weekly_cost = sum_rows(rows, week_start, week_end);
    let monthly_cost = sum_rows(rows, month_bounds.0, month_bounds.1);

    LocalUsageSnapshot {
        primary: rate_window(
            percent(session_cost, LIMITS.0),
            Some(5 * 60),
            Some(Utc.timestamp_millis_opt(rolling_reset(rows, now_ms)).unwrap()),
        ),
        secondary: rate_window(
            percent(weekly_cost, LIMITS.1),
            Some(7 * 24 * 60),
            Some(Utc.timestamp_millis_opt(week_end).unwrap()),
        ),
        tertiary: rate_window(
            percent(monthly_cost, LIMITS.2),
            Some(30 * 24 * 60),
            Some(Utc.timestamp_millis_opt(month_bounds.1).unwrap()),
        ),
        updated_at: now,
    }
}

fn sum_rows(rows: &[UsageRow], start_ms: i64, end_ms: i64) -> f64 {
    rows.iter()
        .filter(|row| row.created_ms >= start_ms && row.created_ms < end_ms)
        .map(|row| row.cost)
        .sum()
}

fn percent(used: f64, limit: f64) -> f64 {
    if !used.is_finite() || limit <= 0.0 {
        return 0.0;
    }
    let value = clamp_percent((used / limit) * 100.0);
    (value * 10.0).round() / 10.0
}

fn rolling_reset(rows: &[UsageRow], now_ms: i64) -> i64 {
    let session_start = now_ms - FIVE_HOURS_MS;
    let oldest = rows
        .iter()
        .filter(|row| row.created_ms >= session_start && row.created_ms < now_ms)
        .map(|row| row.created_ms)
        .min()
        .unwrap_or(now_ms);
    oldest + FIVE_HOURS_MS
}

fn start_of_utc_week(now: DateTime<Utc>) -> DateTime<Utc> {
    let weekday = now.weekday().num_days_from_monday();
    let start_day = now.date_naive() - chrono::Days::new(weekday as u64);
    Utc.from_utc_datetime(&start_day.and_hms_opt(0, 0, 0).unwrap())
}

fn month_bounds_for(now: DateTime<Utc>, anchor_ms: i64) -> (i64, i64) {
    if anchor_ms <= 0 {
        let start = Utc
            .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .unwrap();
        let end = if now.month() == 12 {
            Utc.with_ymd_and_hms(now.year() + 1, 1, 1, 0, 0, 0).unwrap()
        } else {
            Utc.with_ymd_and_hms(now.year(), now.month() + 1, 1, 0, 0, 0)
                .unwrap()
        };
        return (start.timestamp_millis(), end.timestamp_millis());
    }

    let anchor = Utc.timestamp_millis_opt(anchor_ms).unwrap();
    let mut start = anchored_month(now.year(), now.month(), &anchor);
    if start > now {
        let (year, month) = if now.month() == 1 {
            (now.year() - 1, 12)
        } else {
            (now.year(), now.month() - 1)
        };
        start = anchored_month(year, month, &anchor);
    }
    let (next_year, next_month) = if now.month() == 12 {
        (now.year() + 1, 1)
    } else {
        (now.year(), now.month() + 1)
    };
    let end = anchored_month(next_year, next_month, &anchor);
    (start.timestamp_millis(), end.timestamp_millis())
}

fn anchored_month(year: i32, month: u32, anchor: &DateTime<Utc>) -> DateTime<Utc> {
    let days_in_month = days_in_month(year, month);
    let day = anchor.day().min(days_in_month);
    Utc.with_ymd_and_hms(year, month, day, anchor.hour(), anchor.minute(), anchor.second())
        .single()
        .unwrap_or_else(|| Utc.with_ymd_and_hms(year, month, days_in_month, 0, 0, 0).unwrap())
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).unwrap()
    } else {
        Utc.with_ymd_and_hms(year, month + 1, 1, 0, 0, 0).unwrap()
    };
    let current = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).unwrap();
    (next - current).num_days() as u32
}

const MESSAGE_USAGE_SQL: &str = r#"
SELECT
  CAST(COALESCE(json_extract(data, '$.time.created'), time_created) AS INTEGER) AS createdMs,
  CAST(json_extract(data, '$.cost') AS REAL) AS cost
FROM message
WHERE json_valid(data)
  AND json_extract(data, '$.providerID') = 'opencode-go'
  AND json_extract(data, '$.role') = 'assistant'
  AND json_type(data, '$.cost') IN ('integer', 'real')
"#;

const MESSAGE_AND_PART_USAGE_SQL: &str = r#"
WITH message_costs AS (
  SELECT
    id AS messageID,
    CAST(COALESCE(json_extract(data, '$.time.created'), time_created) AS INTEGER) AS createdMs,
    CAST(json_extract(data, '$.cost') AS REAL) AS cost
  FROM message
  WHERE json_valid(data)
    AND json_extract(data, '$.providerID') = 'opencode-go'
    AND json_extract(data, '$.role') = 'assistant'
    AND json_type(data, '$.cost') IN ('integer', 'real')
)
SELECT createdMs, cost
FROM message_costs
UNION ALL
SELECT
  CAST(COALESCE(json_extract(p.data, '$.time.created'), p.time_created, m.time_created) AS INTEGER) AS createdMs,
  CAST(json_extract(p.data, '$.cost') AS REAL) AS cost
FROM part p
JOIN message m ON m.id = p.message_id
WHERE json_valid(p.data)
  AND json_valid(m.data)
  AND json_extract(p.data, '$.type') = 'step-finish'
  AND json_type(p.data, '$.cost') IN ('integer', 'real')
  AND json_extract(m.data, '$.providerID') = 'opencode-go'
  AND json_extract(m.data, '$.role') = 'assistant'
  AND NOT EXISTS (
    SELECT 1
    FROM message_costs
    WHERE message_costs.messageID = p.message_id
  )
"#;
