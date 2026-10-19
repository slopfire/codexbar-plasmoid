//! CodeBuddy (codebuddy.ai, Tencent) usage.
//!
//! The CodeBuddy CLI stores its session in the extension data directory
//! (`~/.local/share/CodeBuddyExtension/Data/Public/auth/Tencent-Cloud.coding-copilot.info`
//! on Linux). The stored bearer token authenticates the public billing API:
//!
//! - `/billing/meter/get-user-resource-summary` — the plan name and one entry
//!   per credit package (the package codes the next call needs)
//! - `/billing/meter/get-user-resource-free-packages` — per-package names,
//!   cycle capacity, remaining capacity, and the cycle reset stamp
//!
//! CodeBuddy has no rolling 5-hour or weekly window: every allowance is a
//! monthly credit cycle, so each package becomes one usage row. Packs that
//! share a display name (a stack of "Bonus Pack" grants) merge into a single
//! row. The API never serves token spend, and the local transcripts carry no
//! per-request cost, so this provider reports quota only.

use crate::http::HttpClient;
use crate::output::{
    clamp_percent, rate_window, CreditsSnapshot, ProviderIdentitySnapshot, ProviderPayload,
    RateWindow, UsageRowSnapshot, UsageSnapshot,
};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_API_BASE: &str = "https://www.codebuddy.ai";
const SITE_URL: &str = "https://www.codebuddy.ai/";
const AUTH_FILE_NAME: &str = "Tencent-Cloud.coding-copilot.info";
const AUTH_SUBDIR: &str = "CodeBuddyExtension/Data/Public/auth";

/// Billing cycles are monthly, so a package window spans 30 days.
const MONTHLY_WINDOW_MINUTES: i64 = 30 * 24 * 60;

/// The API accepts any User-Agent, but send the product name so CodeBuddy's
/// gateway can attribute the traffic.
const USER_AGENT: &str = "codebuddy";

pub fn fetch(http: &HttpClient) -> ProviderPayload {
    match fetch_inner(http) {
        Ok(payload) => payload,
        Err(error) => ProviderPayload::error("codebuddy", error.to_string()),
    }
}

fn fetch_inner(http: &HttpClient) -> Result<ProviderPayload> {
    let session = resolve_session().ok_or_else(|| {
        anyhow!(
            "No CodeBuddy session found. Sign in with the CodeBuddy CLI (`codebuddy`), \
             or set CODEBUDDY_ACCESS_TOKEN / CODEBUDDY_AUTH_FILE."
        )
    })?;
    let base = api_base();

    let summary = fetch_summary(http, &base, &session)?;
    let codes = summary.package_codes();
    // Package names and cycle stamps live on the per-package endpoint. It is
    // decorative: a failure keeps the balances from the summary.
    let accounts = if codes.is_empty() {
        Vec::new()
    } else {
        fetch_accounts(http, &base, &session, &codes).unwrap_or_default()
    };

    let plan_code = summary.subscription_package_code.clone();
    let plan_label = summary
        .subscription_package_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let rows = usage_rows(&accounts, plan_code.as_deref());
    let credits = credits_snapshot(&accounts).or_else(|| summary.credits_snapshot());
    let primary = accounts
        .iter()
        .find(|account| plan_code.as_deref() == account.package_code.as_deref())
        .or_else(|| accounts.first())
        .and_then(account_window);

    let usage = UsageSnapshot {
        primary,
        secondary: None,
        tertiary: None,
        usage_rows: if rows.is_empty() { None } else { Some(rows) },
        provider_cost: None,
        cursor_requests: None,
        updated_at: Utc::now(),
        identity: Some(ProviderIdentitySnapshot {
            account_email: session.email.clone(),
            account_organization: None,
            login_method: plan_label,
        }),
    };

    let mut payload = ProviderPayload::ok("codebuddy", usage, session.email, credits, None);
    payload.site_url = Some(SITE_URL.to_string());
    Ok(payload)
}

fn api_base() -> String {
    std::env::var("CODEBUDDY_API_BASE")
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string())
}

fn fetch_summary(http: &HttpClient, base: &str, session: &Session) -> Result<ResourceSummary> {
    let body = serde_json::json!({});
    post_data(
        http,
        &format!("{base}/billing/meter/get-user-resource-summary"),
        session,
        &body,
        "CodeBuddy resource summary",
    )
}

fn fetch_accounts(
    http: &HttpClient,
    base: &str,
    session: &Session,
    codes: &[String],
) -> Result<Vec<AccountPackage>> {
    // `Status` must be an int array; a bare number is rejected with a 400.
    let body = serde_json::json!({
        "PackageCodes": codes,
        "PageNumber": 1,
        "PageSize": 20,
        "Status": [],
    });
    let data: PackagesData = post_data(
        http,
        &format!("{base}/billing/meter/get-user-resource-free-packages"),
        session,
        &body,
        "CodeBuddy package balances",
    )?;
    Ok(data.accounts)
}

/// POST a JSON body and unwrap the `{code, msg, data}` envelope. A non-zero
/// `code` is an API-level rejection; a non-2xx status is a transport-level one.
fn post_data<T: for<'de> Deserialize<'de>>(
    http: &HttpClient,
    url: &str,
    session: &Session,
    body: &serde_json::Value,
    what: &str,
) -> Result<T> {
    let (status, text) = http.post_bearer_text(url, &session.access_token, USER_AGENT, body)?;
    if !(200..300).contains(&status) {
        let hint = if status == 401 || status == 403 {
            if session.expired {
                " The stored CodeBuddy session has expired; sign in again with `codebuddy`."
            } else {
                " The stored CodeBuddy session was rejected; sign in again with `codebuddy`."
            }
        } else {
            ""
        };
        return Err(anyhow!("{what} returned HTTP {status}.{hint}"));
    }
    let envelope: Envelope<T> = serde_json::from_str(&text)
        .with_context(|| format!("parse {what} response"))?;
    if let Some(code) = envelope.code {
        if code != 0 {
            let message = envelope.msg.unwrap_or_else(|| "unknown error".to_string());
            return Err(anyhow!("{what} failed ({code}): {message}"));
        }
    }
    envelope
        .data
        .ok_or_else(|| anyhow!("{what} returned no data"))
}

struct Session {
    access_token: String,
    email: Option<String>,
    /// The token's own expiry stamp says it is already past.
    expired: bool,
}

/// Read the bearer token the CLI writes after sign-in. `CODEBUDDY_ACCESS_TOKEN`
/// and `CODEBUDDY_AUTH_FILE` allow a headless or non-default install.
fn resolve_session() -> Option<Session> {
    if let Some(token) = non_empty_env("CODEBUDDY_ACCESS_TOKEN") {
        return Some(Session {
            access_token: token,
            email: None,
            expired: false,
        });
    }
    let path = std::env::var("CODEBUDDY_AUTH_FILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| auth_candidates().into_iter().find(|path| path.is_file()))?;
    let raw = std::fs::read_to_string(path).ok()?;
    let auth: AuthFile = serde_json::from_str(&raw).ok()?;
    let access_token = auth
        .auth
        .as_ref()
        .and_then(|auth| auth.access_token.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let expired = auth
        .auth
        .as_ref()
        .and_then(|auth| auth.expires_at)
        .and_then(epoch_ms_to_datetime)
        .is_some_and(|expires_at| expires_at <= Utc::now());
    Some(Session {
        access_token,
        email: auth.account.and_then(|account| account.email()),
        expired,
    })
}

/// The CLI writes its credentials under the platform data directory. The
/// Linux path is also checked explicitly because `dirs::data_dir` follows
/// `XDG_DATA_HOME`, which the CLI ignores.
fn auth_candidates() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    if let Some(data) = dirs::data_dir() {
        paths.push(data.join(AUTH_SUBDIR).join(AUTH_FILE_NAME));
    }
    if let Some(local) = dirs::data_local_dir() {
        paths.push(local.join(AUTH_SUBDIR).join(AUTH_FILE_NAME));
    }
    if let Some(home) = dirs::home_dir() {
        paths.push(home.join(".local/share").join(AUTH_SUBDIR).join(AUTH_FILE_NAME));
        paths.push(home.join("AppData/Local").join(AUTH_SUBDIR).join(AUTH_FILE_NAME));
    }
    paths.dedup();
    paths
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Packages that share a display name, e.g. a stack of "Bonus Pack" grants.
struct PackageGroup<'a> {
    title: String,
    is_plan: bool,
    packages: Vec<&'a AccountPackage>,
}

/// A merged row plus the keys the final ordering needs.
struct MergedRow {
    is_plan: bool,
    granted: f64,
    row: UsageRowSnapshot,
}

/// One row per distinct package name, sorted so the subscription package leads
/// and larger allowances follow.
///
/// Packs that share a name are merged, and only the packs with credits left
/// count toward the merged balance. A spent pack would otherwise divide its
/// zero remainder by its full grant and drag a live pack down with it: a
/// drained 9999-credit pack next to a fresh 30-credit pack belongs at 100%, not
/// at 0.3%. A name whose packs are all spent is dropped outright — except the
/// subscription package, whose 0% is the signal that the plan ran out.
fn usage_rows(accounts: &[AccountPackage], plan_code: Option<&str>) -> Vec<UsageRowSnapshot> {
    let mut groups: Vec<PackageGroup> = Vec::new();
    for account in accounts {
        if account.capacity().is_none() {
            continue;
        }
        let is_plan = plan_code.is_some() && plan_code == account.package_code.as_deref();
        let title = account.title();
        match groups
            .iter_mut()
            .find(|group| group.is_plan == is_plan && group.title == title)
        {
            Some(group) => group.packages.push(account),
            None => groups.push(PackageGroup {
                title,
                is_plan,
                packages: vec![account],
            }),
        }
    }

    let mut merged: Vec<MergedRow> = groups
        .into_iter()
        .filter_map(|group| {
            let live: Vec<&AccountPackage> = group
                .packages
                .iter()
                .copied()
                .filter(|package| package.remaining() > 0.0)
                .collect();
            if live.is_empty() && !group.is_plan {
                return None;
            }
            let counted = if live.is_empty() { group.packages } else { live };
            let mut granted = 0.0;
            let mut remaining = 0.0;
            let mut resets_at = None;
            for package in counted {
                let Some((size, remain)) = package.capacity() else {
                    continue;
                };
                granted += size;
                remaining += remain;
                resets_at = earliest(
                    resets_at,
                    package.deduction_end_time.and_then(epoch_ms_to_datetime),
                );
            }
            if !(granted > 0.0) {
                return None;
            }
            Some(MergedRow {
                is_plan: group.is_plan,
                granted,
                row: UsageRowSnapshot {
                    // The name is the stable key: it survives packs joining
                    // and leaving the group, which the tray bar selection
                    // persists across refreshes.
                    id: group.title.clone(),
                    title: group.title,
                    percent_left: clamp_percent(remaining / granted * 100.0),
                    resets_at,
                },
            })
        })
        .collect();

    merged.sort_by(|left, right| {
        right
            .is_plan
            .cmp(&left.is_plan)
            .then_with(|| right.granted.total_cmp(&left.granted))
            .then_with(|| left.row.title.cmp(&right.row.title))
    });
    merged.into_iter().map(|merged| merged.row).collect()
}

/// Merged rows expire when their first pack does, so the soonest stamp wins.
fn earliest(
    current: Option<DateTime<Utc>>,
    candidate: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (None, candidate) => candidate,
        (current, None) => current,
    }
}

fn credits_snapshot(accounts: &[AccountPackage]) -> Option<CreditsSnapshot> {
    let mut remaining = 0.0;
    let mut used = 0.0;
    let mut limit = 0.0;
    let mut found = false;
    for account in accounts {
        let Some((size, remain)) = account.capacity() else {
            continue;
        };
        found = true;
        limit += size;
        remaining += remain;
        used += (size - remain).max(0.0);
    }
    found.then(|| CreditsSnapshot {
        remaining: Some(remaining),
        used,
        limit: Some(limit),
        updated_at: Utc::now(),
    })
}

/// The subscription package is the one window worth mirroring into `primary`
/// for consumers that only read the legacy fields.
fn account_window(account: &AccountPackage) -> Option<RateWindow> {
    let (size, remain) = account.capacity()?;
    let used_percent = clamp_percent((size - remain).max(0.0) / size * 100.0);
    Some(rate_window(
        used_percent,
        Some(MONTHLY_WINDOW_MINUTES),
        account.deduction_end_time.and_then(epoch_ms_to_datetime),
    ))
}

/// Billing stamps are epoch milliseconds (seconds are tolerated too).
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

/// Numeric fields arrive as JSON numbers on one endpoint and as numeric strings
/// on the other, so accept both.
fn opt_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }))
}

#[derive(Debug, Deserialize)]
struct AuthFile {
    account: Option<AuthAccount>,
    auth: Option<AuthSession>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthAccount {
    nickname: Option<String>,
    uin: Option<String>,
}

impl AuthAccount {
    /// The account nickname is the sign-in email; `uin` is the numeric Tencent
    /// account id and makes a poor display name.
    fn email(self) -> Option<String> {
        self.nickname
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty() && value.contains('@'))
            .or_else(|| {
                self.uin
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthSession {
    access_token: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    expires_at: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    code: Option<i64>,
    msg: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ResourceSummary {
    #[serde(default)]
    packages: Vec<SummaryPackage>,
    subscription_package_code: Option<String>,
    subscription_package_name: Option<String>,
}

impl ResourceSummary {
    fn package_codes(&self) -> Vec<String> {
        self.packages
            .iter()
            .filter_map(|package| package.package_code.as_deref())
            .map(str::trim)
            .filter(|code| !code.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Balances without names or cycle stamps, used when the per-package
    /// endpoint does not answer.
    fn credits_snapshot(&self) -> Option<CreditsSnapshot> {
        let mut remaining = 0.0;
        let mut used = 0.0;
        let mut limit = 0.0;
        let mut found = false;
        for package in &self.packages {
            let (Some(size), Some(remain)) = (package.cycle_total_capacity, package.cycle_remain_capacity)
            else {
                continue;
            };
            if !(size.is_finite() && size > 0.0) {
                continue;
            }
            found = true;
            limit += size;
            remaining += remain.max(0.0);
            used += (size - remain.max(0.0)).max(0.0);
        }
        found.then(|| CreditsSnapshot {
            remaining: Some(remaining),
            used,
            limit: Some(limit),
            updated_at: Utc::now(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SummaryPackage {
    package_code: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    cycle_total_capacity: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    cycle_remain_capacity: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PackagesData {
    #[serde(default)]
    accounts: Vec<AccountPackage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AccountPackage {
    package_code: Option<String>,
    package_name: Option<String>,
    #[serde(default, deserialize_with = "opt_f64")]
    cycle_capacity_size: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    cycle_capacity_size_precise: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    cycle_capacity_remain: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    cycle_capacity_remain_precise: Option<f64>,
    #[serde(default, deserialize_with = "opt_f64")]
    deduction_end_time: Option<f64>,
}

impl AccountPackage {
    /// Cycle capacity in credits. The `*Precise` strings are the exact values;
    /// the integer fields are the rounded fallback.
    fn capacity(&self) -> Option<(f64, f64)> {
        let size = self
            .cycle_capacity_size_precise
            .or(self.cycle_capacity_size)
            .filter(|value| value.is_finite() && *value > 0.0)?;
        let remain = self
            .cycle_capacity_remain_precise
            .or(self.cycle_capacity_remain)
            .unwrap_or(0.0)
            .clamp(0.0, size);
        Some((size, remain))
    }

    fn title(&self) -> String {
        self.package_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| "Credits".to_string())
    }

    /// Credits still available on this pack; a spent pack leaves zero.
    fn remaining(&self) -> f64 {
        self.capacity().map(|(_, remain)| remain).unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accounts_from(raw: &str) -> Vec<AccountPackage> {
        serde_json::from_str::<PackagesData>(raw)
            .expect("packages JSON should deserialize")
            .accounts
    }

    #[test]
    fn summary_reads_the_capitalized_package_list() {
        // Shape captured from POST /billing/meter/get-user-resource-summary.
        let summary: ResourceSummary = serde_json::from_str(
            r#"{
              "Packages": [
                {"PackageCode": "TCACA_code_002", "CycleTotalCapacity": "1000", "CycleRemainCapacity": "1000", "CycleUsedCapacity": "0", "CycleFrozenCapacity": "0", "CapacityUnit": "credit", "TotalCount": 1},
                {"PackageCode": "TCACA_code_007", "CycleTotalCapacity": "30", "CycleRemainCapacity": "30", "CycleUsedCapacity": "0", "CycleFrozenCapacity": "0", "CapacityUnit": "credit", "TotalCount": 1}
              ],
              "SubscriptionPackageCode": "TCACA_code_002",
              "SubscriptionPackageName": "Pro Monthly",
              "IsPaidUser": true,
              "ProTrialStatus": 1
            }"#,
        )
        .expect("summary should deserialize");

        assert_eq!(summary.package_codes(), vec!["TCACA_code_002", "TCACA_code_007"]);
        let credits = summary.credits_snapshot().expect("credits");
        assert_eq!(credits.remaining, Some(1030.0));
        assert_eq!(credits.limit, Some(1030.0));
        assert_eq!(credits.used, 0.0);
    }

    #[test]
    fn package_balances_become_usage_rows() {
        // Shape captured from POST /billing/meter/get-user-resource-free-packages.
        let accounts = accounts_from(
            r#"{
              "Accounts": [
                {"PackageCode": "TCACA_code_002", "PackageName": "Pro Plan Monthly Subscription", "CycleCapacitySize": 1000, "CycleCapacitySizePrecise": "1000", "CycleCapacityRemain": 250, "CycleCapacityRemainPrecise": "250", "CapacityUnit": "credit", "DeductionEndTime": 1794971474000, "Status": 0},
                {"PackageCode": "TCACA_code_007", "PackageName": "Bonus Pack", "CycleCapacitySize": 30, "CycleCapacitySizePrecise": "30", "CycleCapacityRemain": 30, "CycleCapacityRemainPrecise": "30", "CapacityUnit": "credit", "DeductionEndTime": 1794962813000, "Status": 0}
              ],
              "TotalCount": 2
            }"#,
        );

        let rows = usage_rows(&accounts, Some("TCACA_code_002"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "Pro Plan Monthly Subscription");
        assert_eq!(rows[0].title, "Pro Plan Monthly Subscription");
        assert!((rows[0].percent_left - 25.0).abs() < 1e-9);
        assert_eq!(
            rows[0].resets_at.map(|value| value.timestamp_millis()),
            Some(1794971474000)
        );
        assert_eq!(rows[1].title, "Bonus Pack");
        assert!((rows[1].percent_left - 100.0).abs() < 1e-9);

        let credits = credits_snapshot(&accounts).expect("credits");
        assert_eq!(credits.remaining, Some(280.0));
        assert_eq!(credits.used, 750.0);
        assert_eq!(credits.limit, Some(1030.0));
    }

    #[test]
    fn the_subscription_package_leads_then_larger_allowances_follow() {
        let accounts = accounts_from(
            r#"{
              "Accounts": [
                {"PackageCode": "bonus", "PackageName": "Bonus Pack", "CycleCapacitySize": 30, "CycleCapacityRemain": 30},
                {"PackageCode": "extra", "PackageName": "Plan Bonus Quota", "CycleCapacitySize": 1000, "CycleCapacityRemain": 1000},
                {"PackageCode": "plan", "PackageName": "Pro Plan Monthly Subscription", "CycleCapacitySize": 1000, "CycleCapacityRemain": 1000}
              ]
            }"#,
        );

        let rows = usage_rows(&accounts, Some("plan"));
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "Pro Plan Monthly Subscription",
                "Plan Bonus Quota",
                "Bonus Pack"
            ]
        );
    }

    #[test]
    fn a_package_without_capacity_is_skipped() {
        let accounts = accounts_from(
            r#"{
              "Accounts": [
                {"PackageCode": "empty", "PackageName": "Expired Pack", "CycleCapacitySize": 0, "CycleCapacityRemain": 0},
                {"PackageCode": "plan", "PackageName": "Pro Plan Monthly Subscription", "CycleCapacitySize": 1000, "CycleCapacityRemain": 900}
              ]
            }"#,
        );

        let rows = usage_rows(&accounts, Some("plan"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "Pro Plan Monthly Subscription");
        assert!(credits_snapshot(&accounts).is_some());
    }

    #[test]
    fn same_named_packs_merge_and_leave_spent_packs_out() {
        // Two "Bonus Pack" grants: one long drained, one untouched. The drained
        // pack must not drag the merged balance down with its spent credits.
        let accounts = accounts_from(
            r#"{
              "Accounts": [
                {"PackageCode": "bonus-old", "PackageName": "Bonus Pack", "CycleCapacitySize": 9999, "CycleCapacityRemain": 0, "DeductionEndTime": 1794962813000},
                {"PackageCode": "bonus-new", "PackageName": "Bonus Pack", "CycleCapacitySize": 30, "CycleCapacityRemain": 30, "DeductionEndTime": 1797971474000}
              ]
            }"#,
        );

        let rows = usage_rows(&accounts, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "Bonus Pack");
        assert_eq!(rows[0].percent_left, 100.0);
        // The merged row expires with its first live pack.
        assert_eq!(
            rows[0].resets_at.map(|value| value.timestamp_millis()),
            Some(1797971474000)
        );
    }

    #[test]
    fn merged_packs_weight_their_live_credits_together() {
        let accounts = accounts_from(
            r#"{
              "Accounts": [
                {"PackageCode": "bonus-a", "PackageName": "Bonus Pack", "CycleCapacitySize": 30, "CycleCapacityRemain": 15},
                {"PackageCode": "bonus-b", "PackageName": "Bonus Pack", "CycleCapacitySize": 10, "CycleCapacityRemain": 10},
                {"PackageCode": "drained", "PackageName": "Bonus Pack", "CycleCapacitySize": 500, "CycleCapacityRemain": 0}
              ]
            }"#,
        );

        let rows = usage_rows(&accounts, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].percent_left, 62.5);
    }

    #[test]
    fn a_spent_pack_with_no_plan_is_dropped() {
        let accounts = accounts_from(
            r#"{
              "Accounts": [
                {"PackageCode": "bonus", "PackageName": "Bonus Pack", "CycleCapacitySize": 30, "CycleCapacityRemain": 0},
                {"PackageCode": "plan", "PackageName": "Pro Plan Monthly Subscription", "CycleCapacitySize": 1000, "CycleCapacityRemain": 900}
              ]
            }"#,
        );

        let rows = usage_rows(&accounts, Some("plan"));
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["Pro Plan Monthly Subscription"]);
    }

    #[test]
    fn a_fully_used_package_keeps_zero_percent_left() {
        let accounts = accounts_from(
            r#"{"Accounts":[{"PackageCode":"plan","PackageName":"Pro Plan Monthly Subscription","CycleCapacitySize":1000,"CycleCapacityRemain":0}]}"#,
        );

        let rows = usage_rows(&accounts, Some("plan"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].percent_left, 0.0);
        let window = account_window(&accounts[0]).expect("primary window");
        assert_eq!(window.used_percent, 100.0);
        assert_eq!(window.remaining_percent, 0.0);
    }

    #[test]
    fn epoch_milliseconds_convert_to_utc_datetimes() {
        assert_eq!(
            epoch_ms_to_datetime(1794971474000.0).map(|value| value.to_rfc3339()),
            Some("2026-11-18T03:11:14+00:00".to_string())
        );
        // Seconds are tolerated.
        assert_eq!(
            epoch_ms_to_datetime(1794971474.0).map(|value| value.timestamp()),
            Some(1794971474)
        );
        assert!(epoch_ms_to_datetime(0.0).is_none());
        assert!(epoch_ms_to_datetime(f64::NAN).is_none());
    }

    #[test]
    fn auth_file_reads_the_camel_case_token_and_email() {
        let auth: AuthFile = serde_json::from_str(
            r#"{
              "account": {"uid": "d5f39fd7", "nickname": "dev@example.com", "uin": "450701651066", "type": "personal"},
              "auth": {"accessToken": "token-value", "refreshToken": "refresh-value", "tokenType": "Bearer", "domain": "www.codebuddy.ai", "expiresAt": 1823774280965}
            }"#,
        )
        .expect("auth file should deserialize");

        let session = auth.auth.expect("auth session");
        assert_eq!(session.access_token.as_deref(), Some("token-value"));
        assert_eq!(
            session.expires_at.and_then(epoch_ms_to_datetime).map(|value| value.timestamp_millis()),
            Some(1823774280965)
        );
        assert_eq!(
            auth.account.and_then(AuthAccount::email).as_deref(),
            Some("dev@example.com")
        );
    }

    #[test]
    fn numeric_fields_accept_strings_and_numbers() {
        let accounts = accounts_from(
            r#"{"Accounts":[{"PackageCode":"plan","PackageName":"Pro Plan","CycleCapacitySize":1000,"CycleCapacitySizePrecise":"1000.5","CycleCapacityRemain":10,"CycleCapacityRemainPrecise":"10.5"}]}"#,
        );
        let (size, remain) = accounts[0].capacity().expect("capacity");
        assert_eq!(size, 1000.5);
        assert_eq!(remain, 10.5);
    }
}
