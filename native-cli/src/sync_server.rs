//! Sync server: devices publish helper snapshots, the server merges them.
//!
//! Devices POST the widget's normalized usage snapshot (the
//! `codexbar-plasmoid-helper.mjs` stdout shape) as a JSON body. The server
//! stores one snapshot per device id and, on read, merges across devices:
//!
//! - Entries are keyed by (provider, account). The freshest non-error entry
//!   wins for limits, status, and metadata, because those are account-global.
//! - Daily token/cost history is summed per day across the devices that
//!   contributed the same (provider, account) key. Publications are
//!   device-local scans, so overlapping synced logs can double-count; the
//!   merged entry keeps per-device provenance (`remoteDevices`) so readers
//!   can label it.
//! - Per-day `limitResets` annotations are unions of what each publisher
//!   computed on its own local calendar; the server never re-derives them.
//!
//! Security: every data route needs a bearer token (fail-closed). Payloads
//! are the helper's post-normalization, post-anonymization output, so no
//! credentials or raw emails travel through the server when the publishing
//! widget anonymizes.

use crate::config::normalize_provider_id;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::{env, fs, thread, time::Duration};

/// Snapshots older than this are pruned. The helper publishes last-30-day
/// history, so keeping twice that window is generous.
const PRUNE_DAYS: i64 = 60;
/// Hard cap on one published snapshot body; helper payloads are well below
/// this because the daily history window is fixed at 30 days.
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
/// Maximum simultaneously handled connections; beyond this, new ones get a
/// 503 so a stuck client cannot starve the server.
const MAX_CONNECTIONS: isize = 32;
const DEFAULT_PORT: u16 = 8188;

pub struct SyncServerConfig {
    pub bind_addr: IpAddr,
    pub port: u16,
    /// Directory where per-device snapshots are stored.
    pub data_dir: PathBuf,
}

pub fn run_sync_server(args: &HashMap<String, String>) -> Result<()> {
    let config = SyncServerConfig::from_args(args)?;
    fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("create sync data dir {}", config.data_dir.display()))?;
    let token = resolve_token(args, &config.data_dir)?;
    print_startup(&config, &token);
    let snapshots = snapshots_dir(&config.data_dir);
    fs::create_dir_all(&snapshots)
        .with_context(|| format!("create snapshot dir {}", snapshots.display()))?;
    prune_old_snapshots(&snapshots);

    let addr = format!("{}:{}", config.bind_addr, config.port);
    let listener = TcpListener::bind(addr.clone())
        .with_context(|| format!("bind {addr} for the sync server"))?;
    // Report the real port so `--port 0` (tests) and users can find the socket.
    if let Ok(local) = listener.local_addr() {
        eprintln!(
            "sync-server listening on {local}, data dir {}",
            config.data_dir.display()
        );
    }
    serve(listener, config, token)
}

/// Accept loop; blocks forever. Public so tests can pass an already-bound
/// listener for an ephemeral port.
pub fn serve(listener: TcpListener, config: SyncServerConfig, token: String) -> Result<()> {
    let config = Arc::new(config);
    let token = Arc::new(token);
    let active: Arc<AtomicIsize> = Arc::new(AtomicIsize::new(0));
    loop {
        let (mut stream, _peer) = match listener.accept() {
            Ok(pair) => pair,
            Err(error) => {
                eprintln!("sync-server accept error: {error}");
                continue;
            }
        };
        if active.fetch_add(1, Ordering::Relaxed) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::Relaxed);
            let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
            let _ = write_json(
                &mut stream,
                503,
                &json!({"ok": false, "error": "server busy"}),
            );
            continue;
        }
        let cfg = Arc::clone(&config);
        let tok = Arc::clone(&token);
        let activity = Arc::clone(&active);
        thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
            let _ = stream.set_nodelay(true);
            if let Err(error) = handle_connection(stream, &cfg, &tok) {
                eprintln!("sync-server request error: {error:#}");
            }
            activity.fetch_sub(1, Ordering::Relaxed);
        });
    }
}

fn print_startup(config: &SyncServerConfig, token: &str) {
    eprintln!(
        "sync-server starting: bind {}:{} data dir {}",
        config.bind_addr,
        config.port,
        config.data_dir.display()
    );
    eprintln!(
        "sync-server auth: bearer token required on /v1 routes ({} bytes); \
         set CODEXBAR_SYNC_TOKEN or --token, otherwise one is kept at {}",
        token.len(),
        token_file_path(&config.data_dir).display()
    );
}

impl SyncServerConfig {
    fn from_args(args: &HashMap<String, String>) -> Result<Self> {
        let bind_name = args
            .get("bind")
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let bind_addr: IpAddr = bind_name
            .parse()
            .with_context(|| format!("invalid --bind address {bind_name:?}"))?;
        let port = args
            .get("port")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);
        let data_dir = args
            .get("data-dir")
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_data_dir);
        Ok(Self {
            bind_addr,
            port,
            data_dir,
        })
    }

    fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join("codexbar-plasmoid")
            .join("sync-server")
    }
}

/// Token precedence: `--token` flag > `CODEXBAR_SYNC_TOKEN` env > token file
/// in the data dir (generated on first run unless it already exists).
fn resolve_token(args: &HashMap<String, String>, data_dir: &PathBuf) -> Result<String> {
    let explicit = args
        .get("token")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(token) = explicit {
        return Ok(token);
    }
    if let Ok(token) = env::var("CODEXBAR_SYNC_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    let path = token_file_path(data_dir);
    if path.exists() {
        let existing = fs::read_to_string(&path)
            .with_context(|| format!("read sync token file {}", path.display()))?
            .trim()
            .to_string();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }
    let generated = uuid::Uuid::new_v4().simple().to_string();
    let mut handle = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .with_context(|| format!("create sync token file {}", path.display()))?;
    handle
        .write_all(generated.as_bytes())
        .with_context(|| format!("write sync token file {}", path.display()))?;
    Ok(generated)
}

fn token_file_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("token")
}

fn snapshots_dir(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("snapshots")
}

// ---------------------------------------------------------------------------
// HTTP plumbing (std only, no server deps)
// ---------------------------------------------------------------------------

struct ParsedRequest {
    method: String,
    path: String,
    query: Option<String>,
    bearer: Option<String>,
    body: Vec<u8>,
}

impl ParsedRequest {
    fn query_param(&self, name: &str) -> Option<String> {
        let query = self.query.as_deref()?;
        for pair in query.split('&') {
            let mut pieces = pair.splitn(2, '=');
            let key = pieces.next().unwrap_or("");
            if key == name {
                return Some(percent_decode(pieces.next().unwrap_or("")));
            }
        }
        None
    }
}

fn handle_connection(
    mut stream: TcpStream,
    config: &Arc<SyncServerConfig>,
    token: &Arc<String>,
) -> Result<()> {
    let (status, body) = match read_request(&mut stream) {
        Ok(request) => {
            let (status, body) = route(&request, config, token);
            if let ("PUT", Some(device)) = (
                request.method.as_str(),
                request.path.strip_prefix("/v1/snapshot/"),
            ) {
                if status == 200 {
                    eprintln!(
                        "sync-server stored snapshot for device {} ({} bytes)",
                        percent_decode(device),
                        request.body.len()
                    );
                }
            }
            (status, body)
        }
        Err((status, body)) => (status, body),
    };
    write_json(&mut stream, status, &body)
}

fn read_request(stream: &mut TcpStream) -> Result<ParsedRequest, (u16, Value)> {
    let mut raw: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 2048];
    let head_end;
    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|_| hard_error(400, "timeout reading request"))?;
        if read == 0 {
            return Err(hard_error(400, "connection closed before request head"));
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() > 64 * 1024 {
            return Err(hard_error(400, "request head too large"));
        }
        if let Some(index) = find_double_crlf(&raw) {
            head_end = index + 4;
            break;
        }
    }
    let head_text = String::from_utf8_lossy(&raw[..head_end]).to_string();
    let mut lines = head_text.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut pieces = request_line.split_whitespace();
    let method = pieces.next().unwrap_or("").to_ascii_uppercase();
    let target = pieces.next().unwrap_or("");
    if method.is_empty() || target.is_empty() {
        return Err(hard_error(400, "malformed request line"));
    }
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_string(), Some(query.to_string())),
        None => (target.to_string(), None),
    };

    let mut content_length: usize = 0;
    let mut bearer: Option<String> = None;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| hard_error(400, "malformed header line"))?;
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                content_length = value
                    .trim()
                    .parse()
                    .map_err(|_| hard_error(400, "invalid content-length"))?;
            }
            "authorization" => {
                let trimmed = value.trim();
                if trimmed.len() > 7 && trimmed[..7].eq_ignore_ascii_case("bearer ") {
                    bearer = Some(trimmed[7..].trim().to_string());
                }
            }
            _ => {}
        }
    }
    if content_length > MAX_BODY_BYTES {
        return Err(hard_error(413, "snapshot body too large"));
    }
    // Body bytes may have arrived in the same read as the head; take them
    // from the buffer first, then block only for the missing remainder.
    let mut body: Vec<u8> = raw[head_end.min(raw.len())..].to_vec();
    if body.len() > content_length {
        body.truncate(content_length);
    }
    if body.len() < content_length {
        let mut rest = vec![0u8; content_length - body.len()];
        stream
            .read_exact(&mut rest)
            .map_err(|_| hard_error(400, "truncated request body"))?;
        body.extend_from_slice(&rest);
    }
    Ok(ParsedRequest {
        method,
        path,
        query,
        bearer,
        body,
    })
}

fn find_double_crlf(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn hard_error(status: u16, message: &str) -> (u16, Value) {
    (status, json!({"ok": false, "error": message}))
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' && index + 2 < bytes.len() {
            if let Ok(decoded) = u8::from_str_radix(&input[index + 1..index + 3], 16) {
                out.push(decoded);
                index += 3;
                continue;
            }
        }
        out.push(byte);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn write_json(stream: &mut TcpStream, status: u16, body: &Value) -> Result<()> {
    let text = serde_json::to_string(body).unwrap_or_else(|_| "{\"ok\":false}".to_string());
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Bad Request",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        text.len(),
        text
    );
    stream
        .write_all(response.as_bytes())
        .context("write sync server response")?;
    stream.flush().context("flush sync server response")
}

fn authorized(request: &ParsedRequest, token: &str) -> bool {
    request
        .bearer
        .as_deref()
        .map(|given| given == token)
        .unwrap_or(false)
}

fn route(
    request: &ParsedRequest,
    config: &Arc<SyncServerConfig>,
    token: &Arc<String>,
) -> (u16, Value) {
    match request.path.as_str() {
        "/health" => {
            return (
                200,
                json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}),
            )
        }
        p if p == "/v1/usage" || p == "/v1/devices" || p.starts_with("/v1/snapshot/") => {}
        _ => return (404, json!({"ok": false, "error": "not found"})),
    }
    if !authorized(request, token.trim()) {
        return (401, json!({"ok": false, "error": "unauthorized"}));
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/usage") => handle_get_usage(request, config),
        ("GET", "/v1/devices") => handle_get_devices(config),
        ("PUT", path) => handle_put_snapshot(path, request, config),
        _ => (405, json!({"ok": false, "error": "method not allowed"})),
    }
}

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

fn handle_get_usage(request: &ParsedRequest, config: &Arc<SyncServerConfig>) -> (u16, Value) {
    let snapshots = load_snapshots(&snapshots_dir(&config.data_dir));
    let mut merged = merge_snapshots(&snapshots);
    if let Some(provider) = request.query_param("provider") {
        let normalized = normalize_provider_id(&provider);
        if let Some(entries) = merged["entries"].as_array_mut() {
            entries.retain(|entry| {
                normalize_provider_id(entry["provider"].as_str().unwrap_or("")) == normalized
            });
        }
        merged["requestedProvider"] = Value::String(provider);
    }
    (200, merged)
}

fn handle_get_devices(config: &Arc<SyncServerConfig>) -> (u16, Value) {
    let mut devices = Vec::new();
    let mut corrupt = 0;
    for path in snapshot_files(&snapshots_dir(&config.data_dir)) {
        match read_stored_snapshot(&path) {
            Ok(stored) => devices.push(json!({
                "device": stored.device,
                "receivedAt": stored.received_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                "generatedAt": stored.payload["generatedAt"],
                "entryCount": stored
                    .payload["entries"]
                    .as_array()
                    .map(|entries| entries.len())
                    .unwrap_or(0),
            })),
            Err(_) => corrupt += 1,
        }
    }
    devices.sort_by_key(|device| device["device"].to_string());
    (
        200,
        json!({"ok": true, "devices": devices, "corrupt": corrupt}),
    )
}

/// PUT /v1/snapshot/<device-id> — store the helper snapshot for one device.
/// Only healthy snapshots are accepted (`validate_snapshot`) so a failing
/// fetch cannot clobber the device's last good data.
fn handle_put_snapshot(
    path: &str,
    request: &ParsedRequest,
    config: &Arc<SyncServerConfig>,
) -> (u16, Value) {
    let raw_device = path.strip_prefix("/v1/snapshot/").unwrap_or("");
    if request.body.is_empty() {
        return (411, json!({"ok": false, "error": "missing request body"}));
    }
    let device = match sanitize_device_id(&percent_decode(raw_device)) {
        Some(device) => device,
        None => {
            return (
                422,
                json!({"ok": false, "error": "invalid device id (use letters, digits, . _ -, max 64 chars)"}),
            )
        }
    };
    let payload: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            return (
                422,
                json!({"ok": false, "error": format!("body is not valid JSON: {error}")}),
            )
        }
    };
    if let Err(message) = validate_snapshot(&payload) {
        return (422, json!({"ok": false, "error": message}));
    }
    if let Err(error) = store_snapshot(&snapshots_dir(&config.data_dir), &device, &payload) {
        return (
            500,
            json!({"ok": false, "error": format!("store snapshot: {error:#}")}),
        );
    }
    (200, json!({"ok": true, "device": device}))
}

fn validate_snapshot(payload: &Value) -> Result<(), String> {
    let Some(object) = payload.as_object() else {
        return Err("snapshot must be a JSON object".to_string());
    };
    if object.get("ok").and_then(Value::as_bool) == Some(false) {
        return Err("refusing error snapshot (ok=false)".to_string());
    }
    let Some(entries) = object.get("entries").and_then(Value::as_array) else {
        return Err("snapshot must contain an entries array".to_string());
    };
    if entries.iter().any(|entry| !entry.is_object()) {
        return Err("entries must all be objects".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

struct StoredSnapshot {
    device: String,
    received_at: DateTime<Utc>,
    payload: Value,
}

fn sanitize_device_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    if !bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn snapshot_path(dir: &PathBuf, device: &str) -> PathBuf {
    dir.join(format!("{device}.json"))
}

fn store_snapshot(dir: &PathBuf, device: &str, payload: &Value) -> Result<()> {
    // Recreate on demand: the dir may have been removed while the server ran.
    fs::create_dir_all(dir).with_context(|| format!("create snapshot dir {}", dir.display()))?;
    let stored = json!({
        "device": device,
        "receivedAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "payload": payload,
    });
    let bytes = serde_json::to_vec(&stored).context("serialize snapshot")?;
    let uuid = uuid::Uuid::new_v4().simple();
    let temp_path = dir.join(format!(".{device}.{uuid}.tmp"));
    {
        let mut temp = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp_path)
            .with_context(|| format!("create temp file {}", temp_path.display()))?;
        temp.write_all(&bytes)
            .with_context(|| format!("write temp file {}", temp_path.display()))?;
    }
    fs::rename(&temp_path, snapshot_path(dir, device)).with_context(|| {
        format!(
            "rename temp file into {}",
            snapshot_path(dir, device).display()
        )
    })?;
    prune_old_snapshots(dir);
    Ok(())
}

fn read_stored_snapshot(path: &PathBuf) -> Result<StoredSnapshot> {
    let text = fs::read_to_string(path).context("read snapshot file")?;
    let stored: Value = serde_json::from_str(&text).context("parse snapshot file")?;
    let device = stored["device"]
        .as_str()
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow::anyhow!("snapshot file missing device"))?;
    let received_at = parse_timestamp(stored["receivedAt"].as_str())
        .ok_or_else(|| anyhow::anyhow!("snapshot file missing receivedAt"))?;
    Ok(StoredSnapshot {
        device,
        received_at,
        payload: stored["payload"].clone(),
    })
}

fn parse_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    let text = value?;
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn snapshot_files(dir: &PathBuf) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read_dir
        .flatten()
        .map(|item| item.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect()
}

fn load_snapshots(dir: &PathBuf) -> Vec<StoredSnapshot> {
    snapshot_files(dir)
        .into_iter()
        .filter_map(|path| read_stored_snapshot(&path).ok())
        .collect()
}

fn prune_old_snapshots(dir: &PathBuf) {
    let cutoff = Utc::now() - ChronoDuration::days(PRUNE_DAYS);
    for path in snapshot_files(dir) {
        if let Ok(stored) = read_stored_snapshot(&path) {
            if stored.received_at < cutoff {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Merging
// ---------------------------------------------------------------------------

fn merge_snapshots(snapshots: &[StoredSnapshot]) -> Value {
    // (normalized provider, account) -> contributions
    let mut groups: HashMap<String, Vec<Contribution>> = HashMap::new();
    let mut cost_errors: Vec<String> = Vec::new();

    for snapshot in snapshots {
        if let Some(error) = snapshot.payload["costError"].as_str() {
            for piece in error.split(" | ") {
                let piece = piece.trim();
                if !piece.is_empty() && !cost_errors.contains(&piece.to_string()) {
                    cost_errors.push(piece.to_string());
                }
            }
        }
        let Some(entries) = snapshot.payload["entries"].as_array() else {
            continue;
        };
        for entry in entries {
            if !entry.is_object() {
                continue;
            }
            let provider = normalize_provider_id(entry["provider"].as_str().unwrap_or(""));
            if provider.is_empty() {
                continue;
            }
            let account = entry["account"].as_str().unwrap_or("").to_string();
            let key = format!("{provider}|{account}");
            let is_error = !entry["error"].is_null();
            groups.entry(key).or_default().push(Contribution {
                device: &snapshot.device,
                received_at: snapshot.received_at,
                entry,
                updated_at: parse_timestamp(entry["updatedAt"].as_str())
                    .unwrap_or(snapshot.received_at),
                is_error,
            });
        }
    }

    let mut merged_entries: Vec<Value> = groups
        .into_values()
        .map(|contributions| merge_entry_group(&contributions))
        .collect();
    merged_entries.sort_by(|left, right| merge_sort_key(left).cmp(&merge_sort_key(right)));

    json!({
        "ok": true,
        "generatedAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "requestedProvider": "all",
        "entries": merged_entries,
        "costError": if cost_errors.is_empty() {
            Value::Null
        } else {
            Value::String(cost_errors.join(" | "))
        },
    })
}

fn merge_sort_key(entry: &Value) -> String {
    format!(
        "{}|{}",
        entry["provider"].as_str().unwrap_or(""),
        entry["account"].as_str().unwrap_or("")
    )
}

struct Contribution<'a> {
    device: &'a str,
    received_at: DateTime<Utc>,
    entry: &'a Value,
    updated_at: DateTime<Utc>,
    is_error: bool,
}

fn merge_entry_group(contributions: &[Contribution]) -> Value {
    // Healthy entries beat error entries; freshness decides among equals.
    let mut chosen = &contributions[0];
    for candidate in &contributions[1..] {
        let candidate_better = if chosen.is_error != candidate.is_error {
            !candidate.is_error
        } else {
            (candidate.updated_at, candidate.received_at) > (chosen.updated_at, chosen.received_at)
        };
        if candidate_better {
            chosen = candidate;
        }
    }

    let mut merged = chosen.entry.clone();
    let merged_daily = merge_daily_usage(
        contributions
            .iter()
            .map(|contribution| &contribution.entry["dailyUsage"]),
    );
    if !merged_daily.is_empty() {
        merged["dailyUsage"] = Value::Array(merged_daily);
    }

    let contributors: Vec<&Value> = contributions.iter().map(|c| c.entry).collect();
    if let Some(token_usage) = merged_token_usage(
        &chosen.entry["tokenUsage"],
        merged["dailyUsage"].as_array(),
        &contributors,
    ) {
        merged["tokenUsage"] = token_usage;
    }

    let mut devices: Vec<Value> = contributions
        .iter()
        .map(|contribution| {
            json!({
                "device": contribution.device,
                "updatedAt": contribution.entry["updatedAt"],
                "receivedAt": contribution
                    .received_at
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            })
        })
        .collect();
    devices.sort_by(|left, right| right["updatedAt"].as_str().cmp(&left["updatedAt"].as_str()));
    merged["remoteDevices"] = Value::Array(devices);
    merged
}

/// Sum contributions of the same day across devices; model breakdowns merge
/// by name (mirrors the helper's mergeModelBreakdowns) and per-day reset
/// annotations union across publishers.
fn merge_daily_usage<'a>(usages: impl Iterator<Item = &'a Value>) -> Vec<Value> {
    let mut by_day: Map<String, Value> = Map::new();
    for usage in usages {
        let Some(rows) = usage.as_array() else {
            continue;
        };
        for row in rows {
            if !row.is_object() {
                continue;
            }
            let Some(day_key) = row["dayKey"].as_str() else {
                continue;
            };
            let slot = by_day.entry(day_key.to_string()).or_insert_with(|| {
                json!({
                    "dayKey": day_key,
                    "totalTokens": 0,
                    "costUSD": 0.0,
                    "models": [],
                    "limitResets": [],
                })
            });
            slot["totalTokens"] = number_value(
                slot["totalTokens"].as_f64().unwrap_or(0.0)
                    + row["totalTokens"].as_f64().unwrap_or(0.0),
            );
            slot["costUSD"] = json!(round_usd(
                slot["costUSD"].as_f64().unwrap_or(0.0) + row["costUSD"].as_f64().unwrap_or(0.0)
            ));
            merge_models(&mut slot["models"], &row["models"]);
            union_limit_resets(&mut slot["limitResets"], &row["limitResets"]);
        }
    }
    let mut merged: Vec<Value> = by_day.into_values().collect();
    merged.sort_by(|left, right| {
        left["dayKey"]
            .as_str()
            .unwrap_or("")
            .cmp(right["dayKey"].as_str().unwrap_or(""))
    });
    merged
}

/// Integer-ish sums stay JSON integers so numeric comparisons in readers see
/// `2000`, not `2000.0`.
fn number_value(sum: f64) -> Value {
    if sum.fract() == 0.0 && sum.abs() < 9e15 {
        json!(sum as i64)
    } else {
        json!(sum)
    }
}

fn round_usd(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn merge_models(into: &mut Value, from: &Value) {
    let Some(from_rows) = from.as_array() else {
        return;
    };
    if !into.is_array() {
        *into = Value::Array(Vec::new());
    }
    let target = into.as_array_mut().expect("array initialized above");
    for row in from_rows {
        let Some(name) = row["name"].as_str() else {
            continue;
        };
        if let Some(existing) = target
            .iter_mut()
            .find(|candidate| candidate["name"].as_str() == Some(name))
        {
            existing["costUSD"] = json!(round_usd(
                existing["costUSD"].as_f64().unwrap_or(0.0)
                    + row["costUSD"].as_f64().unwrap_or(0.0)
            ));
            existing["totalTokens"] = number_value(
                existing["totalTokens"].as_f64().unwrap_or(0.0)
                    + row["totalTokens"].as_f64().unwrap_or(0.0),
            );
            continue;
        }
        target.push(json!({
            "name": name,
            "costUSD": round_usd(row["costUSD"].as_f64().unwrap_or(0.0)),
            "totalTokens": number_value(row["totalTokens"].as_f64().unwrap_or(0.0)),
        }));
    }
}

/// Union per-day reset annotations from the publishers' own local-calendar
/// annotations, deduped by (title, resetsAt).
fn union_limit_resets(into: &mut Value, from: &Value) {
    let Some(from_rows) = from.as_array() else {
        return;
    };
    if !into.is_array() {
        *into = Value::Array(Vec::new());
    }
    let target = into.as_array_mut().expect("array initialized above");
    for row in from_rows {
        let Some(title) = row["title"].as_str() else {
            continue;
        };
        let resets_at = row["resetsAt"].as_str();
        if target.iter().any(|candidate| {
            candidate["title"].as_str() == Some(title)
                && candidate["resetsAt"].as_str() == resets_at
        }) {
            continue;
        }
        target.push(row.clone());
    }
}

/// Cross-device tokenUsage. The chosen (freshest) entry's tokenUsage is the
/// base: keep its session fields, labels, currency, and provenance, replace
/// the 30-day totals with sums over the merged daily rows, and keep coverage
/// true only when the chosen device's scan was complete AND every other
/// contributor's own scan was complete (fair warning for the reader).
fn merged_token_usage(
    chosen: &Value,
    merged_daily: Option<&Vec<Value>>,
    contributions: &[&Value],
) -> Option<Value> {
    let merged_rows = merged_daily?;
    if !chosen.is_object() {
        // No tokenUsage on the freshest entry? Adopt one from a contributor
        // that did supply it, then stamp sums over the merged history.
        let donor = contributions.iter().find(|value| value.is_object())?;
        let mut adopted = (**donor).clone();
        apply_day_sums(&mut adopted, merged_rows);
        return Some(adopted);
    }
    let mut merged_usage = chosen.clone();
    let all_covered = merged_usage["historyCoverageIsEstablished"]
        .as_bool()
        .unwrap_or(false)
        && contributions
            .iter()
            .filter(|value| value.is_object() && *value != &chosen)
            .all(|value| {
                value["historyCoverageIsEstablished"]
                    .as_bool()
                    .unwrap_or(true)
            });
    merged_usage["historyCoverageIsEstablished"] = json!(all_covered);
    apply_day_sums(&mut merged_usage, merged_rows);
    Some(merged_usage)
}

fn apply_day_sums(token_usage: &mut Value, merged_rows: &[Value]) {
    let mut total_cost = 0.0;
    let mut total_tokens = 0.0;
    for row in merged_rows {
        total_cost += row["costUSD"].as_f64().unwrap_or(0.0);
        total_tokens += row["totalTokens"].as_f64().unwrap_or(0.0);
    }
    token_usage["last30DaysCostUSD"] = json!(round_usd(total_cost));
    token_usage["last30DaysTokens"] = number_value(total_tokens);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(device: &str, received: &str, payload: Value) -> StoredSnapshot {
        StoredSnapshot {
            device: device.to_string(),
            received_at: DateTime::parse_from_rfc3339(received)
                .map(|parsed| parsed.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            payload,
        }
    }

    #[test]
    fn freshest_entry_wins_per_account() {
        let older = snapshot(
            "alpha",
            "2026-09-15T10:00:00Z",
            json!({
                "generatedAt": "2026-09-15T10:00:00Z",
                "costError": null,
                "entries": [{
                    "provider": "codex",
                    "account": "user@example.com",
                    "updatedAt": "2026-09-15T09:55:00Z",
                    "rows": [{"id": "session", "percentLeft": 42}],
                }],
            }),
        );
        let newer = snapshot(
            "beta",
            "2026-09-15T11:00:00Z",
            json!({
                "generatedAt": "2026-09-15T11:00:00Z",
                "costError": null,
                "entries": [{
                    "provider": "codex",
                    "account": "user@example.com",
                    "updatedAt": "2026-09-15T10:55:00Z",
                    "rows": [{"id": "session", "percentLeft": 88}],
                }],
            }),
        );
        let merged = merge_snapshots(&[older, newer]);
        let entries = merged["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["rows"][0]["percentLeft"], 88);
        let devices = entries[0]["remoteDevices"].as_array().unwrap();
        assert_eq!(devices[0]["device"], "beta");
        assert_eq!(devices[1]["device"], "alpha");
    }

    #[test]
    fn account_differences_are_kept() {
        let alpha = snapshot(
            "alpha",
            "2026-09-15T10:00:00Z",
            json!({
                "entries": [{
                    "provider": "claude",
                    "account": "one@example.com",
                    "updatedAt": "2026-09-15T09:55:00Z",
                }],
            }),
        );
        let beta = snapshot(
            "beta",
            "2026-09-15T10:00:00Z",
            json!({
                "entries": [{
                    "provider": "claude",
                    "account": "two@example.com",
                    "updatedAt": "2026-09-15T09:55:00Z",
                }],
            }),
        );
        let merged = merge_snapshots(&[alpha, beta]);
        assert_eq!(merged["entries"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn daily_usage_sums_across_devices() {
        let alpha = snapshot(
            "alpha",
            "2026-09-15T10:00:00Z",
            json!({
                "entries": [{
                    "provider": "codex",
                    "account": "user@example.com",
                    "updatedAt": "2026-09-15T09:55:00Z",
                    "tokenUsage": {
                        "provenance": "listPriceEstimate",
                        "historyCoverageIsEstablished": true,
                        "currencyCode": "USD",
                    },
                    "dailyUsage": [
                        {"dayKey": "2026-09-14", "totalTokens": 500, "costUSD": 1.25,
                         "models": [{"name": "gpt", "costUSD": 1.25, "totalTokens": 500}]},
                        {"dayKey": "2026-09-15", "totalTokens": 800, "costUSD": 2.0, "models": []},
                    ],
                }],
            }),
        );
        let beta = snapshot(
            "beta",
            "2026-09-15T11:00:00Z",
            json!({
                "entries": [{
                    "provider": "codex",
                    "account": "user@example.com",
                    "updatedAt": "2026-09-15T10:55:00Z",
                    "tokenUsage": {
                        "provenance": "listPriceEstimate",
                        "historyCoverageIsEstablished": false,
                        "currencyCode": "USD",
                    },
                    "dailyUsage": [
                        {"dayKey": "2026-09-15", "totalTokens": 1200, "costUSD": 3.1,
                         "models": [{"name": "gpt", "costUSD": 0.6, "totalTokens": 200}]},
                    ],
                }],
            }),
        );
        let merged = merge_snapshots(&[alpha, beta]);
        let entry = &merged["entries"].as_array().unwrap()[0];
        let days = entry["dailyUsage"].as_array().unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(days[0]["totalTokens"], 500);
        assert_eq!(days[0]["costUSD"], 1.25);
        assert_eq!(days[1]["totalTokens"], 2000);
        assert_eq!(days[1]["costUSD"], 5.1);
        // Day 15's per-model split only exists on beta (alpha's day had none).
        let latest_models = days[1]["models"].as_array().unwrap();
        assert_eq!(latest_models.len(), 1);
        assert_eq!(latest_models[0]["totalTokens"], 200);
        assert_eq!(entry["tokenUsage"]["historyCoverageIsEstablished"], false);
        assert_eq!(entry["tokenUsage"]["last30DaysCostUSD"], 6.35);
        assert_eq!(entry["tokenUsage"]["last30DaysTokens"], 2500);
    }

    #[test]
    fn limit_resets_union_across_publishers() {
        let alpha = snapshot(
            "alpha",
            "2026-09-15T10:00:00Z",
            json!({
                "entries": [{
                    "provider": "codex",
                    "account": "user@example.com",
                    "updatedAt": "2026-09-15T09:55:00Z",
                    "rows": [{"id": "weekly", "percentLeft": 35, "resetsAt": "2026-09-18T12:00:00Z"}],
                    "dailyUsage": [
                        {"dayKey": "2026-09-18", "totalTokens": 100, "costUSD": 0.5, "limitResets": [
                            {"title": "Weekly", "percentLeft": 35, "resetsAt": "2026-09-18T12:00:00Z"},
                        ]},
                    ],
                }],
            }),
        );
        let beta = snapshot(
            "beta",
            "2026-09-15T11:00:00Z",
            json!({
                "entries": [{
                    "provider": "codex",
                    "account": "user@example.com",
                    "updatedAt": "2026-09-15T10:55:00Z",
                    "rows": [{"id": "weekly", "percentLeft": 35, "resetsAt": "2026-09-18T12:00:00Z"}],
                    "dailyUsage": [
                        {"dayKey": "2026-09-18", "totalTokens": 10, "costUSD": 0.05, "limitResets": [
                            {"title": "Weekly", "percentLeft": 35, "resetsAt": "2026-09-18T12:00:00Z"},
                            {"title": "Bonus", "percentLeft": 10, "resetsAt": "2026-09-18T18:00:00Z"},
                        ]},
                    ],
                }],
            }),
        );
        let merged = merge_snapshots(&[alpha, beta]);
        let entry = &merged["entries"].as_array().unwrap()[0];
        let resets = entry["dailyUsage"][0]["limitResets"].as_array().unwrap();
        assert_eq!(
            resets.len(),
            2,
            "duplicate title+resetsAt collapses, new one joins"
        );
        assert!(resets.iter().any(|reset| reset["title"] == "Bonus"));
        assert!(
            resets
                .iter()
                .filter(|reset| reset["title"] == "Weekly")
                .count()
                == 1
        );
    }

    #[test]
    fn error_entries_lose_to_healthy_ones() {
        let broken = snapshot(
            "alpha",
            "2026-09-15T12:00:00Z",
            json!({
                "entries": [{
                    "provider": "devin",
                    "account": "",
                    "updatedAt": "2026-09-15T11:55:00Z",
                    "error": {"message": "boom"},
                }],
            }),
        );
        let healthy = snapshot(
            "beta",
            "2026-09-15T09:00:00Z",
            json!({
                "entries": [{
                    "provider": "devin",
                    "account": "",
                    "updatedAt": "2026-09-15T08:55:00Z",
                    "rows": [],
                }],
            }),
        );
        let merged = merge_snapshots(&[broken, healthy]);
        let entries = merged["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0]["error"].is_null());
    }

    #[test]
    fn cost_errors_union_distinct_messages() {
        let alpha = snapshot(
            "alpha",
            "2026-09-15T10:00:00Z",
            json!({
                "entries": [],
                "costError": "OpenCode not found | Cursor cookie missing",
            }),
        );
        let beta = snapshot(
            "beta",
            "2026-09-15T10:00:00Z",
            json!({
                "entries": [],
                "costError": "Cursor cookie missing",
            }),
        );
        let merged = merge_snapshots(&[alpha, beta]);
        assert_eq!(
            merged["costError"],
            "OpenCode not found | Cursor cookie missing"
        );
    }

    #[test]
    fn missing_device_id_payloads_reject() {
        assert!(sanitize_device_id("good-1.laptop_2").is_some());
        assert!(sanitize_device_id("").is_none());
        assert!(sanitize_device_id("../etc/passwd").is_none());
        assert!(sanitize_device_id(&"a".repeat(65)).is_none());
    }
}
