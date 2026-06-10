use crate::http::HttpClient;
use crate::output::{clamp_percent, rate_window};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, ORIGIN, REFERER};
use serde_json::Value;
use std::time::Duration;
use uuid::Uuid;

const SERVER_URL: &str = "https://opencode.ai/_server";
const WORKSPACES_SERVER_ID: &str = "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
const SUBSCRIPTION_SERVER_ID: &str = "7abeebee372f304e050aaaf92be863f4a86490e382f8c79db68fd94040d691b4";

pub struct OpenCodeUsage {
    pub primary: crate::output::RateWindow,
    pub secondary: crate::output::RateWindow,
    pub tertiary: Option<crate::output::RateWindow>,
    pub updated_at: DateTime<Utc>,
}

pub fn fetch_workspace_id(http: &HttpClient, cookie_header: &str) -> Result<String> {
    let mut text = fetch_server(
        http,
        WORKSPACES_SERVER_ID,
        None,
        "GET",
        "https://opencode.ai/",
        cookie_header,
    )?;
    let mut ids = parse_workspace_ids(&text);
    if ids.is_empty() {
        text = fetch_server(
            http,
            WORKSPACES_SERVER_ID,
            Some("[]"),
            "POST",
            "https://opencode.ai/",
            cookie_header,
        )?;
        ids = parse_workspace_ids(&text);
    }
    ids.into_iter()
        .next()
        .ok_or_else(|| anyhow!("Missing OpenCode workspace id."))
}

pub fn fetch_subscription(
    http: &HttpClient,
    workspace_id: &str,
    cookie_header: &str,
) -> Result<String> {
    let referer = format!("https://opencode.ai/workspace/{workspace_id}/billing");
    let mut text = fetch_server(
        http,
        SUBSCRIPTION_SERVER_ID,
        Some(&serde_json::to_string(&[workspace_id])?),
        "GET",
        &referer,
        cookie_header,
    )?;
    if looks_signed_out(&text) || is_explicit_null(&text) || !has_usage(&text) {
        text = fetch_server(
            http,
            SUBSCRIPTION_SERVER_ID,
            Some(&serde_json::to_string(&[workspace_id])?),
            "POST",
            &referer,
            cookie_header,
        )?;
    }
    if looks_signed_out(&text) {
        return Err(anyhow!("OpenCode session cookie is invalid or expired."));
    }
    if !has_usage(&text) {
        return Err(anyhow!(
            "No subscription usage data was returned for workspace {workspace_id}."
        ));
    }
    Ok(text)
}

pub fn fetch_usage_page(
    http: &HttpClient,
    workspace_id: &str,
    cookie_header: &str,
) -> Result<String> {
    let url = format!("https://opencode.ai/workspace/{workspace_id}/go");
    let headers = crate::http::html_headers(cookie_header)?;
    let text = http.fetch_text(&url, &headers)?;
    if looks_signed_out(&text) || !has_usage(&text) {
        return Err(anyhow!("Missing OpenCode Go usage fields."));
    }
    Ok(text)
}

pub fn parse_usage(text: &str, include_monthly: bool) -> Result<OpenCodeUsage> {
    if let Some(snapshot) = parse_usage_object(&serde_json::from_str(text).ok(), include_monthly) {
        return Ok(snapshot);
    }

    let rolling_percent = extract_number(r"rollingUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)", text);
    let rolling_reset = extract_int(r"rollingUsage[^}]*?resetInSec\s*:\s*([0-9]+)", text);
    let weekly_percent = extract_number(r"weeklyUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)", text);
    let weekly_reset = extract_int(r"weeklyUsage[^}]*?resetInSec\s*:\s*([0-9]+)", text);
    let monthly_percent = if include_monthly {
        extract_number(r"monthlyUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)", text)
    } else {
        None
    };
    let monthly_reset = if include_monthly {
        extract_int(r"monthlyUsage[^}]*?resetInSec\s*:\s*([0-9]+)", text)
    } else {
        None
    };

    let (rolling_percent, rolling_reset, weekly_percent, weekly_reset) = match (
        rolling_percent,
        rolling_reset,
        weekly_percent,
        weekly_reset,
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => return Err(anyhow!("Missing usage fields.")),
    };

    Ok(build_snapshot(
        rolling_percent,
        weekly_percent,
        rolling_reset,
        weekly_reset,
        match (monthly_percent, monthly_reset) {
            (Some(percent), reset) => Some((percent, reset.unwrap_or(0))),
            _ => None,
        },
    ))
}

fn fetch_server(
    http: &HttpClient,
    server_id: &str,
    args: Option<&str>,
    method: &str,
    referer: &str,
    cookie_header: &str,
) -> Result<String> {
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(cookie_header)?);
    headers.insert("X-Server-Id", HeaderValue::from_str(server_id)?);
    headers.insert(
        "X-Server-Instance",
        HeaderValue::from_str(&format!("server-fn:{}", Uuid::new_v4()))?,
    );
    headers.insert(ORIGIN, HeaderValue::from_static("https://opencode.ai"));
    headers.insert(REFERER, HeaderValue::from_str(referer)?);
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("text/javascript, application/json;q=0.9, */*;q=0.8"),
    );

    if method.eq_ignore_ascii_case("GET") {
        let mut url = url::Url::parse(SERVER_URL)?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("id", server_id);
            if let Some(args) = args.filter(|value| *value != "[]") {
                query.append_pair("args", args);
            }
        }
        http.fetch_text(url.as_str(), &headers)
    } else {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        http.post_text(SERVER_URL, &headers, args.unwrap_or("[]").as_bytes())
    }
}

fn build_snapshot(
    rolling_percent: f64,
    weekly_percent: f64,
    rolling_reset: i64,
    weekly_reset: i64,
    monthly: Option<(f64, i64)>,
) -> OpenCodeUsage {
    let now = Utc::now();
    OpenCodeUsage {
        primary: open_code_window(rolling_percent, 5 * 60, now, rolling_reset),
        secondary: open_code_window(weekly_percent, 7 * 24 * 60, now, weekly_reset),
        tertiary: monthly.map(|(percent, reset)| open_code_window(percent, 30 * 24 * 60, now, reset)),
        updated_at: now,
    }
}

fn open_code_window(
    used_percent: f64,
    window_minutes: i64,
    now: DateTime<Utc>,
    reset_in_sec: i64,
) -> crate::output::RateWindow {
    let resets_at = now + Duration::from_secs(reset_in_sec.max(0) as u64);
    rate_window(
        clamp_percent(used_percent),
        Some(window_minutes),
        Some(resets_at),
    )
}

fn parse_usage_object(value: &Option<Value>, include_monthly: bool) -> Option<OpenCodeUsage> {
    let value = value.as_ref()?;
    for candidate in candidate_objects(value) {
        let rolling = first_object(&candidate, &["rollingUsage", "rolling", "rolling_usage"]);
        let weekly = first_object(&candidate, &["weeklyUsage", "weekly", "weekly_usage"]);
        let monthly = if include_monthly {
            first_object(&candidate, &["monthlyUsage", "monthly", "monthly_usage"])
        } else {
            None
        };
        if let (Some(rolling), Some(weekly)) = (rolling, weekly) {
            let rolling_window = parse_window(&rolling, Utc::now())?;
            let weekly_window = parse_window(&weekly, Utc::now())?;
            let monthly_window = monthly.and_then(|value| parse_window(&value, Utc::now()));
            return Some(build_snapshot(
                rolling_window.0,
                weekly_window.0,
                rolling_window.1,
                weekly_window.1,
                monthly_window.map(|(percent, reset)| (percent, reset)),
            ));
        }
    }
    None
}

fn candidate_objects(value: &Value) -> Vec<Value> {
    let mut out = vec![value.clone()];
    for key in ["data", "result", "usage", "billing", "payload"] {
        if let Some(nested) = value.get(key) {
            out.push(nested.clone());
        }
    }
    out
}

fn first_object(value: &Value, keys: &[&str]) -> Option<Value> {
    for key in keys {
        if let Some(object) = value.get(*key) {
            return Some(object.clone());
        }
    }
    None
}

fn parse_window(dict: &Value, now: DateTime<Utc>) -> Option<(f64, i64)> {
    let percent_keys = [
        "usagePercent", "usedPercent", "percentUsed", "percent", "usage",
    ];
    let mut percent = None;
    for key in percent_keys {
        if let Some(value) = dict.get(key).and_then(value_as_f64) {
            percent = Some(value);
            break;
        }
    }
    let mut percent = percent?;
    if percent <= 1.0 && percent >= 0.0 {
        percent *= 100.0;
    }
    let mut reset_in_sec = ["resetInSec", "resetInSeconds", "resetSeconds", "resetSec"]
        .iter()
        .find_map(|key| dict.get(*key).and_then(value_as_i64))
        .unwrap_or(0);
    if reset_in_sec <= 0 {
        for key in ["resetAt", "resetsAt"] {
            if let Some(reset_at) = dict
                .get(key)
                .and_then(|value| value.as_str())
                .and_then(|value| value.parse::<DateTime<Utc>>().ok())
            {
                reset_in_sec = (reset_at - now).num_seconds().max(0);
                break;
            }
        }
    }
    Some((clamp_percent(percent), reset_in_sec))
}

fn parse_workspace_ids(text: &str) -> Vec<String> {
    let regex = Regex::new(r#"id\s*:\s*"(wrk_[^"]+)""#).unwrap();
    let mut ids = Vec::new();
    for cap in regex.captures_iter(text) {
        if let Some(id) = cap.get(1) {
            ids.push(id.as_str().to_string());
        }
    }
    if ids.is_empty() {
        collect_workspace_ids(&serde_json::from_str(text).ok(), &mut ids);
    }
    ids
}

fn collect_workspace_ids(value: &Option<Value>, out: &mut Vec<String>) {
    let Some(value) = value else { return };
    match value {
        Value::String(text) if text.starts_with("wrk_") && !out.contains(text) => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| collect_workspace_ids(&Some(item.clone()), out)),
        Value::Object(map) => map.values().for_each(|item| collect_workspace_ids(&Some(item.clone()), out)),
        _ => {}
    }
}

fn has_usage(text: &str) -> bool {
    parse_usage_object(&serde_json::from_str(text).ok(), true).is_some()
        || Regex::new(r"rollingUsage[^}]*?usagePercent\s*:\s*([0-9]+(?:\.[0-9]+)?)")
            .unwrap()
            .is_match(text)
}

fn looks_signed_out(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "login",
        "sign in",
        "auth/authorize",
        "not associated with an account",
        "actor of type \"public\"",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_explicit_null(text: &str) -> bool {
    text.trim().eq_ignore_ascii_case("null")
}

fn extract_number(pattern: &str, text: &str) -> Option<f64> {
    Regex::new(pattern)
        .ok()?
        .captures(text)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

fn extract_int(pattern: &str, text: &str) -> Option<i64> {
    Regex::new(pattern)
        .ok()?
        .captures(text)?
        .get(1)?
        .as_str()
        .parse()
        .ok()
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
