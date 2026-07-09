use crate::http::HttpClient;
use crate::output::{
    clamp_percent, rate_window, ProviderIdentitySnapshot, ProviderPayload, ProviderStatusPayload,
    UsageRowSnapshot, UsageSnapshot,
};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const GET_USER_STATUS_PATH: &str = "/exa.language_server_pb.LanguageServerService/GetUserStatus";
const GET_COMMAND_MODEL_CONFIGS_PATH: &str =
    "/exa.language_server_pb.LanguageServerService/GetCommandModelConfigs";
const GET_UNLEASH_DATA_PATH: &str = "/exa.language_server_pb.LanguageServerService/GetUnleashData";
const RETRIEVE_USER_QUOTA_SUMMARY_PATH: &str =
    "/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary";
const QUOTA_SUMMARY_ROW_ORDER: &[(&str, &str, &str)] = &[
    ("gemini-5h", "gemini-five-hour", "Gemini five-hour limit"),
    ("gemini-weekly", "gemini-weekly", "Gemini weekly limit"),
    ("3p-5h", "claude-gpt-five-hour", "Claude/GPT five-hour limit"),
    ("3p-weekly", "claude-gpt-weekly", "Claude/GPT weekly limit"),
];
const NOT_RUNNING_MESSAGE: &str =
    "Antigravity is not running. Start agy or the Antigravity IDE first.";

/// Which fetch strategy the Antigravity provider should use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntigravityMethod {
    /// Try the local IDE language server, then fall back to Cloud Code OAuth
    /// when the IDE is not running (mirrors `antigravity-usage` auto mode).
    Auto,
    /// Only probe the local IDE language server.
    #[allow(dead_code)]
    Local,
    /// Only use the Google Cloud Code API with OAuth tokens stored by
    /// `antigravity-usage login` (no IDE required).
    Cloud,
}

/// Map a plasmoid source string to an Antigravity fetch method.
pub fn method_for_source(source: &str) -> AntigravityMethod {
    match source {
        "native-auth" => AntigravityMethod::Cloud,
        _ => AntigravityMethod::Auto,
    }
}

pub fn fetch(http: &HttpClient, timeout: Duration, method: AntigravityMethod) -> ProviderPayload {
    match method {
        AntigravityMethod::Local => fetch_local_payload(timeout),
        AntigravityMethod::Cloud => match fetch_cloud(http, timeout) {
            Ok(payload) => payload,
            Err(error) => ProviderPayload::error("antigravity", error.to_string()),
        },
        AntigravityMethod::Auto => match fetch_local(timeout) {
            LocalOutcome::Fresh(payload) => payload,
            LocalOutcome::Cached(cached) => fetch_cloud(http, timeout).unwrap_or(cached),
            LocalOutcome::NotRunning(_) | LocalOutcome::Error(_) => match fetch_cloud(http, timeout)
            {
                Ok(payload) => payload,
                Err(error) => ProviderPayload::error("antigravity", error.to_string()),
            },
        },
    }
}

/// Outcome of a local IDE probe, distinguishing fresh data from a cached
/// fallback so the Auto method can decide whether to try Cloud Code.
enum LocalOutcome {
    Fresh(ProviderPayload),
    Cached(ProviderPayload),
    NotRunning(String),
    Error(String),
}

fn fetch_local(timeout: Duration) -> LocalOutcome {
    match fetch_inner(timeout) {
        Ok(payload) => {
            save_cached_payload(&payload);
            LocalOutcome::Fresh(payload)
        }
        Err(error) if is_not_running_error(&error) => match cached_payload() {
            Some(cached) => LocalOutcome::Cached(cached),
            None => LocalOutcome::NotRunning(error.to_string()),
        },
        Err(error) => LocalOutcome::Error(error.to_string()),
    }
}

fn fetch_local_payload(timeout: Duration) -> ProviderPayload {
    match fetch_local(timeout) {
        LocalOutcome::Fresh(payload) => payload,
        LocalOutcome::Cached(payload) => payload,
        LocalOutcome::NotRunning(message) | LocalOutcome::Error(message) => {
            ProviderPayload::error("antigravity", message)
        }
    }
}

fn fetch_inner(timeout: Duration) -> Result<ProviderPayload> {
    let process = detect_process_info()?;
    let ports = listening_ports(process.pid)?;
    let http = HttpClient::new_insecure_localhost(timeout)?;
    let endpoint = resolve_working_endpoint(&http, &ports, &process.csrf_token)?;
    let request_endpoints = request_endpoints(&endpoint, &ports, &process.csrf_token);

    let mut snapshot = match fetch_user_status(&http, &request_endpoints) {
        Ok(snapshot) => snapshot,
        Err(primary_error) => {
            fetch_command_model_configs(&http, &request_endpoints).map_err(|fallback_error| {
                anyhow!("{primary_error}; fallback GetCommandModelConfigs failed: {fallback_error}")
            })?
        }
    };

    if let Ok(quota_rows) = fetch_quota_summary_rows(&http, &request_endpoints) {
        snapshot.usage_rows = Some(quota_rows);
    }

    let account_email = snapshot
        .identity
        .as_ref()
        .and_then(|identity| identity.account_email.clone());

    Ok(ProviderPayload::ok(
        "antigravity",
        snapshot,
        account_email,
        None,
        None,
    ))
}

struct ProcessInfo {
    pid: u32,
    csrf_token: String,
}

struct ConnectionEndpoint {
    scheme: &'static str,
    port: u16,
    csrf_token: String,
}

fn detect_process_info() -> Result<ProcessInfo> {
    let output = Command::new("ps")
        .args(["-ax", "-o", "pid=,command="])
        .output()
        .context("run ps for antigravity process detection")?;
    if !output.status.success() {
        anyhow::bail!("ps failed while searching for agy/antigravity-cli");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    process_info_from_ps_output(&text)
}

fn process_info_from_ps_output(output: &str) -> Result<ProcessInfo> {
    let mut saw_tokenless_ide = false;
    for line in output.lines() {
        let Some((pid, command)) = parse_process_line(line) else {
            continue;
        };
        let Some(kind) = antigravity_process_kind(&command) else {
            continue;
        };
        let Some(token) = resolved_csrf_token(kind, &command) else {
            saw_tokenless_ide = true;
            continue;
        };
        return Ok(ProcessInfo {
            pid,
            csrf_token: token,
        });
    }
    if saw_tokenless_ide {
        anyhow::bail!("Antigravity IDE language server found without CSRF token");
    }
    anyhow::bail!(NOT_RUNNING_MESSAGE)
}

fn is_not_running_error(error: &anyhow::Error) -> bool {
    error.to_string().contains(NOT_RUNNING_MESSAGE)
}

fn cached_payload() -> Option<ProviderPayload> {
    let raw = fs::read_to_string(cache_path()).ok()?;
    let mut payload: ProviderPayload = serde_json::from_str(&raw).ok()?;
    if payload.provider != "antigravity" || payload.usage.is_none() {
        return None;
    }
    payload.status = Some(ProviderStatusPayload {
        indicator: "minor".to_string(),
        description: "Antigravity is not running; showing last fetched usage.".to_string(),
        updated_at: Some(Utc::now().to_rfc3339()),
        url: String::new(),
    });
    payload.error = None;
    Some(payload)
}

fn save_cached_payload(payload: &ProviderPayload) {
    if payload.provider != "antigravity" || payload.usage.is_none() || payload.error.is_some() {
        return;
    }
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string(payload) {
        let _ = fs::write(path, raw);
    }
}

fn cache_path() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/"))
                .join(".cache")
        })
        .join("codexbar-plasmoid")
        .join("antigravity-last.json")
}

fn parse_process_line(line: &str) -> Option<(u32, String)> {
    let trimmed = line.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let pid = parts.next()?.parse().ok()?;
    let command = parts.next()?.trim().to_string();
    if command.is_empty() {
        return None;
    }
    Some((pid, command))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProcessKind {
    Ide,
    Cli,
}

fn antigravity_process_kind(command: &str) -> Option<ProcessKind> {
    let lower = command.to_ascii_lowercase();
    if is_language_server_command_line(&lower) && is_antigravity_command_line(&lower) {
        return Some(ProcessKind::Ide);
    }
    if is_antigravity_cli_command_line(&lower) {
        return Some(ProcessKind::Cli);
    }
    None
}

fn is_language_server_command_line(lower_command: &str) -> bool {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"(^|[/\\])language_server(_macos|\.exe)?(\s|$)").unwrap());
    re.is_match(lower_command)
}

fn is_antigravity_cli_command_line(lower_command: &str) -> bool {
    static CLI_PATH_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static AGY_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let cli_path_re = CLI_PATH_RE.get_or_init(|| {
        Regex::new(r"(^|[/\\])(antigravity-cli|antigravity_cli)([\s/\\]|$)").unwrap()
    });
    let agy_re = AGY_RE.get_or_init(|| Regex::new(r"(^|[/\\])agy(\s|$)").unwrap());
    cli_path_re.is_match(lower_command) || agy_re.is_match(lower_command)
}

fn is_antigravity_command_line(lower_command: &str) -> bool {
    lower_command.contains("--app_data_dir") && lower_command.contains("antigravity")
        || lower_command.contains("/antigravity/")
        || lower_command.contains("\\antigravity\\")
}

fn resolved_csrf_token(kind: ProcessKind, command: &str) -> Option<String> {
    if let Some(token) = extract_flag("--csrf_token", command) {
        return Some(token);
    }
    match kind {
        ProcessKind::Ide => None,
        ProcessKind::Cli => Some(String::new()),
    }
}

fn extract_flag(flag: &str, command: &str) -> Option<String> {
    let pattern = format!(r"{flag}[=\s]+([^\s]+)");
    let re = Regex::new(&pattern).ok()?;
    re.captures(command)
        .and_then(|caps| caps.get(1))
        .map(|value| value.as_str().to_string())
}

fn listening_ports(pid: u32) -> Result<Vec<u16>> {
    // Locate lsof via PATH. The plasmoid runs under a stripped environment
    // on some distros (notably NixOS), so the only portable lookup is a
    // PATH walk instead of hard-coded absolute paths.
    let lsof = which("lsof")
        .ok_or_else(|| anyhow!("lsof not available for Antigravity port detection"))?;

    let output = Command::new(&lsof)
        .args(["-nP", "-iTCP", "-sTCP:LISTEN", "-a", "-p", &pid.to_string()])
        .output()
        .with_context(|| format!("run {} for pid {pid}", lsof.display()))?;
    if !output.status.success() {
        anyhow::bail!("lsof failed while listing Antigravity listening ports");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let ports = parse_listening_ports(&text);
    if ports.is_empty() {
        anyhow::bail!("No listening ports found for Antigravity process");
    }
    Ok(ports)
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let candidate = dir.join(name);
        let Ok(meta) = std::fs::metadata(&candidate) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta.permissions().mode() & 0o111 == 0 {
            continue;
        }
        return Some(candidate);
    }
    None
}

fn parse_listening_ports(output: &str) -> Vec<u16> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r":(\d+)\s+\(LISTEN\)").unwrap());
    let mut ports = Vec::new();
    for caps in re.captures_iter(output) {
        if let Some(port) = caps.get(1).and_then(|value| value.as_str().parse().ok()) {
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
    }
    ports
}

fn language_server_endpoints(ports: &[u16], csrf_token: &str) -> Vec<ConnectionEndpoint> {
    ports
        .iter()
        .map(|port| ConnectionEndpoint {
            scheme: "https",
            port: *port,
            csrf_token: csrf_token.to_string(),
        })
        .collect()
}

fn request_endpoints(
    resolved: &ConnectionEndpoint,
    ports: &[u16],
    csrf_token: &str,
) -> Vec<ConnectionEndpoint> {
    let mut endpoints = vec![resolved.clone()];
    for candidate in language_server_endpoints(ports, csrf_token) {
        if !endpoints
            .iter()
            .any(|endpoint| endpoint.matches(&candidate))
        {
            endpoints.push(candidate);
        }
    }
    endpoints
}

impl ConnectionEndpoint {
    fn matches(&self, other: &Self) -> bool {
        self.scheme == other.scheme
            && self.port == other.port
            && self.csrf_token == other.csrf_token
    }

    fn url(&self, path: &str) -> String {
        format!("{}://127.0.0.1:{}{}", self.scheme, self.port, path)
    }
}

impl Clone for ConnectionEndpoint {
    fn clone(&self) -> Self {
        Self {
            scheme: self.scheme,
            port: self.port,
            csrf_token: self.csrf_token.clone(),
        }
    }
}

fn resolve_working_endpoint(
    http: &HttpClient,
    ports: &[u16],
    csrf_token: &str,
) -> Result<ConnectionEndpoint> {
    let candidates = language_server_endpoints(ports, csrf_token);
    for endpoint in &candidates {
        if test_endpoint_connectivity(http, endpoint).unwrap_or(false) {
            return Ok(endpoint.clone());
        }
    }
    candidates
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("No working Antigravity API port found"))
}

fn test_endpoint_connectivity(http: &HttpClient, endpoint: &ConnectionEndpoint) -> Result<bool> {
    let body = serde_json::json!({
        "context": {
            "properties": {
                "devMode": "false",
                "extensionVersion": "unknown",
                "hasAnthropicModelAccess": "true",
                "ide": "antigravity",
                "ideVersion": "unknown",
                "installationId": "codexbar-plasmoid",
                "language": "UNSPECIFIED",
                "os": "linux",
                "requestedModelId": "MODEL_UNSPECIFIED",
            }
        }
    });
    match http.post_connect_json(
        &endpoint.url(GET_UNLEASH_DATA_PATH),
        &endpoint.csrf_token,
        &body,
    ) {
        Ok(_) => Ok(true),
        Err(error) => {
            let message = error.to_string();
            if message.contains("HTTP ") {
                Ok(true)
            } else {
                Ok(false)
            }
        }
    }
}

fn default_request_body() -> serde_json::Value {
    serde_json::json!({
        "metadata": {
            "ideName": "antigravity",
            "extensionName": "antigravity",
            "ideVersion": "unknown",
            "locale": "en",
        }
    })
}

fn fetch_user_status(http: &HttpClient, endpoints: &[ConnectionEndpoint]) -> Result<UsageSnapshot> {
    let body = default_request_body();
    let mut last_error = None;
    for endpoint in endpoints {
        match http.post_connect_json(
            &endpoint.url(GET_USER_STATUS_PATH),
            &endpoint.csrf_token,
            &body,
        ) {
            Ok(text) => match parse_user_status_response(&text) {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("GetUserStatus failed")))
}

fn fetch_command_model_configs(
    http: &HttpClient,
    endpoints: &[ConnectionEndpoint],
) -> Result<UsageSnapshot> {
    let body = default_request_body();
    let mut last_error = None;
    for endpoint in endpoints {
        match http.post_connect_json(
            &endpoint.url(GET_COMMAND_MODEL_CONFIGS_PATH),
            &endpoint.csrf_token,
            &body,
        ) {
            Ok(text) => match parse_command_model_response(&text) {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("GetCommandModelConfigs failed")))
}

fn fetch_quota_summary_rows(
    http: &HttpClient,
    endpoints: &[ConnectionEndpoint],
) -> Result<Vec<UsageRowSnapshot>> {
    let body = default_request_body();
    let mut last_error = None;
    for endpoint in endpoints {
        match http.post_connect_json(
            &endpoint.url(RETRIEVE_USER_QUOTA_SUMMARY_PATH),
            &endpoint.csrf_token,
            &body,
        ) {
            Ok(text) => match parse_quota_summary_response(&text) {
                Ok(rows) => return Ok(rows),
                Err(error) => last_error = Some(error),
            },
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("RetrieveUserQuotaSummary failed")))
}

fn parse_quota_summary_response(text: &str) -> Result<Vec<UsageRowSnapshot>> {
    // Local IDE wraps groups under `response`; Cloud Code returns `groups` at the root.
    let groups = if let Ok(response) = serde_json::from_str::<QuotaSummaryResponse>(text) {
        response
            .response
            .and_then(|payload| payload.groups)
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let groups = if groups.is_empty() {
        serde_json::from_str::<CloudQuotaSummaryResponse>(text)
            .context("parse RetrieveUserQuotaSummary JSON")?
            .groups
            .unwrap_or_default()
    } else {
        groups
    };
    rows_from_quota_groups(groups)
}

/// Shared mapper for the four Antigravity quota buckets (Gemini/Claude × 5h/weekly).
fn rows_from_quota_groups(groups: Vec<QuotaGroup>) -> Result<Vec<UsageRowSnapshot>> {
    let mut bucket_map = std::collections::HashMap::new();
    for group in groups {
        for bucket in group.buckets.unwrap_or_default() {
            if let Some(id) = bucket.bucket_id.clone() {
                bucket_map.insert(id, bucket);
            }
        }
    }

    let rows = QUOTA_SUMMARY_ROW_ORDER
        .iter()
        .filter_map(|(bucket_id, row_id, title)| {
            bucket_map.get(*bucket_id).map(|bucket| UsageRowSnapshot {
                id: (*row_id).to_string(),
                title: (*title).to_string(),
                percent_left: bucket
                    .remaining_fraction
                    .map(|fraction| clamp_percent(fraction * 100.0))
                    .unwrap_or(0.0),
                resets_at: bucket
                    .reset_time
                    .as_deref()
                    .and_then(parse_iso_date),
            })
        })
        .collect::<Vec<_>>();

    if rows.is_empty() {
        anyhow::bail!("RetrieveUserQuotaSummary returned no quota buckets");
    }
    Ok(rows)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryResponse {
    response: Option<QuotaSummaryPayload>,
}

/// Cloud Code `v1internal:retrieveUserQuotaSummary` shape (no `response` wrapper).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudQuotaSummaryResponse {
    groups: Option<Vec<QuotaGroup>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryPayload {
    groups: Option<Vec<QuotaGroup>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaGroup {
    buckets: Option<Vec<QuotaBucket>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaBucket {
    bucket_id: Option<String>,
    remaining_fraction: Option<f64>,
    reset_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatusResponse {
    code: Option<CodeValue>,
    user_status: Option<UserStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandModelConfigResponse {
    code: Option<CodeValue>,
    client_model_configs: Option<Vec<ModelConfig>>,
}

#[derive(Debug, Deserialize)]
struct CodeValue {
    raw_value: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatus {
    email: Option<String>,
    user_tier: Option<UserTier>,
    plan_status: Option<PlanStatus>,
    cascade_model_config_data: Option<ModelConfigData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserTier {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanStatus {
    plan_info: Option<PlanInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanInfo {
    plan_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelConfigData {
    client_model_configs: Option<Vec<ModelConfig>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelConfig {
    label: Option<String>,
    model_or_alias: Option<ModelOrAlias>,
    quota_info: Option<QuotaInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelOrAlias {
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaInfo {
    remaining_fraction: Option<f64>,
    reset_time: Option<String>,
}

#[derive(Clone)]
struct ModelQuota {
    label: String,
    model_id: String,
    remaining_fraction: Option<f64>,
    reset_time: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelFamily {
    Claude,
    Gpt,
    GeminiPro,
    GeminiFlash,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModelGroup {
    Gemini,
    ClaudeGpt,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuotaWindow {
    Weekly,
    FiveHour,
    Unknown,
}

struct NormalizedModel {
    quota: ModelQuota,
    family: ModelFamily,
    selection_priority: Option<i32>,
}

fn parse_user_status_response(text: &str) -> Result<UsageSnapshot> {
    let response: UserStatusResponse =
        serde_json::from_str(text).context("parse GetUserStatus JSON")?;
    if let Some(code) = invalid_code(response.code.as_ref()) {
        anyhow::bail!("GetUserStatus API error: {code}");
    }
    let user_status = response
        .user_status
        .ok_or_else(|| anyhow!("GetUserStatus missing userStatus"))?;
    let configs = user_status
        .cascade_model_config_data
        .and_then(|data| data.client_model_configs)
        .unwrap_or_default();
    let quotas = configs
        .into_iter()
        .filter_map(quota_from_config)
        .collect::<Vec<_>>();
    let plan = user_status
        .user_tier
        .and_then(|tier| tier.name)
        .or_else(|| {
            user_status
                .plan_status
                .and_then(|status| status.plan_info)
                .and_then(|info| info.plan_name)
        });
    usage_snapshot_from_quotas(quotas, user_status.email, plan)
}

fn parse_command_model_response(text: &str) -> Result<UsageSnapshot> {
    let response: CommandModelConfigResponse =
        serde_json::from_str(text).context("parse GetCommandModelConfigs JSON")?;
    if let Some(code) = invalid_code(response.code.as_ref()) {
        anyhow::bail!("GetCommandModelConfigs API error: {code}");
    }
    let quotas = response
        .client_model_configs
        .unwrap_or_default()
        .into_iter()
        .filter_map(quota_from_config)
        .collect::<Vec<_>>();
    usage_snapshot_from_quotas(quotas, None, None)
}

fn invalid_code(code: Option<&CodeValue>) -> Option<String> {
    let raw = code?.raw_value?;
    if raw == 0 {
        None
    } else {
        Some(raw.to_string())
    }
}

fn quota_from_config(config: ModelConfig) -> Option<ModelQuota> {
    let label = config.label?;
    let model_id = config
        .model_or_alias
        .and_then(|alias| alias.model)
        .unwrap_or_else(|| label.clone());
    let quota_info = config.quota_info?;
    Some(ModelQuota {
        label,
        model_id,
        remaining_fraction: quota_info.remaining_fraction,
        reset_time: quota_info.reset_time.as_deref().and_then(parse_iso_date),
    })
}

fn usage_snapshot_from_quotas(
    quotas: Vec<ModelQuota>,
    account_email: Option<String>,
    plan: Option<String>,
) -> Result<UsageSnapshot> {
    if quotas.is_empty() {
        anyhow::bail!("No Antigravity quota models available");
    }

    let now = Utc::now();
    let normalized = quotas.into_iter().map(normalize_model).collect::<Vec<_>>();
    let usage_models: Vec<_> = normalized
        .iter()
        .filter(|model| model.quota.remaining_fraction.is_some() || model.quota.reset_time.is_some())
        .cloned()
        .collect();
    let summary_models: Vec<_> = usage_models
        .iter()
        .filter(|model| model.quota.remaining_fraction.is_some())
        .cloned()
        .collect();
    let usage_rows = grouped_usage_rows(&usage_models, now);

    let primary = representative(ModelFamily::Claude, &summary_models)
        .or_else(|| representative(ModelFamily::Gpt, &summary_models))
        .or_else(|| fallback_representative(&summary_models))
        .map(rate_window_for_quota);
    let secondary =
        representative(ModelFamily::GeminiPro, &summary_models).map(rate_window_for_quota);
    let tertiary =
        representative(ModelFamily::GeminiFlash, &summary_models).map(rate_window_for_quota);

    Ok(UsageSnapshot {
        primary,
        secondary,
        tertiary,
        usage_rows: if usage_rows.is_empty() {
            None
        } else {
            Some(usage_rows)
        },
        provider_cost: None,
        cursor_requests: None,
        updated_at: now,
        identity: Some(ProviderIdentitySnapshot {
            account_email,
            account_organization: None,
            login_method: plan,
        }),
    })
}

fn normalize_model(quota: ModelQuota) -> NormalizedModel {
    let model_id = quota.model_id.to_ascii_lowercase();
    let label = quota.label.to_ascii_lowercase();
    let family = family_for_model(&model_id, &label);
    let is_lite = model_id.contains("lite") || label.contains("lite");
    let is_autocomplete = model_id.contains("autocomplete")
        || label.contains("autocomplete")
        || model_id.starts_with("tab_");
    let is_image = model_id.contains("image") || label.contains("image");
    let is_selectable = !is_lite && !is_autocomplete && !is_image;
    let is_low_priority_gemini_pro =
        model_id.contains("pro-low") || (label.contains("pro") && label.contains("low"));

    let selection_priority = match family {
        ModelFamily::Claude => Some(0),
        ModelFamily::Gpt => Some(1),
        ModelFamily::GeminiPro if is_low_priority_gemini_pro && is_selectable => Some(0),
        ModelFamily::GeminiPro if is_selectable => Some(1),
        ModelFamily::GeminiFlash if is_selectable => Some(0),
        _ => None,
    };

    NormalizedModel {
        quota,
        family,
        selection_priority,
    }
}

impl Clone for NormalizedModel {
    fn clone(&self) -> Self {
        Self {
            quota: self.quota.clone(),
            family: self.family,
            selection_priority: self.selection_priority,
        }
    }
}

fn family_for_model(model_id: &str, label: &str) -> ModelFamily {
    let from_id = family_from_text(model_id);
    if from_id != ModelFamily::Unknown {
        return from_id;
    }
    family_from_text(label)
}

fn family_from_text(text: &str) -> ModelFamily {
    if text.contains("claude") || text.contains("opus") || text.contains("sonnet") {
        ModelFamily::Claude
    } else if text.contains("gpt") {
        ModelFamily::Gpt
    } else if text.contains("gemini") && text.contains("pro") {
        ModelFamily::GeminiPro
    } else if text.contains("gemini") && text.contains("flash") {
        ModelFamily::GeminiFlash
    } else if text.contains("gemini") {
        ModelFamily::GeminiFlash
    } else {
        ModelFamily::Unknown
    }
}

fn remaining_percent(quota: &ModelQuota) -> f64 {
    quota
        .remaining_fraction
        .map(|fraction| clamp_percent(fraction * 100.0))
        .unwrap_or(0.0)
}

fn grouped_usage_rows(models: &[NormalizedModel], now: DateTime<Utc>) -> Vec<UsageRowSnapshot> {
    [
        (
            ModelGroup::Gemini,
            QuotaWindow::FiveHour,
            "gemini-five-hour",
            "Gemini five-hour limit",
        ),
        (
            ModelGroup::Gemini,
            QuotaWindow::Weekly,
            "gemini-weekly",
            "Gemini weekly limit",
        ),
        (
            ModelGroup::ClaudeGpt,
            QuotaWindow::FiveHour,
            "claude-gpt-five-hour",
            "Claude/GPT five-hour limit",
        ),
        (
            ModelGroup::ClaudeGpt,
            QuotaWindow::Weekly,
            "claude-gpt-weekly",
            "Claude/GPT weekly limit",
        ),
    ]
    .into_iter()
    .filter_map(|(group, window, id, title)| {
        grouped_usage_row(models, group, window, id, title, now)
    })
    .collect()
}

fn grouped_usage_row(
    models: &[NormalizedModel],
    group: ModelGroup,
    window: QuotaWindow,
    id: &str,
    title: &str,
    now: DateTime<Utc>,
) -> Option<UsageRowSnapshot> {
    let model = models
        .iter()
        .filter(|model| quota_group(model) == group)
        .filter(|model| quota_window(&model.quota, now) == window)
        .min_by(|left, right| compare_group_candidates(left, right))?;

    Some(UsageRowSnapshot {
        id: id.to_string(),
        title: title.to_string(),
        percent_left: remaining_percent(&model.quota),
        resets_at: model.quota.reset_time,
    })
}

fn quota_group(model: &NormalizedModel) -> ModelGroup {
    match model.family {
        ModelFamily::GeminiPro | ModelFamily::GeminiFlash => ModelGroup::Gemini,
        ModelFamily::Claude | ModelFamily::Gpt => ModelGroup::ClaudeGpt,
        ModelFamily::Unknown => ModelGroup::Unknown,
    }
}

fn quota_window(quota: &ModelQuota, now: DateTime<Utc>) -> QuotaWindow {
    let text = format!("{} {}", quota.model_id, quota.label).to_ascii_lowercase();
    if contains_weekly_marker(&text) {
        return QuotaWindow::Weekly;
    }
    if contains_five_hour_marker(&text) {
        return QuotaWindow::FiveHour;
    }

    let Some(reset_time) = quota.reset_time else {
        return QuotaWindow::Unknown;
    };
    let reset_minutes = reset_time.signed_duration_since(now).num_minutes();
    if reset_minutes <= 0 {
        QuotaWindow::Unknown
    } else if reset_minutes <= 12 * 60 {
        QuotaWindow::FiveHour
    } else {
        QuotaWindow::Weekly
    }
}

fn contains_weekly_marker(text: &str) -> bool {
    text.contains("weekly") || text.contains("week")
}

fn contains_five_hour_marker(text: &str) -> bool {
    text.contains("five hour")
        || text.contains("five-hour")
        || text.contains("five_hour")
        || text.contains("5 hour")
        || text.contains("5-hour")
        || text.contains("5_hour")
        || text.contains("5h")
}

fn compare_group_candidates(left: &NormalizedModel, right: &NormalizedModel) -> std::cmp::Ordering {
    remaining_percent(&left.quota)
        .partial_cmp(&remaining_percent(&right.quota))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| compare_reset_times(left.quota.reset_time, right.quota.reset_time))
        .then_with(|| left.quota.label.cmp(&right.quota.label))
}

fn compare_reset_times(
    left: Option<DateTime<Utc>>,
    right: Option<DateTime<Utc>>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn representative(family: ModelFamily, models: &[NormalizedModel]) -> Option<ModelQuota> {
    let mut candidates: Vec<_> = models
        .iter()
        .filter(|model| model.family == family && model.selection_priority.is_some())
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|left, right| {
        let left_has_fraction = left.quota.remaining_fraction.is_some();
        let right_has_fraction = right.quota.remaining_fraction.is_some();
        left_has_fraction
            .cmp(&right_has_fraction)
            .reverse()
            .then_with(|| {
                left.selection_priority
                    .unwrap_or(i32::MAX)
                    .cmp(&right.selection_priority.unwrap_or(i32::MAX))
            })
            .then_with(|| {
                remaining_percent(&left.quota)
                    .partial_cmp(&remaining_percent(&right.quota))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.quota.label.cmp(&right.quota.label))
    });
    candidates.first().map(|model| model.quota.clone())
}

fn fallback_representative(models: &[NormalizedModel]) -> Option<ModelQuota> {
    let mut candidates: Vec<_> = models.iter().collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|left, right| {
        let left_has_fraction = left.quota.remaining_fraction.is_some();
        let right_has_fraction = right.quota.remaining_fraction.is_some();
        left_has_fraction
            .cmp(&right_has_fraction)
            .reverse()
            .then_with(|| {
                remaining_percent(&left.quota)
                    .partial_cmp(&remaining_percent(&right.quota))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.quota.label.cmp(&right.quota.label))
    });
    candidates.first().map(|model| model.quota.clone())
}

fn rate_window_for_quota(quota: ModelQuota) -> crate::output::RateWindow {
    let remaining = remaining_percent(&quota);
    rate_window(100.0 - remaining, None, quota.reset_time)
}

fn parse_iso_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| value.parse::<DateTime<Utc>>().ok())
}

// ===========================================================================
// Cloud Code (Native Auth) — Google OAuth tokens stored by `antigravity-usage`
// ===========================================================================

/// OAuth *app* client id/secret are intentionally not shipped in this repo.
/// Supply them locally (env or `~/.codexbar/config.json`) so token refresh can
/// call Google's token endpoint. User access/refresh tokens still come from
/// `antigravity-usage login` under `~/.config/antigravity-usage`.
fn oauth_client_id() -> Result<String> {
    crate::config::antigravity_oauth_client_id().ok_or_else(|| {
        anyhow!(
            "Missing Antigravity OAuth client id. Set ANTIGRAVITY_OAUTH_CLIENT_ID \
             (or oauth_client_id on the antigravity entry in ~/.codexbar/config.json). \
             These are the Google Cloud OAuth app credentials used by antigravity-usage \
             for token refresh — not your personal tokens."
        )
    })
}

fn oauth_client_secret() -> Result<String> {
    crate::config::antigravity_oauth_client_secret().ok_or_else(|| {
        anyhow!(
            "Missing Antigravity OAuth client secret. Set ANTIGRAVITY_OAUTH_CLIENT_SECRET \
             (or oauth_client_secret on the antigravity entry in ~/.codexbar/config.json). \
             These are the Google Cloud OAuth app credentials used by antigravity-usage \
             for token refresh — not your personal tokens."
        )
    })
}

const OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CLOUDCODE_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
const CLOUDCODE_USER_AGENT: &str = "antigravity";
const LOAD_CODE_ASSIST_PATH: &str = "/v1internal:loadCodeAssist";
const ONBOARD_USER_PATH: &str = "/v1internal:onboardUser";
const FETCH_AVAILABLE_MODELS_PATH: &str = "/v1internal:fetchAvailableModels";
/// Official dual-window buckets (Gemini/Claude 5h + weekly). Prefer this over
/// inferring windows from per-model `fetchAvailableModels` reset times.
const RETRIEVE_USER_QUOTA_SUMMARY_CLOUD_PATH: &str = "/v1internal:retrieveUserQuotaSummary";
const TOKEN_EXPIRY_BUFFER_MS: i64 = 5 * 60 * 1000;

/// Tokens stored by `antigravity-usage login`. Field names match the on-disk
/// camelCase JSON so we can read and refresh them in place.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredTokens {
    access_token: String,
    refresh_token: String,
    /// Unix timestamp in milliseconds.
    expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActiveAccountConfig {
    #[serde(default)]
    active_account: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadCodeAssistResponse {
    #[serde(default)]
    cloudaicompanion_project: Option<serde_json::Value>,
    #[serde(default)]
    plan_info: Option<LoadCodeAssistPlanInfo>,
    #[serde(default)]
    current_tier: Option<TierRef>,
    #[serde(default)]
    paid_tier: Option<TierRef>,
    #[serde(default)]
    allowed_tiers: Option<Vec<AllowedTier>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadCodeAssistPlanInfo {
    #[serde(default)]
    plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TierRef {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AllowedTier {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    is_default: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAvailableModelsResponse {
    #[serde(default)]
    models: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudModelInfo {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    quota_info: Option<CloudQuotaInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudQuotaInfo {
    #[serde(default)]
    remaining_fraction: Option<f64>,
    #[serde(default)]
    reset_time: Option<String>,
}

/// Google's OAuth token endpoint returns snake_case fields (`access_token`,
/// `expires_in`, optional `refresh_token`). Do not rename to camelCase.
#[derive(Debug, Deserialize)]
struct TokenRefreshResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
}

fn antigravity_usage_config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("/"))
                .join(".config")
        })
        .join("antigravity-usage")
}

fn sanitize_account_name(email: &str) -> String {
    email
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn load_account_tokens(email: &str) -> Option<StoredTokens> {
    let path = antigravity_usage_config_dir()
        .join("accounts")
        .join(sanitize_account_name(email))
        .join("tokens.json");
    let raw = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Load the active account's tokens, falling back to the legacy single-account
/// `tokens.json`. Returns the tokens alongside the path they came from so a
/// refreshed token set can be persisted back to the same file.
fn load_tokens() -> Result<(StoredTokens, PathBuf)> {
    let config_dir = antigravity_usage_config_dir();
    let config_path = config_dir.join("config.json");
    if let Ok(raw) = fs::read_to_string(&config_path) {
        if let Ok(config) = serde_json::from_str::<ActiveAccountConfig>(&raw) {
            if let Some(email) = config
                .active_account
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                if let Some(tokens) = load_account_tokens(email) {
                    let path = config_dir
                        .join("accounts")
                        .join(sanitize_account_name(email))
                        .join("tokens.json");
                    return Ok((tokens, path));
                }
            }
        }
    }

    let legacy_path = config_dir.join("tokens.json");
    let raw = fs::read_to_string(&legacy_path)
        .context("No antigravity-usage login found. Run `antigravity-usage login` first.")?;
    let tokens: StoredTokens = serde_json::from_str(&raw)
        .context("Parse antigravity-usage tokens.json failed")?;
    Ok((tokens, legacy_path))
}

fn persist_tokens(path: &Path, tokens: &StoredTokens) -> Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let raw = serde_json::to_string_pretty(tokens).context("serialize refreshed tokens")?;
    fs::write(path, raw).context("write refreshed tokens")?;
    Ok(())
}

fn form_urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push_str(&format!("{byte:02X}"));
            }
        }
    }
    out
}

/// Return a valid access token, refreshing via the OAuth token endpoint when
/// the cached token is within the expiry buffer. The refreshed token set is
/// persisted back to the same file `antigravity-usage` uses.
fn ensure_valid_access_token(
    http: &HttpClient,
    tokens: &mut StoredTokens,
    path: &Path,
) -> Result<String> {
    let now_ms = Utc::now().timestamp_millis();
    if now_ms < tokens.expires_at - TOKEN_EXPIRY_BUFFER_MS {
        return Ok(tokens.access_token.clone());
    }
    if tokens.refresh_token.is_empty() {
        anyhow::bail!("No refresh token available. Run `antigravity-usage login` again.");
    }

    let body = format!(
        "refresh_token={}&client_id={}&client_secret={}&grant_type=refresh_token",
        form_urlencode(&tokens.refresh_token),
        form_urlencode(&oauth_client_id()?),
        form_urlencode(&oauth_client_secret()?),
    );
    let text = http
        .post_form(OAUTH_TOKEN_URL, &body)
        .context("refresh Google OAuth access token")?;
    let response: TokenRefreshResponse = serde_json::from_str(&text).with_context(|| {
        format!(
            "parse OAuth token refresh response (HTTP body {} bytes)",
            text.len()
        )
    })?;

    tokens.access_token = response.access_token;
    if let Some(refresh) = response.refresh_token {
        if !refresh.is_empty() {
            tokens.refresh_token = refresh;
        }
    }
    tokens.expires_at = now_ms + response.expires_in.saturating_mul(1000);
    let _ = persist_tokens(path, tokens);
    Ok(tokens.access_token.clone())
}

fn cloudcode_metadata() -> serde_json::Value {
    serde_json::json!({
        "ideType": "ANTIGRAVITY",
        "platform": "PLATFORM_UNSPECIFIED",
        "pluginType": "GEMINI"
    })
}

fn extract_cloud_project_id(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(string) if !string.is_empty() => Some(string.clone()),
        serde_json::Value::Object(object) => object
            .get("id")
            .and_then(|item| item.as_str())
            .filter(|string| !string.is_empty())
            .map(|string| string.to_string()),
        _ => None,
    }
}

fn pick_onboard_tier(response: &LoadCodeAssistResponse) -> Option<String> {
    if let Some(tiers) = &response.allowed_tiers {
        if let Some(default) = tiers
            .iter()
            .find(|tier| tier.is_default == Some(true) && tier.id.is_some())
        {
            return default.id.clone();
        }
        if let Some(first) = tiers.iter().find_map(|tier| tier.id.clone()) {
            return Some(first);
        }
        if !tiers.is_empty() {
            return Some("LEGACY".to_string());
        }
    }
    response
        .paid_tier
        .as_ref()
        .and_then(|tier| tier.id.clone())
        .or_else(|| response.current_tier.as_ref().and_then(|tier| tier.id.clone()))
}

fn load_code_assist(http: &HttpClient, access_token: &str) -> Result<LoadCodeAssistResponse> {
    let body = serde_json::json!({ "metadata": cloudcode_metadata() });
    let url = format!("{CLOUDCODE_BASE_URL}{LOAD_CODE_ASSIST_PATH}");
    let (status, text) = http.post_bearer_text(&url, access_token, CLOUDCODE_USER_AGENT, &body)?;

    if status == 401 || status == 403 {
        anyhow::bail!(
            "Cloud Code authentication failed (HTTP {status}). Run `antigravity-usage login` again."
        );
    }
    if status >= 400 {
        let snippet = text.chars().take(200).collect::<String>();
        anyhow::bail!("Cloud Code loadCodeAssist failed: HTTP {status}: {snippet}");
    }
    let response: LoadCodeAssistResponse =
        serde_json::from_str(&text).context("parse loadCodeAssist response")?;
    Ok(response)
}

fn onboard_user(http: &HttpClient, access_token: &str, tier_id: &str) -> Result<()> {
    let body = serde_json::json!({ "tierId": tier_id, "metadata": cloudcode_metadata() });
    let url = format!("{CLOUDCODE_BASE_URL}{ONBOARD_USER_PATH}");
    let _ = http.post_bearer_text(&url, access_token, CLOUDCODE_USER_AGENT, &body)?;
    Ok(())
}

fn should_show_cloud_model(model_id: &str) -> bool {
    let lower = model_id.to_ascii_lowercase();
    if lower.starts_with("chat_") || lower.starts_with("tab_") {
        return false;
    }
    if lower.contains("image") {
        return false;
    }
    if lower.starts_with("rev") {
        return false;
    }
    if lower.contains("mquery") || lower.contains("lite") {
        return false;
    }
    true
}

fn fetch_available_models(
    http: &HttpClient,
    access_token: &str,
    project_id: Option<&str>,
) -> Result<Vec<ModelQuota>> {
    let body = match project_id {
        Some(id) => serde_json::json!({ "project": id }),
        None => serde_json::json!({}),
    };
    let url = format!("{CLOUDCODE_BASE_URL}{FETCH_AVAILABLE_MODELS_PATH}");
    let (status, text) = http.post_bearer_text(&url, access_token, CLOUDCODE_USER_AGENT, &body)?;

    if status == 401 || status == 403 {
        anyhow::bail!(
            "Cloud Code authentication failed (HTTP {status}). Run `antigravity-usage login` again."
        );
    }
    if status >= 400 {
        let snippet = text.chars().take(200).collect::<String>();
        anyhow::bail!("Cloud Code fetchAvailableModels failed: HTTP {status}: {snippet}");
    }

    let response: FetchAvailableModelsResponse =
        serde_json::from_str(&text).context("parse fetchAvailableModels response")?;
    Ok(quotas_from_models(response.models.unwrap_or_default()))
}

/// Pure parser that turns the `models` map from `fetchAvailableModels` into the
/// shared `ModelQuota` shape consumed by `usage_snapshot_from_quotas`.
fn quotas_from_models(models: serde_json::Map<String, serde_json::Value>) -> Vec<ModelQuota> {
    let mut quotas = Vec::new();
    for (model_id, value) in models {
        let Ok(info) = serde_json::from_value::<CloudModelInfo>(value) else {
            continue;
        };
        if !should_show_cloud_model(&model_id) {
            continue;
        }
        let Some(quota_info) = info.quota_info else {
            continue;
        };
        let label = info
            .display_name
            .or(info.label)
            .unwrap_or_else(|| model_id.clone());
        let model_identifier = info.model.unwrap_or_else(|| model_id.clone());
        quotas.push(ModelQuota {
            label,
            model_id: model_identifier,
            remaining_fraction: quota_info.remaining_fraction,
            reset_time: quota_info.reset_time.as_deref().and_then(parse_iso_date),
        });
    }
    quotas
}

/// Fetch usage via the Google Cloud Code API using OAuth tokens stored by
/// `antigravity-usage login`. No running IDE is required.
fn fetch_cloud(http: &HttpClient, _timeout: Duration) -> Result<ProviderPayload> {
    let (mut tokens, path) = load_tokens()?;
    let access_token = ensure_valid_access_token(http, &mut tokens, &path)?;

    let mut code_assist = load_code_assist(http, &access_token)?;
    let mut project_id = tokens
        .project_id
        .clone()
        .or_else(|| {
            code_assist
                .cloudaicompanion_project
                .as_ref()
                .and_then(|value| extract_cloud_project_id(value))
        });

    // No project yet — try a single onboarding pass, then reload.
    if project_id.is_none() {
        if let Some(tier_id) = pick_onboard_tier(&code_assist) {
            let _ = onboard_user(http, &access_token, &tier_id);
            if let Ok(reloaded) = load_code_assist(http, &access_token) {
                code_assist = reloaded;
                project_id = code_assist
                    .cloudaicompanion_project
                    .as_ref()
                    .and_then(|value| extract_cloud_project_id(value));
            }
        }
    }

    // Persist a newly resolved project id so the next refresh skips onboarding.
    if tokens.project_id.as_deref() != project_id.as_deref() {
        if let Some(id) = project_id.clone() {
            tokens.project_id = Some(id);
            let _ = persist_tokens(&path, &tokens);
        }
    }

    let plan = code_assist
        .plan_info
        .and_then(|info| info.plan_type);

    let quotas = fetch_available_models(http, &access_token, project_id.as_deref())
        .unwrap_or_default();
    if quotas.is_empty() {
        anyhow::bail!("No Antigravity quota models returned by Cloud Code API");
    }

    let mut snapshot = usage_snapshot_from_quotas(quotas, tokens.email.clone(), plan)?;
    // Prefer official dual-window buckets (includes weekly limits). Per-model
    // fetchAvailableModels only exposes the nearer reset and often hides weekly.
    if let Ok(quota_rows) =
        fetch_cloud_quota_summary_rows(http, &access_token, project_id.as_deref())
    {
        snapshot.usage_rows = Some(quota_rows);
    }

    let mut payload = ProviderPayload::ok("antigravity", snapshot, tokens.email.clone(), None, None);
    payload.source = "native-auth".to_string();
    save_cached_payload(&payload);
    Ok(payload)
}

/// Cloud Code dual-window quota summary (Gemini/Claude × 5h/weekly).
fn fetch_cloud_quota_summary_rows(
    http: &HttpClient,
    access_token: &str,
    project_id: Option<&str>,
) -> Result<Vec<UsageRowSnapshot>> {
    let body = match project_id {
        Some(id) => serde_json::json!({ "project": id }),
        None => serde_json::json!({}),
    };
    let url = format!("{CLOUDCODE_BASE_URL}{RETRIEVE_USER_QUOTA_SUMMARY_CLOUD_PATH}");
    let (status, text) = http.post_bearer_text(&url, access_token, CLOUDCODE_USER_AGENT, &body)?;

    if status == 401 || status == 403 {
        anyhow::bail!(
            "Cloud Code authentication failed (HTTP {status}). Run `antigravity-usage login` again."
        );
    }
    if status >= 400 {
        let snippet = text.chars().take(200).collect::<String>();
        anyhow::bail!("Cloud Code retrieveUserQuotaSummary failed: HTTP {status}: {snippet}");
    }

    parse_quota_summary_response(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_agy_process_without_csrf_token() {
        let info =
            process_info_from_ps_output("523690 /home/sfire/.local/bin/agy\n99999 /usr/bin/bash\n")
                .expect("agy process should be detected");
        assert_eq!(info.pid, 523690);
        assert_eq!(info.csrf_token, "");
    }

    #[test]
    fn parses_user_status_into_three_family_lanes() {
        let snapshot = parse_user_status_response(
            r#"{
              "userStatus": {
                "email": "user@example.com",
                "userTier": { "name": "Google AI Pro" },
                "cascadeModelConfigData": {
                  "clientModelConfigs": [
                    {
                      "label": "Claude Sonnet 4.6 (Thinking)",
                      "modelOrAlias": { "model": "MODEL_CLAUDE" },
                      "quotaInfo": { "remainingFraction": 0.4, "resetTime": "2026-06-10T13:14:41Z" }
                    },
                    {
                      "label": "Gemini 3.1 Pro (Low)",
                      "modelOrAlias": { "model": "MODEL_GEMINI_PRO_LOW" },
                      "quotaInfo": { "remainingFraction": 0.25, "resetTime": "2026-06-10T13:14:41Z" }
                    },
                    {
                      "label": "Gemini 3.5 Flash (Medium)",
                      "modelOrAlias": { "model": "MODEL_GEMINI_FLASH" },
                      "quotaInfo": { "remainingFraction": 0.8, "resetTime": "2026-06-10T13:14:41Z" }
                    }
                  ]
                }
              }
            }"#,
        )
        .expect("user status should parse");

        assert_eq!(
            snapshot.primary.as_ref().map(|w| w.remaining_percent),
            Some(40.0)
        );
        assert_eq!(
            snapshot.secondary.as_ref().map(|w| w.remaining_percent),
            Some(25.0)
        );
        assert_eq!(
            snapshot.tertiary.as_ref().map(|w| w.remaining_percent),
            Some(80.0)
        );
        assert_eq!(
            snapshot
                .identity
                .as_ref()
                .and_then(|identity| identity.account_email.clone()),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn parses_antigravity_grouped_weekly_and_five_hour_limits() {
        let weekly_reset = (Utc::now() + chrono::Duration::hours(168)).to_rfc3339();
        let five_hour_reset = (Utc::now() + chrono::Duration::hours(5)).to_rfc3339();
        let response = serde_json::json!({
            "userStatus": {
                "email": "user@example.com",
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "Gemini Flash weekly",
                            "modelOrAlias": { "model": "MODEL_GEMINI_FLASH" },
                            "quotaInfo": { "remainingFraction": 0.999, "resetTime": weekly_reset }
                        },
                        {
                            "label": "Gemini Pro five-hour",
                            "modelOrAlias": { "model": "MODEL_GEMINI_PRO" },
                            "quotaInfo": { "remainingFraction": 0.9938, "resetTime": five_hour_reset }
                        },
                        {
                            "label": "Claude Sonnet weekly",
                            "modelOrAlias": { "model": "MODEL_CLAUDE_SONNET" },
                            "quotaInfo": { "remainingFraction": 0.6587, "resetTime": weekly_reset }
                        },
                        {
                            "label": "GPT-OSS weekly",
                            "modelOrAlias": { "model": "MODEL_GPT_OSS" },
                            "quotaInfo": { "remainingFraction": 0.66, "resetTime": weekly_reset }
                        },
                        {
                            "label": "Claude Opus five-hour",
                            "modelOrAlias": { "model": "MODEL_CLAUDE_OPUS" },
                            "quotaInfo": { "remainingFraction": 0.0, "resetTime": five_hour_reset }
                        }
                    ]
                }
            }
        })
        .to_string();

        let snapshot = parse_user_status_response(&response).expect("user status should parse");
        let rows = snapshot
            .usage_rows
            .as_ref()
            .expect("Antigravity should expose grouped quota rows");

        assert_eq!(rows.len(), 4);
        assert_usage_row(&rows[0], "gemini-five-hour", 99.38);
        assert_usage_row(&rows[1], "gemini-weekly", 99.9);
        assert_usage_row(&rows[2], "claude-gpt-five-hour", 0.0);
        assert_usage_row(&rows[3], "claude-gpt-weekly", 65.87);
        assert!(rows.iter().all(|row| row.resets_at.is_some()));
    }

    #[test]
    fn parses_quota_summary_into_grouped_rows() {
        let weekly_reset = (Utc::now() + chrono::Duration::hours(168)).to_rfc3339();
        let five_hour_reset = (Utc::now() + chrono::Duration::hours(5)).to_rfc3339();
        let response = serde_json::json!({
            "response": {
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "buckets": [
                            {
                                "bucketId": "gemini-weekly",
                                "displayName": "Weekly Limit",
                                "window": "weekly",
                                "remainingFraction": 0.8951932,
                                "resetTime": weekly_reset
                            },
                            {
                                "bucketId": "gemini-5h",
                                "displayName": "Five Hour Limit",
                                "window": "5h",
                                "remainingFraction": 0.949465,
                                "resetTime": five_hour_reset
                            }
                        ]
                    },
                    {
                        "displayName": "Claude and GPT models",
                        "buckets": [
                            {
                                "bucketId": "3p-weekly",
                                "displayName": "Weekly Limit",
                                "window": "weekly",
                                "remainingFraction": 0.45907852,
                                "resetTime": weekly_reset
                            },
                            {
                                "bucketId": "3p-5h",
                                "displayName": "Five Hour Limit",
                                "window": "5h",
                                "remainingFraction": 0.4010748,
                                "resetTime": five_hour_reset
                            }
                        ]
                    }
                ]
            }
        })
        .to_string();

        let rows = parse_quota_summary_response(&response).expect("quota summary should parse");

        assert_eq!(rows.len(), 4);
        assert_usage_row(&rows[0], "gemini-five-hour", 94.95);
        assert_usage_row(&rows[1], "gemini-weekly", 89.52);
        assert_usage_row(&rows[2], "claude-gpt-five-hour", 40.11);
        assert_usage_row(&rows[3], "claude-gpt-weekly", 45.91);
        assert!(rows.iter().all(|row| row.resets_at.is_some()));
    }

    #[test]
    fn parses_cloud_code_quota_summary_root_groups() {
        // Cloud Code returns groups at the root (no `response` wrapper) and is
        // the source of weekly limits for native-auth.
        let weekly_reset = (Utc::now() + chrono::Duration::hours(120)).to_rfc3339();
        let five_hour_reset = (Utc::now() + chrono::Duration::hours(5)).to_rfc3339();
        let response = serde_json::json!({
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "displayName": "Weekly Limit",
                            "window": "weekly",
                            "remainingFraction": 0.98587036,
                            "resetTime": weekly_reset
                        },
                        {
                            "bucketId": "gemini-5h",
                            "displayName": "Five Hour Limit",
                            "window": "5h",
                            "remainingFraction": 1.0,
                            "resetTime": five_hour_reset
                        }
                    ]
                },
                {
                    "displayName": "Claude and GPT models",
                    "buckets": [
                        {
                            "bucketId": "3p-weekly",
                            "displayName": "Weekly Limit",
                            "window": "weekly",
                            "remainingFraction": 1.0,
                            "resetTime": weekly_reset
                        },
                        {
                            "bucketId": "3p-5h",
                            "displayName": "Five Hour Limit",
                            "window": "5h",
                            "remainingFraction": 1.0,
                            "resetTime": five_hour_reset
                        }
                    ]
                }
            ],
            "description": "Within each group, models share a weekly limit and a 5-hour limit."
        })
        .to_string();

        let rows = parse_quota_summary_response(&response)
            .expect("cloud quota summary should parse");

        assert_eq!(rows.len(), 4);
        assert_usage_row(&rows[0], "gemini-five-hour", 100.0);
        assert_usage_row(&rows[1], "gemini-weekly", 98.59);
        assert_usage_row(&rows[2], "claude-gpt-five-hour", 100.0);
        assert_usage_row(&rows[3], "claude-gpt-weekly", 100.0);
        assert!(rows.iter().all(|row| row.resets_at.is_some()));
    }

    #[test]
    fn keeps_zero_remaining_rows_when_antigravity_omits_fraction() {
        let five_hour_reset = (Utc::now() + chrono::Duration::hours(5)).to_rfc3339();
        let response = serde_json::json!({
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelConfigs": [
                        {
                            "label": "Gemini 3.5 Flash (Medium)",
                            "modelOrAlias": { "model": "MODEL_PLACEHOLDER_M20" },
                            "quotaInfo": { "remainingFraction": 0.98, "resetTime": five_hour_reset }
                        },
                        {
                            "label": "Claude Sonnet 4.6 (Thinking)",
                            "modelOrAlias": { "model": "MODEL_PLACEHOLDER_M35" },
                            "quotaInfo": { "resetTime": five_hour_reset }
                        },
                        {
                            "label": "GPT-OSS 120B (Medium)",
                            "modelOrAlias": { "model": "MODEL_OPENAI_GPT_OSS_120B_MEDIUM" },
                            "quotaInfo": { "resetTime": five_hour_reset }
                        }
                    ]
                }
            }
        })
        .to_string();

        let snapshot = parse_user_status_response(&response).expect("user status should parse");
        let rows = snapshot
            .usage_rows
            .as_ref()
            .expect("Antigravity should expose grouped quota rows");

        assert_eq!(rows.len(), 2);
        assert_usage_row(&rows[0], "gemini-five-hour", 98.0);
        assert_usage_row(&rows[1], "claude-gpt-five-hour", 0.0);
    }

    fn assert_usage_row(row: &UsageRowSnapshot, id: &str, percent_left: f64) {
        assert_eq!(row.id, id);
        assert!(
            (row.percent_left - percent_left).abs() < 0.01,
            "{} percent left was {}",
            row.id,
            row.percent_left
        );
    }

    #[test]
    fn parses_google_oauth_token_refresh_snake_case_response() {
        // Google's token endpoint uses snake_case. A prior bug applied
        // rename_all = "camelCase" and rejected valid refresh responses.
        let response: TokenRefreshResponse = serde_json::from_str(
            r#"{
              "access_token": "ya29.test-token",
              "expires_in": 3599,
              "scope": "https://www.googleapis.com/auth/userinfo.email openid",
              "token_type": "Bearer",
              "id_token": "eyJhbGciOiJSUzI1NiJ9.e30.sig"
            }"#,
        )
        .expect("Google OAuth refresh JSON should parse");

        assert_eq!(response.access_token, "ya29.test-token");
        assert_eq!(response.expires_in, 3599);
        assert!(response.refresh_token.is_none());
    }

    #[test]
    fn parses_google_oauth_token_refresh_with_optional_refresh_token() {
        let response: TokenRefreshResponse = serde_json::from_str(
            r#"{
              "access_token": "ya29.rotated",
              "expires_in": 3600,
              "refresh_token": "1//0new-refresh",
              "token_type": "Bearer"
            }"#,
        )
        .expect("refresh response with new refresh_token should parse");

        assert_eq!(response.access_token, "ya29.rotated");
        assert_eq!(
            response.refresh_token.as_deref(),
            Some("1//0new-refresh")
        );
    }

    #[test]
    fn method_for_source_maps_native_auth_to_cloud() {
        assert_eq!(method_for_source("native-auth"), AntigravityMethod::Cloud);
        assert_eq!(method_for_source("native"), AntigravityMethod::Auto);
        assert_eq!(method_for_source("auto"), AntigravityMethod::Auto);
    }

    #[test]
    fn parses_cloud_code_fetch_available_models_into_quotas() {
        let weekly_reset = (Utc::now() + chrono::Duration::hours(168)).to_rfc3339();
        let five_hour_reset = (Utc::now() + chrono::Duration::hours(5)).to_rfc3339();
        let response: FetchAvailableModelsResponse = serde_json::from_str(&serde_json::json!({
            "models": {
                "claude-sonnet-4-5": {
                    "displayName": "Claude Sonnet 4.5",
                    "model": "MODEL_CLAUDE_SONNET",
                    "quotaInfo": { "remainingFraction": 0.42, "resetTime": weekly_reset }
                },
                "gemini-3-pro-low": {
                    "displayName": "Gemini 3 Pro (Low)",
                    "model": "MODEL_GEMINI_PRO_LOW",
                    "quotaInfo": { "remainingFraction": 0.25, "resetTime": five_hour_reset }
                },
                "gemini-3-flash": {
                    "displayName": "Gemini 3 Flash",
                    "model": "MODEL_GEMINI_FLASH",
                    "quotaInfo": { "remainingFraction": 0.8, "resetTime": five_hour_reset }
                },
                "chat_autocomplete": {
                    "displayName": "Autocomplete",
                    "quotaInfo": { "remainingFraction": 1.0, "resetTime": five_hour_reset }
                },
                "no-quota-model": {
                    "displayName": "No Quota"
                }
            }
        }).to_string()).expect("fetchAvailableModels JSON should parse");

        let quotas = quotas_from_models(response.models.unwrap_or_default());

        // chat_autocomplete (chat_ prefix) and no-quota-model are filtered out.
        assert_eq!(quotas.len(), 3);
        assert!(quotas.iter().any(|q| q.model_id == "MODEL_CLAUDE_SONNET"));
        assert!(quotas.iter().any(|q| q.model_id == "MODEL_GEMINI_PRO_LOW"));
        assert!(quotas.iter().any(|q| q.model_id == "MODEL_GEMINI_FLASH"));
        assert!(quotas.iter().all(|q| q.reset_time.is_some()));
    }

    #[test]
    fn cloud_code_quotas_build_three_family_snapshot() {
        let weekly_reset = (Utc::now() + chrono::Duration::hours(168)).to_rfc3339();
        let five_hour_reset = (Utc::now() + chrono::Duration::hours(5)).to_rfc3339();
        let quotas = vec![
            ModelQuota {
                label: "Claude Sonnet 4.5".to_string(),
                model_id: "MODEL_CLAUDE_SONNET".to_string(),
                remaining_fraction: Some(0.42),
                reset_time: parse_iso_date(&weekly_reset),
            },
            ModelQuota {
                label: "Gemini 3 Pro (Low)".to_string(),
                model_id: "MODEL_GEMINI_PRO_LOW".to_string(),
                remaining_fraction: Some(0.25),
                reset_time: parse_iso_date(&five_hour_reset),
            },
            ModelQuota {
                label: "Gemini 3 Flash".to_string(),
                model_id: "MODEL_GEMINI_FLASH".to_string(),
                remaining_fraction: Some(0.8),
                reset_time: parse_iso_date(&five_hour_reset),
            },
        ];

        let snapshot = usage_snapshot_from_quotas(quotas, Some("user@example.com".to_string()), Some("GOOGLE_AI_PRO".to_string()))
            .expect("cloud quotas should build a snapshot");

        assert_eq!(
            snapshot.primary.as_ref().map(|w| w.remaining_percent),
            Some(42.0)
        );
        assert_eq!(
            snapshot.secondary.as_ref().map(|w| w.remaining_percent),
            Some(25.0)
        );
        assert_eq!(
            snapshot.tertiary.as_ref().map(|w| w.remaining_percent),
            Some(80.0)
        );
        assert_eq!(
            snapshot
                .identity
                .as_ref()
                .and_then(|identity| identity.account_email.clone()),
            Some("user@example.com".to_string())
        );
        assert_eq!(
            snapshot
                .identity
                .as_ref()
                .and_then(|identity| identity.login_method.clone()),
            Some("GOOGLE_AI_PRO".to_string())
        );
    }

    #[test]
    fn extract_cloud_project_id_handles_string_and_object_forms() {
        assert_eq!(
            extract_cloud_project_id(&serde_json::json!("proj-123")),
            Some("proj-123".to_string())
        );
        assert_eq!(
            extract_cloud_id(&serde_json::json!({ "id": "proj-obj" })),
            Some("proj-obj".to_string())
        );
        assert_eq!(extract_cloud_project_id(&serde_json::json!("")), None);
        assert_eq!(extract_cloud_project_id(&serde_json::json!(null)), None);
    }

    fn extract_cloud_id(value: &serde_json::Value) -> Option<String> {
        extract_cloud_project_id(value)
    }
}
