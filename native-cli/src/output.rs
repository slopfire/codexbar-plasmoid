use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPayload {
    pub provider: String,
    pub account: Option<String>,
    pub version: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site_url: Option<String>,
    pub status: Option<ProviderStatusPayload>,
    pub usage: Option<UsageSnapshot>,
    pub credits: Option<CreditsSnapshot>,
    pub error: Option<ProviderErrorPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatusPayload {
    pub indicator: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary: Option<RateWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<RateWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tertiary: Option<RateWindow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_rows: Option<Vec<UsageRowSnapshot>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_cost: Option<ProviderCostSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_requests: Option<CursorRequestUsage>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<ProviderIdentitySnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRowSnapshot {
    pub id: String,
    pub title: String,
    pub percent_left: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorRequestUsage {
    pub used: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderIdentitySnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_email: Option<String>,
    pub account_organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_method: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditsSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<f64>,
    pub used: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            site_url: None,
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
            site_url: None,
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

impl ProviderPayload {
    pub fn anonymize_emails(&mut self) {
        if let Some(ref email) = self.account {
            self.account = Some(anonymize_identity_str(email));
        }
        if let Some(ref mut usage) = self.usage {
            if let Some(ref mut identity) = usage.identity {
                if let Some(ref email) = identity.account_email {
                    identity.account_email = Some(anonymize_identity_str(email));
                }
                if let Some(ref org) = identity.account_organization {
                    identity.account_organization = Some(anonymize_identity_str(org));
                }
            }
        }
    }
}

/// Mask emails and opaque ids (UUIDs, long hex tokens) for UI display.
pub fn anonymize_identity_str(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return value.to_string();
    }
    if trimmed.contains('@') {
        return anonymize_email_str(trimmed);
    }
    if looks_like_opaque_id(trimmed) {
        return anonymize_opaque_id(trimmed);
    }
    value.to_string()
}

pub fn anonymize_email_str(email: &str) -> String {
    if !email.contains('@') {
        return email.to_string();
    }
    let parts: Vec<&str> = email.split('@').collect();
    if parts.len() != 2 {
        return email.to_string();
    }
    let local = parts[0];
    let domain = parts[1];
    if local.chars().count() <= 2 {
        let masked_local: String = local.chars().take(1).chain(std::iter::once('*')).collect();
        format!("{}@{}", masked_local, domain)
    } else {
        let first = local.chars().next().unwrap_or('*');
        let last = local.chars().last().unwrap_or('*');
        let stars: String = std::iter::repeat('*').take(local.chars().count() - 2).collect();
        format!("{}{}{}@{}", first, stars, last, domain)
    }
}

fn looks_like_opaque_id(value: &str) -> bool {
    // UUID: 8-4-4-4-12 hex
    if is_uuid_like(value) {
        return true;
    }
    // Long hex / base64-ish tokens (team ids, org ids, user ids without @).
    let alnum: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    alnum.len() >= 16
        && alnum
            .chars()
            .all(|c| c.is_ascii_hexdigit() || matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9'))
        && alnum.chars().filter(|c| c.is_ascii_hexdigit()).count() * 2 >= alnum.len()
}

fn is_uuid_like(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let expected = [8, 4, 4, 4, 12];
    parts.iter().zip(expected).all(|(part, len)| {
        part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit())
    })
}

fn anonymize_opaque_id(value: &str) -> String {
    if is_uuid_like(value) {
        // 8f3b78b6-3140-4f03-917d-0c96638112db → 8f3b****-****-****-****-********12db
        let parts: Vec<&str> = value.split('-').collect();
        return format!(
            "{}****-****-****-****-********{}",
            &parts[0][..4],
            &parts[4][parts[4].len().saturating_sub(4)..]
        );
    }
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= 8 {
        let keep = 1.min(chars.len());
        let stars = "*".repeat(chars.len().saturating_sub(keep));
        return format!("{}{}", chars.iter().take(keep).collect::<String>(), stars);
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    let stars = "*".repeat(chars.len().saturating_sub(8));
    format!("{head}{stars}{tail}")
}

#[cfg(test)]
mod anonymize_tests {
    use super::*;

    #[test]
    fn test_anonymize_email() {
        assert_eq!(anonymize_email_str("a@example.com"), "a*@example.com");
        assert_eq!(anonymize_email_str("ab@example.com"), "a*@example.com");
        assert_eq!(anonymize_email_str("abc@example.com"), "a*c@example.com");
        assert_eq!(anonymize_email_str("user@example.com"), "u**r@example.com");
        assert_eq!(anonymize_email_str("username@example.com"), "u******e@example.com");
        assert_eq!(anonymize_email_str("not_an_email"), "not_an_email");
    }

    #[test]
    fn test_anonymize_uuid_and_identity() {
        assert_eq!(
            anonymize_identity_str("8f3b78b6-3140-4f03-917d-0c96638112db"),
            "8f3b****-****-****-****-********12db"
        );
        assert_eq!(
            anonymize_identity_str("user@example.com"),
            "u**r@example.com"
        );
        // Human-readable org names stay readable.
        assert_eq!(anonymize_identity_str("Acme Corp"), "Acme Corp");
    }
}
