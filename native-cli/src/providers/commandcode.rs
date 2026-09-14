//! Command Code (commandcode.ai) usage.
//!
//! The `cmd` CLI keeps a bearer API key in `~/.commandcode/auth.json` (written by
//! `cmd login`). The same key authenticates the public alpha API, which reports
//! the rolling credit windows shown in https://commandcode.ai/usage:
//!
//! - `/alpha/billing/credits` — 5-hour + weekly windows and credit balances
//! - `/alpha/billing/subscriptions` — plan tier
//! - `/alpha/whoami` — account identity
//!
//! Token spend history is not served by the API; it comes from the local session
//! transcripts instead (see `local_cost::fetch_costs`).

use crate::http::HttpClient;
use crate::output::{
    clamp_percent, CreditsSnapshot, ProviderIdentitySnapshot, ProviderPayload, RateWindow,
    UsageSnapshot,
};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_API_BASE: &str = "https://api.commandcode.ai";
const SITE_URL: &str = "https://commandcode.ai/usage";

/// Plan tiers and their monthly credit allotment, mirroring the mapping the
/// `cmd` CLI uses for its own usage screen. Unknown tiers report no plan label
/// rather than inventing one.
const PLAN_TIERS: &[(&str, &str, f64)] = &[
    ("individual-go", "Go", 10.0),
    ("individual-goat", "GOAT", 70.0),
    ("individual-pro", "Pro", 30.0),
    ("individual-pro-v1", "Pro", 80.0),
    ("individual-provider", "Provider", 15.0),
    ("individual-max", "Max", 150.0),
    ("individual-ultra", "Ultra", 300.0),
    ("teams-pro", "Teams Pro", 40.0),
];

pub fn fetch(http: &HttpClient) -> ProviderPayload {
    match fetch_inner(http) {
        Ok(payload) => payload,
        Err(error) => ProviderPayload::error("commandcode", error.to_string()),
    }
}

fn fetch_inner(http: &HttpClient) -> Result<ProviderPayload> {
    let api_key = resolve_api_key().ok_or_else(|| {
        anyhow!(
            "No Command Code session found. Run `cmd login`, set COMMANDCODE_API_KEY, \
             or point COMMANDCODE_HOME at the CLI data directory."
        )
    })?;
    let base = api_base();
    let headers = auth_headers(&api_key)?;

    let credits: CreditsResponse = serde_json::from_value(http.fetch_json_value(
        &format!("{base}/alpha/billing/credits"),
        &headers,
    )?)
    .context("parse Command Code credits")?;

    // Plan and identity are decorative: a missing or unreadable response must
    // not take down the limit windows.
    let whoami = http
        .fetch_json_value(&format!("{base}/alpha/whoami"), &headers)
        .ok()
        .and_then(|value| serde_json::from_value::<WhoamiResponse>(value).ok());
    let subscription = http
        .fetch_json_value(&format!("{base}/alpha/billing/subscriptions"), &headers)
        .ok()
        .and_then(|value| serde_json::from_value::<SubscriptionsResponse>(value).ok());

    let plan = subscription.as_ref().and_then(active_plan);
    let account_email = whoami
        .as_ref()
        .and_then(|response| response.user.as_ref())
        .and_then(|user| user.email.clone())
        .map(|email| email.trim().to_string())
        .filter(|email| !email.is_empty());

    let limits = credits.window_limits.as_ref();
    let usage = UsageSnapshot {
        primary: window_rate(
            limits.and_then(|limits| limits.five_hour.as_ref()),
            5 * 60,
        ),
        secondary: window_rate(limits.and_then(|limits| limits.weekly.as_ref()), 7 * 24 * 60),
        tertiary: None,
        usage_rows: None,
        provider_cost: None,
        cursor_requests: None,
        updated_at: Utc::now(),
        identity: Some(ProviderIdentitySnapshot {
            account_email: account_email.clone(),
            account_organization: None,
            login_method: plan.as_ref().and_then(|plan| plan.label.clone()),
        }),
    };

    let mut payload = ProviderPayload::ok(
        "commandcode",
        usage,
        account_email,
        credits_snapshot(&credits, plan.as_ref()),
        None,
    );
    payload.site_url = Some(SITE_URL.to_string());
    Ok(payload)
}

fn auth_headers(api_key: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|error| anyhow!("invalid Command Code API key: {error}"))?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    Ok(headers)
}

fn api_base() -> String {
    std::env::var("COMMANDCODE_API_BASE")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
}

/// Prefer the documented `auth.json` written by `cmd login`. `COMMANDCODE_HOME`
/// and `COMMANDCODE_AUTH_FILE` allow pointing at a non-default install.
fn resolve_api_key() -> Option<String> {
    if let Some(key) = non_empty_env("COMMANDCODE_API_KEY") {
        return Some(key);
    }
    let path = std::env::var("COMMANDCODE_AUTH_FILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| commandcode_home().join("auth.json"));
    let raw = std::fs::read_to_string(path).ok()?;
    let auth: AuthFile = serde_json::from_str(&raw).ok()?;
    auth.api_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn commandcode_home() -> PathBuf {
    non_empty_env("COMMANDCODE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".commandcode")
        })
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// One credit window. `used`/`cap` are credit amounts, not percentages.
fn window_rate(window: Option<&CreditWindow>, window_minutes: i64) -> Option<RateWindow> {
    let window = window?;
    let cap = window.cap.filter(|cap| cap.is_finite() && *cap > 0.0)?;
    let used = window.used.unwrap_or(0.0).max(0.0);
    let used_percent = if window.exceeded == Some(true) {
        100.0
    } else {
        clamp_percent(used / cap * 100.0)
    };
    Some(RateWindow {
        used_percent,
        remaining_percent: (100.0 - used_percent).max(0.0),
        window_minutes: Some(window_minutes),
        resets_at: window.reset_at.and_then(epoch_ms_to_datetime),
        reset_description: None,
    })
}

/// API reset stamps are epoch milliseconds (seconds are tolerated too).
fn epoch_ms_to_datetime(value: f64) -> Option<DateTime<Utc>> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let millis = if value > 10_000_000_000.0 {
        value
    } else {
        value * 1000.0
    };
    Utc.timestamp_millis_opt(millis.round() as i64).single()
}

/// `monthlyCredits` is the *remaining* monthly allowance; purchased and free
/// credits roll over on top of it.
fn credits_snapshot(credits: &CreditsResponse, plan: Option<&PlanInfo>) -> Option<CreditsSnapshot> {
    let balance = credits.credits.as_ref()?;
    let purchased = balance.purchased_credits.unwrap_or(0.0).max(0.0);
    let free = balance.free_credits.unwrap_or(0.0).max(0.0);
    let monthly = balance.monthly_credits.map(|value| value.max(0.0));
    if monthly.is_none() && purchased <= 0.0 && free <= 0.0 {
        return None;
    }
    let remaining = monthly.unwrap_or(0.0) + purchased + free;
    // Only a known plan tier gives a total to measure `used` against; without
    // one the balance is still reported, just without a limit.
    let (used, limit) = match (plan.and_then(|plan| plan.monthly_credits), monthly) {
        (Some(allotment), Some(monthly)) => (
            (allotment - monthly).max(0.0),
            Some(allotment + purchased + free),
        ),
        _ => (0.0, None),
    };
    Some(CreditsSnapshot {
        remaining: Some(remaining),
        used,
        limit,
        updated_at: Utc::now(),
    })
}

/// Only an active subscription names a plan; cancelled/expired tiers keep the
/// account on whatever credits remain and must not show a paid tier.
fn active_plan(subscriptions: &SubscriptionsResponse) -> Option<PlanInfo> {
    let data = subscriptions.data.as_ref()?;
    let status = data.status.as_deref()?.trim().to_ascii_lowercase();
    if status != "active" && status != "trialing" {
        return None;
    }
    let plan_id = data.plan_id.as_deref()?.trim();
    if plan_id.is_empty() {
        return None;
    }
    let tier = PLAN_TIERS
        .iter()
        .find(|(id, _, _)| *id == plan_id)
        .map(|(_, label, credits)| PlanInfo {
            label: Some((*label).to_string()),
            monthly_credits: Some(*credits),
        });
    Some(tier.unwrap_or(PlanInfo {
        label: None,
        monthly_credits: None,
    }))
}

struct PlanInfo {
    label: Option<String>,
    monthly_credits: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthFile {
    api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreditsResponse {
    credits: Option<CreditBalance>,
    #[serde(rename = "windowLimits")]
    window_limits: Option<WindowLimits>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditBalance {
    monthly_credits: Option<f64>,
    purchased_credits: Option<f64>,
    free_credits: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct WindowLimits {
    #[serde(rename = "fiveHour")]
    five_hour: Option<CreditWindow>,
    weekly: Option<CreditWindow>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditWindow {
    used: Option<f64>,
    cap: Option<f64>,
    exceeded: Option<bool>,
    reset_at: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct WhoamiResponse {
    user: Option<WhoamiUser>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WhoamiUser {
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionsResponse {
    data: Option<Subscription>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Subscription {
    status: Option<String>,
    plan_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credits_from(raw: &str) -> CreditsResponse {
        serde_json::from_str(raw).expect("credits JSON should deserialize")
    }

    #[test]
    fn parses_live_credit_windows() {
        // Shape captured from GET /alpha/billing/credits.
        let credits = credits_from(
            r#"{
              "credits": {
                "belowThreshold": false,
                "creditThreshold": 0,
                "monthlyCredits": 69.8345429039,
                "purchasedCredits": 0,
                "freeCredits": 0
              },
              "windowLimits": {
                "limited": true,
                "exceeded": null,
                "fiveHour": { "used": 0.1654570961, "cap": 14, "exceeded": false, "resetAt": 1789384949480 },
                "weekly": { "used": 0.1654570961, "cap": 35, "exceeded": false, "resetAt": 1789971749480 }
              }
            }"#,
        );

        let limits = credits.window_limits.as_ref().expect("window limits");
        let primary = window_rate(limits.five_hour.as_ref(), 300).expect("5-hour window");
        let secondary = window_rate(limits.weekly.as_ref(), 10080).expect("weekly window");

        assert!((primary.used_percent - 1.181836).abs() < 1e-6);
        assert!((primary.remaining_percent - 98.818164).abs() < 1e-6);
        assert_eq!(primary.window_minutes, Some(300));
        assert_eq!(
            primary.resets_at.map(|value| value.timestamp_millis()),
            Some(1789384949480)
        );
        assert!((secondary.used_percent - 0.472734).abs() < 1e-6);
        assert_eq!(secondary.window_minutes, Some(10080));
    }

    #[test]
    fn exceeded_window_pins_percent_at_one_hundred() {
        let credits = credits_from(
            r#"{"windowLimits":{"fiveHour":{"used":20,"cap":14,"exceeded":true,"resetAt":0}}}"#,
        );
        let window = window_rate(credits.window_limits.unwrap().five_hour.as_ref(), 300);
        let window = window.expect("window");
        assert_eq!(window.used_percent, 100.0);
        assert_eq!(window.remaining_percent, 0.0);
        assert!(window.resets_at.is_none());
    }

    #[test]
    fn missing_or_zero_cap_yields_no_window() {
        assert!(window_rate(None, 300).is_none());
        let credits = credits_from(r#"{"windowLimits":{"weekly":{"used":1,"cap":0}}}"#);
        assert!(window_rate(credits.window_limits.unwrap().weekly.as_ref(), 10080).is_none());
    }

    #[test]
    fn credits_report_remaining_plus_rolling_balances() {
        let credits = credits_from(
            r#"{"credits":{"monthlyCredits":69.8345429039,"purchasedCredits":5,"freeCredits":1.5},
                "windowLimits":{"limited":true}}"#,
        );
        let plan = active_plan(&subscriptions(
            r#"{"success":true,"data":{"status":"active","planId":"individual-goat"}}"#,
        ))
        .expect("plan");

        let snapshot = credits_snapshot(&credits, Some(&plan)).expect("credits");
        assert_eq!(snapshot.remaining, Some(76.3345429039));
        assert_eq!(snapshot.limit, Some(76.5));
        // Consumed monthly allowance: allotment (70) − remaining monthly credits.
        assert!((snapshot.used - 0.1654570961).abs() < 1e-9);
        assert!(
            (snapshot.remaining.unwrap() + snapshot.used - snapshot.limit.unwrap()).abs() < 1e-9,
            "remaining + used must equal the reported limit"
        );
    }

    #[test]
    fn unknown_plan_tier_reports_balance_without_a_limit() {
        let credits = credits_from(r#"{"credits":{"monthlyCredits":12.5}}"#);
        let plan = active_plan(&subscriptions(
            r#"{"data":{"status":"active","planId":"individual-mystery"}}"#,
        ))
        .expect("plan");
        assert!(plan.label.is_none());
        assert!(plan.monthly_credits.is_none());

        let snapshot = credits_snapshot(&credits, Some(&plan)).expect("credits");
        assert_eq!(snapshot.remaining, Some(12.5));
        assert_eq!(snapshot.limit, None);
        assert_eq!(snapshot.used, 0.0);
    }

    #[test]
    fn inactive_subscription_does_not_report_a_plan() {
        assert!(active_plan(&subscriptions(
            r#"{"data":{"status":"canceled","planId":"individual-goat"}}"#
        ))
        .is_none());
        assert!(active_plan(&subscriptions(
            r#"{"data":{"status":"active","planId":""}}"#
        ))
        .is_none());
        // No subscription payload at all (free account).
        assert!(active_plan(&subscriptions(r#"{"success":true,"data":null}"#)).is_none());
    }

    #[test]
    fn active_plan_labels_match_the_cli_tiers() {
        for (plan_id, label) in [
            ("individual-go", "Go"),
            ("individual-goat", "GOAT"),
            ("individual-pro", "Pro"),
            ("individual-pro-v1", "Pro"),
            ("individual-provider", "Provider"),
            ("individual-max", "Max"),
            ("individual-ultra", "Ultra"),
            ("teams-pro", "Teams Pro"),
        ] {
            let plan = active_plan(&subscriptions(&format!(
                r#"{{"data":{{"status":"active","planId":"{plan_id}"}}}}"#
            )))
            .expect("plan");
            assert_eq!(plan.label.as_deref(), Some(label), "plan {plan_id}");
        }
    }

    #[test]
    fn known_tier_reports_its_monthly_allotment() {
        let plan = active_plan(&subscriptions(
            r#"{"data":{"status":"active","planId":"individual-goat"}}"#,
        ))
        .expect("plan");
        assert_eq!(plan.monthly_credits, Some(70.0));
    }

    #[test]
    fn epoch_milliseconds_convert_to_utc_datetimes() {
        let reset = epoch_ms_to_datetime(1789384949480.0).expect("reset");
        assert_eq!(reset.timestamp_millis(), 1789384949480);
        // Seconds-since-epoch inputs are still accepted.
        assert_eq!(
            epoch_ms_to_datetime(1_789_384_949.0).map(|value| value.timestamp()),
            Some(1789384949)
        );
        assert!(epoch_ms_to_datetime(0.0).is_none());
        assert!(epoch_ms_to_datetime(f64::NAN).is_none());
    }

    #[test]
    fn auth_file_reads_the_camel_case_key() {
        let auth: AuthFile = serde_json::from_str(
            r#"{"apiKey":"user_abc","userId":"1da35edb","userName":"DillerOFire"}"#,
        )
        .expect("auth file");
        assert_eq!(auth.api_key.as_deref(), Some("user_abc"));
    }

    fn subscriptions(raw: &str) -> SubscriptionsResponse {
        serde_json::from_str(raw).expect("subscriptions JSON should deserialize")
    }
}
