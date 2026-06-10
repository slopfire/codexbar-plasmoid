use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPayload {
    pub provider: String,
    pub account: Option<String>,
    pub version: Option<String>,
    pub source: String,
    pub status: Option<ProviderStatusPayload>,
    pub usage: Option<UsageSnapshot>,
    pub credits: Option<CreditsSnapshot>,
    pub error: Option<ProviderErrorPayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatusPayload {
    pub indicator: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<RateWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<RateWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tertiary: Option<RateWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_cost: Option<ProviderCostSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_requests: Option<CursorRequestUsage>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<ProviderIdentitySnapshot>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateWindow {
    pub used_percent: f64,
    pub remaining_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCostSnapshot {
    pub used: f64,
    pub limit: f64,
    pub currency_code: String,
    pub period: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorRequestUsage {
    pub used: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderIdentitySnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    pub account_organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_method: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditsSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f64>,
    pub used: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderErrorPayload {
    pub code: i32,
    pub message: String,
    pub kind: String,
}

impl ProviderPayload {
    pub fn error(provider: &str, message: impl Into<String>) -> Self {
        Self {
            provider: provider.to_string(),
            account: None,
            version: None,
            source: "native".to_string(),
            status: None,
            usage: None,
            credits: None,
            error: Some(ProviderErrorPayload {
                code: 1,
                message: message.into(),
                kind: "provider".to_string(),
            }),
        }
    }

    pub fn ok(
        provider: &str,
        usage: UsageSnapshot,
        account: Option<String>,
        credits: Option<CreditsSnapshot>,
        status: Option<ProviderStatusPayload>,
    ) -> Self {
        Self {
            provider: provider.to_string(),
            account,
            version: None,
            source: "native".to_string(),
            status,
            usage: Some(usage),
            credits,
            error: None,
        }
    }
}

pub fn rate_window(used_percent: f64, window_minutes: Option<i64>, resets_at: Option<DateTime<Utc>>) -> RateWindow {
    let used = clamp_percent(used_percent);
    RateWindow {
        used_percent: used,
        remaining_percent: (100.0 - used).max(0.0),
        window_minutes,
        resets_at,
        reset_description: None,
    }
}

pub fn clamp_percent(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 100.0)
}
