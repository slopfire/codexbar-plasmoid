use crate::cookies::resolve_all_cookie_headers;
use crate::http::HttpClient;
use crate::output::{
    rate_window, ProviderIdentitySnapshot, ProviderPayload, UsageSnapshot,
};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE, COOKIE};
use serde::Deserialize;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;

const BILLING_URL: &str = "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";
const SESSION_URL: &str = "https://grok.com/api/auth/session";
const USERINFO_URL: &str = "https://auth.x.ai/oauth2/userinfo";

/// Empty gRPC-web framed protobuf message (flag 0 + length 0).
const EMPTY_GRPC_WEB_FRAME: &[u8] = &[0x00, 0x00, 0x00, 0x00, 0x00];

/// Fetch every visible Grok account: `grok login` credentials in `~/.grok/auth.json`
/// plus browser `sso` sessions (Chrome, Zen, Firefox, …). Returns one payload per
/// distinct account so the plasmoid can show both SuperGrok CLI and grok.com
/// browser logins side by side.
pub fn fetch(http: &HttpClient) -> Vec<ProviderPayload> {
    let mut payloads = Vec::new();
    let mut seen_keys = HashSet::new();

    for auth in load_auth_credentials() {
        match fetch_bearer(http, &auth) {
            Ok(payload) => push_unique(&mut payloads, &mut seen_keys, payload),
            Err(error) => payloads.push(ProviderPayload::error(
                "grok",
                format!(
                    "Grok CLI auth ({}): {error}",
                    auth.email.as_deref().unwrap_or("unknown")
                ),
            )),
        }
    }

    for cookie in resolve_all_cookie_headers("grok") {
        // Skip cookie headers that don't include the sso session token.
        if !cookie.header.contains("sso=") {
            continue;
        }
        match fetch_cookie(http, &cookie.header, &cookie.source) {
            Ok(payload) => push_unique(&mut payloads, &mut seen_keys, payload),
            Err(error) => {
                // Soft-fail individual browser sessions so a stale Chrome cookie
                // does not hide a working Zen login (or vice versa).
                let mut payload = ProviderPayload::error(
                    "grok",
                    format!("Grok browser session ({}): {error}", cookie.source),
                );
                payload.source = cookie.source.clone();
                // Only surface the error when we have no successes yet; otherwise
                // keep quiet about stale secondary stores.
                if payloads.iter().all(|p| p.error.is_some()) {
                    payloads.push(payload);
                }
            }
        }
    }

    if payloads.is_empty() {
        payloads.push(ProviderPayload::error(
            "grok",
            "No Grok session found. Run `grok login`, or sign in to grok.com in Chrome/Zen.",
        ));
    }

    // Prefer successful entries; if any succeeded, drop pure error leftovers so
    // the widget does not show a failing Chrome cookie next to a good Zen login.
    let successes: Vec<_> = payloads.iter().filter(|p| p.error.is_none()).cloned().collect();
    if !successes.is_empty() {
        return successes;
    }
    // All failed — keep a single first error.
    payloads.into_iter().take(1).collect()
}

fn push_unique(
    payloads: &mut Vec<ProviderPayload>,
    seen: &mut HashSet<String>,
    payload: ProviderPayload,
) {
    let key = account_key(&payload);
    if seen.insert(key) {
        payloads.push(payload);
    }
}

fn account_key(payload: &ProviderPayload) -> String {
    if let Some(email) = payload
        .account
        .as_deref()
        .or_else(|| {
            payload
                .usage
                .as_ref()
                .and_then(|u| u.identity.as_ref())
                .and_then(|i| i.account_email.as_deref())
        })
        .map(str::to_lowercase)
    {
        return format!("email:{email}");
    }
    if let Some(org) = payload
        .usage
        .as_ref()
        .and_then(|u| u.identity.as_ref())
        .and_then(|i| i.account_organization.as_deref())
    {
        return format!("org:{org}");
    }
    format!(
        "src:{}:{}",
        payload.source,
        payload.account.as_deref().unwrap_or("?")
    )
}

#[derive(Debug, Clone)]
struct AuthCredential {
    bearer: String,
    email: Option<String>,
    team_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthEntry {
    key: Option<String>,
    email: Option<String>,
    team_id: Option<String>,
    auth_mode: Option<String>,
    expires_at: Option<String>,
}

fn grok_home() -> PathBuf {
    env::var("GROK_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join(".grok")
        })
}

fn load_auth_credentials() -> Vec<AuthCredential> {
    let path = grok_home().join("auth.json");
    let Ok(raw) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&raw) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (_scope, value) in map {
        let Ok(entry) = serde_json::from_value::<AuthEntry>(value) else {
            continue;
        };
        let Some(bearer) = entry.key.filter(|k| !k.trim().is_empty()) else {
            continue;
        };
        // Do not pre-skip on expires_at: the access token may still be accepted
        // briefly after the local expiry stamp, and a 401 from the API is a
        // clearer signal to re-run `grok login` than silently dropping the account.
        let _ = entry.expires_at;
        // auth_mode is login transport only; plan is SuperGrok product name.
        let _ = entry.auth_mode;
        out.push(AuthCredential {
            bearer,
            email: entry.email.filter(|e| !e.is_empty()),
            team_id: entry.team_id.filter(|t| !t.is_empty()),
        });
    }
    out
}

fn fetch_bearer(http: &HttpClient, auth: &AuthCredential) -> Result<ProviderPayload> {
    let mut headers = grpc_headers()?;
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", auth.bearer))
            .context("invalid bearer token")?,
    );
    let billing = fetch_billing(http, &headers)?;

    let mut email = auth.email.clone();
    let team_id = auth.team_id.clone();
    if email.is_none() {
        if let Ok(info) = fetch_userinfo(http, &auth.bearer) {
            email = info.email.or(email);
            // userinfo does not expose team; keep auth.json team_id
        }
    }

    // Product plan for Grok Build / grok.com paid weekly limit (UI: "Weekly SuperGrok Limit").
    // auth_mode is the login transport (oidc/…), not a plan name — do not surface it as Plan.
    let plan = Some("SuperGrok".to_string());

    Ok(build_payload(
        billing,
        email,
        team_id,
        plan,
        "native-auth",
    ))
}

fn fetch_cookie(http: &HttpClient, cookie_header: &str, source_label: &str) -> Result<ProviderPayload> {
    let mut headers = grpc_headers()?;
    headers.insert(
        COOKIE,
        HeaderValue::from_str(cookie_header).context("invalid cookie header")?,
    );
    let billing = fetch_billing(http, &headers)?;

    let session = fetch_web_session(http, cookie_header).ok();
    let email = session.as_ref().and_then(|s| s.email.clone());
    let user_id = session.as_ref().and_then(|s| s.user_id.clone());
    // Same weekly SuperGrok limit product as native-auth; browser vs CLI is `source`, not Plan.
    let plan = Some("SuperGrok".to_string());

    Ok(build_payload(
        billing,
        email.or(user_id),
        session.and_then(|s| s.organization_id).filter(|s| !s.is_empty()),
        plan,
        source_label,
    ))
}

fn build_payload(
    billing: BillingSnapshot,
    email: Option<String>,
    organization: Option<String>,
    // Product plan name (shown as Plan: …). Not login transport.
    plan: Option<String>,
    source: &str,
) -> ProviderPayload {
    let window_minutes = billing
        .resets_at
        .map(|end| {
            let start = billing.period_start.unwrap_or_else(Utc::now);
            end.signed_duration_since(start).num_minutes().max(0)
        });

    let mut payload = ProviderPayload::ok(
        "grok",
        UsageSnapshot {
            primary: Some(rate_window(billing.used_percent, window_minutes, billing.resets_at)),
            secondary: None,
            tertiary: None,
            usage_rows: None,
            provider_cost: None,
            cursor_requests: None,
            updated_at: Utc::now(),
            identity: Some(ProviderIdentitySnapshot {
                account_email: email.clone(),
                account_organization: organization,
                // login_method field is mapped to Plan in the plasmoid helper.
                login_method: plan,
            }),
        },
        email,
        None,
        None,
    );
    payload.source = source.to_string();
    payload
}

struct BillingSnapshot {
    used_percent: f64,
    period_start: Option<DateTime<Utc>>,
    resets_at: Option<DateTime<Utc>>,
}

fn grpc_headers() -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/grpc-web+proto"),
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/grpc-web+proto"));
    headers.insert("x-grpc-web", HeaderValue::from_static("1"));
    headers.insert("Origin", HeaderValue::from_static("https://grok.com"));
    headers.insert("Referer", HeaderValue::from_static("https://grok.com/"));
    Ok(headers)
}

fn fetch_billing(http: &HttpClient, headers: &HeaderMap) -> Result<BillingSnapshot> {
    let bytes = http.post_bytes(BILLING_URL, headers, EMPTY_GRPC_WEB_FRAME)?;
    parse_billing_response(&bytes)
}

fn parse_billing_response(bytes: &[u8]) -> Result<BillingSnapshot> {
    // gRPC-web response: data frames (flag bit7 clear) then trailers (flag bit7 set).
    let mut offset = 0;
    let mut data_payload: Option<&[u8]> = None;
    while offset + 5 <= bytes.len() {
        let flag = bytes[offset];
        let len = u32::from_be_bytes([
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
            bytes[offset + 4],
        ]) as usize;
        offset += 5;
        if offset + len > bytes.len() {
            break;
        }
        let payload = &bytes[offset..offset + len];
        offset += len;
        if flag & 0x80 != 0 {
            let trailer = String::from_utf8_lossy(payload);
            if trailer.contains("grpc-status:0") || trailer.contains("grpc-status: 0") {
                continue;
            }
            if let Some(status) = trailer.lines().find(|l| l.starts_with("grpc-status:")) {
                let code = status.trim_start_matches("grpc-status:").trim();
                if code != "0" {
                    let msg = trailer
                        .lines()
                        .find(|l| l.starts_with("grpc-message:"))
                        .map(|l| {
                            l.trim_start_matches("grpc-message:")
                                .trim()
                                .replace("%20", " ")
                        })
                        .unwrap_or_else(|| format!("grpc-status {code}"));
                    return Err(anyhow!("Grok billing failed: {msg}"));
                }
            }
        } else if !payload.is_empty() {
            data_payload = Some(payload);
        }
    }

    let payload = data_payload.ok_or_else(|| anyhow!("Empty Grok billing response"))?;
    // Outer message field 1 = current period config.
    let period = find_length_delimited(payload, 1)
        .ok_or_else(|| anyhow!("Missing Grok credits period in billing response"))?;

    // Field 1 = credit_usage_percent (fixed32 float). Omitted means 0% used.
    let used_percent = find_fixed32(period, 1)
        .map(f32::from_bits)
        .map(|v| f64::from(v))
        .unwrap_or(0.0);

    // Field 4/5 = google.protobuf.Timestamp { seconds = 1, nanos = 2 }.
    let period_start = find_length_delimited(period, 4)
        .and_then(|ts| find_varint(ts, 1))
        .and_then(|secs| Utc.timestamp_opt(secs as i64, 0).single());
    let resets_at = find_length_delimited(period, 5)
        .and_then(|ts| find_varint(ts, 1))
        .and_then(|secs| Utc.timestamp_opt(secs as i64, 0).single());

    Ok(BillingSnapshot {
        used_percent,
        period_start,
        resets_at,
    })
}

#[derive(Debug, Deserialize)]
struct WebSessionResponse {
    status: Option<String>,
    session: Option<WebSession>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebSession {
    user_id: Option<String>,
    email: Option<String>,
    organization_id: Option<String>,
}

fn fetch_web_session(http: &HttpClient, cookie_header: &str) -> Result<WebSession> {
    let mut headers = HeaderMap::new();
    headers.insert(
        COOKIE,
        HeaderValue::from_str(cookie_header).context("invalid cookie header")?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert("Referer", HeaderValue::from_static("https://grok.com/"));
    let value = http.fetch_json_value(SESSION_URL, &headers)?;
    let parsed: WebSessionResponse = serde_json::from_value(value).context("parse session")?;
    if parsed.status.as_deref() != Some("authenticated") {
        return Err(anyhow!("Grok browser session is not authenticated"));
    }
    parsed
        .session
        .ok_or_else(|| anyhow!("Missing Grok session payload"))
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    email: Option<String>,
}

fn fetch_userinfo(http: &HttpClient, bearer: &str) -> Result<UserInfo> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {bearer}")).context("invalid bearer")?,
    );
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    let value = http.fetch_json_value(USERINFO_URL, &headers)?;
    serde_json::from_value(value).context("parse userinfo")
}

// --- minimal protobuf field readers (proto3 wire types) ---

fn read_varint(buf: &[u8], mut offset: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    while offset < buf.len() {
        let byte = buf[offset];
        offset += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, offset));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    None
}

/// Walk protobuf fields; `visit` receives (field, wire, start, end) into `buf`.
/// Return false from visit to stop early.
fn for_each_field(buf: &[u8], mut visit: impl FnMut(u32, u32, usize, usize) -> bool) {
    let mut offset = 0;
    while offset < buf.len() {
        let Some((key, next)) = read_varint(buf, offset) else {
            break;
        };
        offset = next;
        let field = (key >> 3) as u32;
        let wire = (key & 0x7) as u32;
        match wire {
            0 => {
                let Some((_, next)) = read_varint(buf, offset) else {
                    break;
                };
                if !visit(field, wire, offset, next) {
                    return;
                }
                offset = next;
            }
            1 => {
                if offset + 8 > buf.len() {
                    break;
                }
                let next = offset + 8;
                if !visit(field, wire, offset, next) {
                    return;
                }
                offset = next;
            }
            2 => {
                let Some((len, next)) = read_varint(buf, offset) else {
                    break;
                };
                let len = len as usize;
                if next + len > buf.len() {
                    break;
                }
                let end = next + len;
                if !visit(field, wire, next, end) {
                    return;
                }
                offset = end;
            }
            5 => {
                if offset + 4 > buf.len() {
                    break;
                }
                let next = offset + 4;
                if !visit(field, wire, offset, next) {
                    return;
                }
                offset = next;
            }
            _ => break,
        }
    }
}

fn find_length_delimited<'a>(buf: &'a [u8], field_num: u32) -> Option<&'a [u8]> {
    let mut range = None;
    for_each_field(buf, |field, wire, start, end| {
        if field == field_num && wire == 2 {
            range = Some((start, end));
            false
        } else {
            true
        }
    });
    range.map(|(start, end)| &buf[start..end])
}

fn find_fixed32(buf: &[u8], field_num: u32) -> Option<u32> {
    let mut found = None;
    for_each_field(buf, |field, wire, start, end| {
        if field == field_num && wire == 5 && end - start == 4 {
            found = Some(u32::from_le_bytes([
                buf[start],
                buf[start + 1],
                buf[start + 2],
                buf[start + 3],
            ]));
            false
        } else {
            true
        }
    });
    found
}

fn find_varint(buf: &[u8], field_num: u32) -> Option<u64> {
    let mut found = None;
    for_each_field(buf, |field, wire, start, end| {
        if field == field_num && wire == 0 {
            if let Some((v, _)) = read_varint(&buf[start..end], 0) {
                found = Some(v);
            }
            false
        } else {
            true
        }
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_billing_frame() {
        // Synthesize: field1 { field1 fixed32 15.0 }
        // outer key 0x0a = field1 len-delimited
        // inner: key 0x0d = field1 fixed32, then 00 00 70 41 (15.0f)
        let inner = [0x0d, 0x00, 0x00, 0x70, 0x41];
        let mut msg = vec![0x0a, inner.len() as u8];
        msg.extend_from_slice(&inner);
        // wrap as grpc-web frame
        let mut frame = vec![0x00, 0x00, 0x00, 0x00, msg.len() as u8];
        frame.extend_from_slice(&msg);
        frame.extend_from_slice(b"\x80\x00\x00\x00\x0fgrpc-status:0\r\n");
        let snap = parse_billing_response(&frame).unwrap();
        assert!((snap.used_percent - 15.0).abs() < 0.01);
    }
}
