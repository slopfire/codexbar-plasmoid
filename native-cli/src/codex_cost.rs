//! Native Codex session cost scanner.
//!
//! Replacement for `codexbar cost --provider codex` (the upstream Swift CLI
//! segfaults on Linux while walking Codex history). Reads Codex rollout
//! transcripts as JSONL:
//!
//! ```text
//! $CODEX_HOME/sessions/YYYY/MM/DD/rollout-*.jsonl   ($CODEX_HOME ?= ~/.codex)
//! $CODEX_HOME/archived_sessions/*.jsonl             (flat layout)
//! ```
//!
//! Billing rules (parity with the upstream scanner, deviations documented):
//! - Billed unit is `token_usage_record.payload.usage` (per-response delta).
//!   `turn/thread_token_usage` are cumulative counters, never billed.
//! - Ownership: `thread_id` must equal the file session (first `session_meta`
//!   id); a record `session_id`, when present, must equal the ledger session
//!   (first meta's `payload.session_id`, falling back to its id). Fork files
//!   carry the parent id here — comparing against the file id instead would
//!   drop every fork record.
//! - Dedup by `response_id` (globally, keeping the max), so fork-copied rows
//!   cannot double-bill even when the parent file is present. (Upstream also
//!   suppresses pre-`subagent_history_start_ordinal` copied prefixes and
//!   subtracts fork baselines; v1 relies on response dedup instead —
//!   copies share response ids with the parent scan.)
//! - `event_msg`/`token_count` (`info.last_token_usage`) is billed ONLY in
//!   files that contain zero valid usage records (headless `codex_exec`
//!   sessions). This can never double-count, at the price of ignoring legacy
//!   rows in mixed files (documented deviation).
//! - Model comes from the nearest preceding `turn_context` in file order
//!   (`payload.model/d_model_name`, then `payload.info.model/model_name`);
//!   unknown when absent. Routed `cline-pass/*` style ids are unpriced.
//! - Day buckets are UTC calendar days, consistent with the other native
//!   providers in `local_cost.rs` (upstream uses local-calendar days).
//!
//! Pricing: per-model API list rates (`listPriceEstimate` provenance) from
//! the upstream models.dev catalog cache when present, else the bundled
//! table below. Unknown models count tokens at $0 (never invent rates).

use crate::local_cost::{snapshot_from_rows, CostRow, CostSnapshot};
use anyhow::Result;
use chrono::DateTime;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

// --- Model normalization ---------------------------------------------------

/// Routes whose `route/model` ids may be priced under the route's provider.
/// Anything else routed is unpriced (never borrow another vendor's price).
const PRICEABLE_ROUTES: &[&str] = &[
    "deepseek",
    "kimi-coding",
    "kimi-for-coding",
    "openai",
    "opencode",
    "opencode-free",
    "opencode-go",
];

/// Normalize a raw model string to a lookup key.
/// Returns None when the id can never be priced.
fn normalize_model_id(raw: &str) -> Option<String> {
    let mut id = raw.trim().to_string();
    if id.is_empty() || id.eq_ignore_ascii_case("unknown") {
        return None;
    }
    if let Some(rest) = id.strip_prefix("openai/") {
        id = rest.to_string();
    }
    // Upstream aliases.
    id = match id.as_str() {
        "gpt-5.6" => "gpt-5.6-sol".to_string(),
        "gpt-reserve" => "gpt-5.6-luna".to_string(),
        "gpt-daybreak-blue-latest" => "gpt-5.6-sol".to_string(),
        "gpt-daybreak-red-latest" => "gpt-5.6-cyber".to_string(),
        other => other.to_string(),
    };
    // Routed ids: keep only priceable routes, look up the bare model part.
    if let Some((route, model)) = id.split_once('/') {
        if !PRICEABLE_ROUTES.contains(&route) {
            return None;
        }
        id = model.to_string();
    }
    // Strip one trailing date/version suffix and retry.
    let mut key = id.clone();
    if !is_known_key(&key) {
        if let Some(stripped) = strip_version_suffix(&key) {
            if is_known_key(&stripped) {
                key = stripped;
            }
        }
    }
    Some(key)
}

/// Strip `-YYYY-MM-DD`, `-YYYYMMDD`, or `-vN:M` style suffixes.
fn strip_version_suffix(key: &str) -> Option<String> {
    // -YYYY-MM-DD (11 chars) or -YYYYMMDD (9 chars).
    if key.len() > 11 && key.as_bytes()[key.len() - 11] == b'-' {
        let tail = &key[key.len() - 10..];
        if tail.len() == 10
            && tail.as_bytes()[4] == b'-'
            && tail.as_bytes()[7] == b'-'
            && tail[..4].bytes().all(|b| b.is_ascii_digit())
            && tail[5..7].bytes().all(|b| b.is_ascii_digit())
            && tail[8..].bytes().all(|b| b.is_ascii_digit())
        {
            return Some(key[..key.len() - 11].to_string());
        }
    }
    if key.len() > 9 && key.as_bytes()[key.len() - 9] == b'-' {
        let tail = &key[key.len() - 8..];
        if tail.bytes().all(|b| b.is_ascii_digit()) {
            return Some(key[..key.len() - 9].to_string());
        }
    }
    // -vN:M / -vN suffix.
    if let Some(pos) = key.rfind("-v") {
        let tail = &key[pos + 2..];
        if !tail.is_empty()
            && tail
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'.' || b == b':')
        {
            return Some(key[..pos].to_string());
        }
    }
    None
}

// --- Rates -----------------------------------------------------------------

/// USD per 1M tokens. `None` rates fold to the input rate (legacy folding).
#[derive(Clone, Copy, Debug)]
struct AboveRates {
    input_per_m: f64,
    output_per_m: f64,
    cached_per_m: Option<f64>,
    write_per_m: Option<f64>,
}

/// USD per 1M tokens. `None` rates fold to the input rate (legacy folding).
#[derive(Clone, Copy, Debug)]
struct ModelRates {
    input_per_m: f64,
    output_per_m: f64,
    cached_per_m: Option<f64>,
    write_per_m: Option<f64>,
    /// Long-context threshold (input tokens); whole request flips above it.
    threshold: Option<i64>,
    above: Option<AboveRates>,
}

/// Bundled fallback table (USD per 1M). Mirrors the upstream bundled dict.
const BUNDLED_RATES: &[(&str, ModelRates)] = &[
    ("gpt-5.6-sol", rates(4.0, 20.0, Some(0.4), Some(5.0), Some(272_000), Some(over(8.0, 30.0, Some(0.8), Some(10.0))))),
    ("gpt-5.6-terra", rates(2.0, 12.0, Some(0.2), Some(2.5), Some(272_000), Some(over(4.0, 18.0, Some(0.4), Some(5.0))))),
    ("gpt-6-sol", rates(2.0, 10.0, Some(0.2), Some(2.5), Some(272_000), Some(over(4.0, 15.0, Some(0.4), Some(5.0))))),
    ("gpt-5.6-luna", rates(0.2, 1.2, Some(0.02), Some(0.25), Some(272_000), Some(over(0.4, 1.8, Some(0.04), Some(0.5))))),
    ("gpt-6-astra", rates(10.0, 50.0, Some(1.0), Some(12.5), Some(272_000), Some(over(20.0, 75.0, Some(2.0), Some(25.0))))),
    ("gpt-6.1-sol", rates(2.0, 10.0, Some(0.1), Some(2.5), Some(272_000), Some(over(4.0, 15.0, Some(0.2), Some(5.0))))),
    ("gpt-6-luna", rates(0.1, 0.5, Some(0.01), Some(0.125), Some(272_000), Some(over(0.2, 0.75, Some(0.02), Some(0.25))))),
    ("gpt-5", rates(1.25, 10.0, Some(0.125), None, None, None)),
    ("gpt-5-codex", rates(1.25, 10.0, Some(0.125), None, None, None)),
    ("gpt-5-mini", rates(0.25, 2.0, Some(0.025), None, None, None)),
    ("gpt-5-nano", rates(0.05, 0.4, Some(0.005), None, None, None)),
    ("gpt-5-pro", rates(15.0, 120.0, None, None, None, None)),
    ("gpt-5.1", rates(1.25, 10.0, Some(0.125), None, None, None)),
    ("gpt-5.1-codex", rates(1.25, 10.0, Some(0.125), None, None, None)),
    ("gpt-5.1-codex-max", rates(1.25, 10.0, Some(0.125), None, None, None)),
    ("gpt-5.1-codex-mini", rates(0.25, 2.0, Some(0.025), None, None, None)),
    ("gpt-5.2", rates(1.75, 14.0, Some(0.175), None, None, None)),
    ("gpt-5.2-codex", rates(1.75, 14.0, Some(0.175), None, None, None)),
    ("gpt-5.2-pro", rates(21.0, 168.0, None, None, None, None)),
    ("gpt-5.3-codex", rates(1.75, 14.0, Some(0.175), None, None, None)),
    ("gpt-5.3-codex-spark", rates(1.75, 14.0, Some(0.175), None, None, None)),
    ("gpt-5.4", rates(2.5, 15.0, Some(0.25), None, Some(272_000), Some(over(5.0, 22.5, Some(0.5), None)))),
    ("gpt-5.4-mini", rates(0.75, 4.5, Some(0.075), None, None, None)),
    ("gpt-5.4-nano", rates(0.2, 1.25, Some(0.02), None, None, None)),
    ("gpt-5.4-pro", rates(30.0, 180.0, None, None, Some(272_000), Some(over(60.0, 270.0, None, None)))),
    ("gpt-5.5", rates(5.0, 30.0, Some(0.5), None, Some(272_000), Some(over(10.0, 45.0, Some(1.0), None)))),
    ("gpt-5.5-pro", rates(30.0, 180.0, None, None, Some(272_000), Some(over(60.0, 270.0, None, None)))),
    ("gpt-5.6-cyber", rates(12.5, 75.0, Some(1.25), Some(15.625), None, None)),
    ("gpt-5.5-cyber", rates(12.5, 75.0, Some(1.25), None, None, None)),
];

const fn rates(
    input_per_m: f64,
    output_per_m: f64,
    cached_per_m: Option<f64>,
    write_per_m: Option<f64>,
    threshold: Option<i64>,
    above: Option<AboveRates>,
) -> ModelRates {
    ModelRates { input_per_m, output_per_m, cached_per_m, write_per_m, threshold, above }
}

const fn over(
    input_per_m: f64,
    output_per_m: f64,
    cached_per_m: Option<f64>,
    write_per_m: Option<f64>,
) -> AboveRates {
    AboveRates { input_per_m, output_per_m, cached_per_m, write_per_m }
}

fn is_known_key(key: &str) -> bool {
    BUNDLED_RATES.iter().any(|(k, _)| *k == key)
}

/// Historical (date-dependent) overrides: (model, cutoff_ms, rates).
/// Rows older than the cutoff use these instead of catalog/bundled.
const HISTORICAL_RATES: &[(&str, i64, ModelRates)] = &[
    ("gpt-5.6-sol", 1_787_270_400_000, rates(5.0, 30.0, Some(0.5), Some(6.25), Some(272_000), Some(over(10.0, 45.0, Some(1.0), Some(12.5))))),
    ("gpt-5.6-terra", 1_785_369_600_000, rates(2.5, 15.0, Some(0.25), Some(3.125), Some(272_000), Some(over(5.0, 22.5, Some(0.5), Some(6.25))))),
    ("gpt-5.6-luna", 1_785_369_600_000, rates(1.0, 6.0, Some(0.1), Some(1.25), Some(272_000), Some(over(2.0, 9.0, Some(0.2), Some(2.5))))),
];

/// Free Auto-review policy: this model at/after the cutoff is plan-included ($0).
const AUTOREVIEW_MODEL: &str = "codex-auto-review";
const AUTOREVIEW_FREE_SINCE_MS: i64 = 1_791_270_000_000;

/// Runtime catalog cache artifact version (filename-embedded).
const CATALOG_ARTIFACT_VERSION: u32 = 1;

/// Rates resolved from the models.dev catalog cache file.
type CatalogMap = HashMap<String, ModelRates>;

fn catalog_path() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(if std::env::var("XDG_CACHE_HOME").ok().filter(|v| !v.trim().is_empty()).is_some() { "" } else { ".cache" });
    base.join("CodexBar")
        .join("model-pricing")
        .join(format!("models-dev-v{}.json", CATALOG_ARTIFACT_VERSION))
}

fn json_f64(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|n| n as f64)).or_else(|| {
        value.as_u64().and_then(|n| {
            if n <= i64::MAX as u64 {
                Some(n as f64)
            } else {
                None
            }
        })
    })
}

/// Load priceable `openai`-provider entries from the catalog cache.
/// Returns None when the file is missing/invalid/versioned differently.
fn load_catalog(path: &Path) -> Option<CatalogMap> {
    let text = fs::read_to_string(path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    if root.get("version").and_then(|v| v.as_u64()) != Some(CATALOG_ARTIFACT_VERSION as u64) {
        return None;
    }
    let models = root.pointer("/catalog/providers/openai/models")?.as_object()?;
    let mut out = CatalogMap::new();
    for (key, entry) in models {
        let cost = entry.get("cost")?;
        let input = json_f64(cost.get("input")?)?;
        let output = json_f64(cost.get("output")?)?;
        if !(input.is_finite() && output.is_finite() && input >= 0.0 && output >= 0.0) {
            continue;
        }
        let cached = cost.get("cache_read").and_then(json_f64).filter(|v| v.is_finite() && *v >= 0.0);
        let write = cost.get("cache_write").and_then(json_f64).filter(|v| v.is_finite() && *v >= 0.0);
        let (threshold, above) = resolve_catalog_threshold(cost);
        out.insert(
            key.clone(),
            ModelRates {
                input_per_m: input,
                output_per_m: output,
                cached_per_m: cached,
                write_per_m: write,
                threshold,
                above,
            },
        );
    }
    Some(out)
}

/// Threshold resolution: match a context tier whose rates equal
/// `context_over_200k`, else 272_000 fallback when the block is present.
fn resolve_catalog_threshold(cost: &serde_json::Value) -> (Option<i64>, Option<AboveRates>) {
    let over = cost.get("context_over_200k");
    let Some(over) = over else {
        return (None, None);
    };
    let above = AboveRates {
        input_per_m: json_f64(over.get("input").unwrap_or(&serde_json::Value::Null)).unwrap_or(f64::NAN),
        output_per_m: json_f64(over.get("output").unwrap_or(&serde_json::Value::Null)).unwrap_or(f64::NAN),
        cached_per_m: over.get("cache_read").and_then(json_f64),
        write_per_m: over.get("cache_write").and_then(json_f64),
    };
    if !above.input_per_m.is_finite() || !above.output_per_m.is_finite() {
        return (None, None);
    }
    let mut threshold = Some(272_000i64);
    if let Some(tiers) = cost.get("tiers").and_then(|t| t.as_array()) {
        for tier in tiers {
            let size = tier.pointer("/tier/size").and_then(|v| v.as_i64()).unwrap_or(0);
            let is_context = tier.pointer("/tier/type").and_then(|v| v.as_str()) == Some("context");
            if !is_context || size <= 0 {
                continue;
            }
            // Compare with missing==missing semantics.
            let same = [("input", Some(above.input_per_m)), ("output", Some(above.output_per_m)),
                        ("cache_read", above.cached_per_m), ("cache_write", above.write_per_m)]
                .iter()
                .all(|(k, want)| match (tier.get(k).and_then(json_f64), *want) {
                    (None, None) => true,
                    (Some(a), Some(b)) => (a - b).abs() < 1e-12,
                    _ => false,
                });
            if same {
                threshold = Some(size);
                break;
            }
        }
    }
    (threshold, Some(above))
}

fn bundled_rates(key: &str) -> Option<ModelRates> {
    BUNDLED_RATES.iter().find(|(k, _)| *k == key).map(|(_, r)| *r)
}

/// Resolve rates for a normalized model id at a row timestamp (ms).
/// Order: historical override (predates cutoff) → catalog → bundled.
/// Returns None for unpriced ids (tokens still counted, $0).
fn resolve_rates(key: &str, row_ms: i64, catalog: Option<&CatalogMap>) -> Option<ModelRates> {
    for (hk, cutoff, rates) in HISTORICAL_RATES {
        if *hk == key && row_ms < *cutoff {
            return Some(*rates);
        }
    }
    if let Some(cat) = catalog {
        if let Some(r) = cat.get(key) {
            return Some(*r);
        }
    }
    bundled_rates(key)
}

/// Price one validated usage row. Unknown model → None (billed as $0).
/// `model_key` is the normalized id (None when absent/unnamed).
fn price_usage(
    usage: &Usage,
    model_key: Option<&str>,
    row_ms: i64,
    catalog: Option<&CatalogMap>,
) -> Option<f64> {
    let key = model_key?;
    if key == AUTOREVIEW_MODEL && row_ms >= AUTOREVIEW_FREE_SINCE_MS {
        return Some(0.0);
    }
    let rates = resolve_rates(key, row_ms, catalog)?;
    let cached = usage.cached.min(usage.input);
    let remaining = usage.input - cached;
    let write = usage.write.min(remaining);
    let plain = remaining - write;
    let long = rates.threshold.is_some_and(|thr| usage.input > thr);
    let in_rate = if long {
        rates.above.map(|a| a.input_per_m).unwrap_or(rates.input_per_m)
    } else {
        rates.input_per_m
    };
    let cached_rate = if long {
        rates.above.and_then(|a| a.cached_per_m).or(rates.cached_per_m).unwrap_or(in_rate)
    } else {
        rates.cached_per_m.unwrap_or(in_rate)
    };
    let write_rate = if long {
        rates.above.and_then(|a| a.write_per_m).or(rates.write_per_m).unwrap_or(in_rate)
    } else {
        rates.write_per_m.unwrap_or(in_rate)
    };
    let out_rate = if long {
        rates.above.map(|a| a.output_per_m).unwrap_or(rates.output_per_m)
    } else {
        rates.output_per_m
    };
    let cost = plain as f64 * in_rate / 1_000_000.0
        + cached as f64 * cached_rate / 1_000_000.0
        + write as f64 * write_rate / 1_000_000.0
        + usage.output as f64 * out_rate / 1_000_000.0;
    Some(cost)
}

// --- Usage parsing ---------------------------------------------------------

/// Validated token counts. cached ⊆ input, write ⊆ remainder, reasoning ⊆ output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Usage {
    input: i64,
    cached: i64,
    write: i64,
    output: i64,
    reasoning: i64,
}

fn json_int_strict(value: &serde_json::Value) -> Option<i64> {
    let v = value;
    if v.is_boolean() {
        return None;
    }
    if let Some(n) = v.as_i64() {
        return if n >= 0 { Some(n) } else { None };
    }
    if let Some(n) = v.as_u64() {
        return if n <= i64::MAX as u64 { Some(n as i64) } else { None };
    }
    if let Some(f) = v.as_f64() {
        if f.is_finite() && f >= 0.0 && f < i64::MAX as f64 && f.round() == f {
            return Some(f as i64);
        }
    }
    None
}

/// Strict usage validation (token_usage_record path).
fn parse_usage_strict(obj: &serde_json::Value) -> Option<Usage> {
    let input = obj.get("input_tokens").and_then(json_int_strict)?;
    let output = obj.get("output_tokens").and_then(json_int_strict)?;
    let cached = obj.get("cached_input_tokens").and_then(json_int_strict).unwrap_or(0);
    if cached > input {
        return None;
    }
    let write = obj.get("cache_write_input_tokens").and_then(json_int_strict).unwrap_or(0);
    if write > input - cached {
        return None;
    }
    let reasoning = obj.get("reasoning_output_tokens").and_then(json_int_strict).unwrap_or(0);
    if reasoning > output {
        return None;
    }
    input.checked_add(output)?;
    Some(Usage { input, cached, write, output, reasoning })
}

/// Lenient usage parsing (token_count path): clamp instead of rejecting.
fn parse_usage_lenient(obj: &serde_json::Value) -> Option<Usage> {
    let input = obj.get("input_tokens").and_then(json_int_strict)?;
    let output = obj.get("output_tokens").and_then(json_int_strict)?;
    let cached = obj.get("cached_input_tokens").and_then(json_int_strict).unwrap_or(0).min(input);
    let write = obj.get("cache_write_input_tokens").and_then(json_int_strict).unwrap_or(0).min(input - cached);
    let reasoning = obj.get("reasoning_output_tokens").and_then(json_int_strict).unwrap_or(0).min(output);
    Some(Usage { input, cached, write, output, reasoning })
}

fn non_empty_str(value: Option<&serde_json::Value>) -> Option<String> {
    value?.as_str().map(str::trim).filter(|s| !s.is_empty()).map(ToOwned::to_owned)
}

fn parse_timestamp_ms(value: Option<&serde_json::Value>) -> Option<i64> {
    let v = value?;
    if let Some(s) = v.as_str() {
        if let Ok(dt) = DateTime::parse_from_rfc3339(s.trim()) {
            return Some(dt.timestamp_millis());
        }
        return None;
    }
    // Numeric fallback: seconds vs milliseconds heuristic.
    let n = v.as_i64().or_else(|| v.as_u64().and_then(|n| if n <= i64::MAX as u64 { Some(n as i64) } else { None }))?;
    if n <= 0 {
        return None;
    }
    Some(if n > 10_000_000_000 { n } else { n * 1000 })
}

// --- File scanning ---------------------------------------------------------

/// One billable row before pricing.
struct RawRow {
    created_ms: i64,
    usage: Usage,
    model_raw: Option<String>,
    response_id: Option<String>,
}

/// Scan one transcript file. Returns (record rows, fallback token_count rows).
/// token_count rows are only billed by the caller when a file has no records.
fn scan_file(path: &Path) -> (Vec<RawRow>, Vec<RawRow>) {
    let Ok(file) = fs::File::open(path) else {
        return (Vec::new(), Vec::new());
    };
    let mut session_id: Option<String> = None;
    // Ledger session: first meta's payload.session_id, else its id. Fork
    // files carry the parent id here; record session_ids are checked
    // against this (NOT the file session id) or every fork record drops.
    let mut ledger_session: Option<String> = None;
    let mut current_model: Option<String> = None;
    let mut records: Vec<RawRow> = Vec::new();
    let mut token_counts: Vec<RawRow> = Vec::new();

    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {
            continue;
        };
        let bytes = line.as_bytes();
        // Byte prefilter: skip everything that cannot be billable or
        // model/identity carrying. The `latest_token_usage_record` guard
        // keeps giant `compacted` snapshot lines out of the JSON parser.
        let has_record = bytes.windows(19).any(|w| w == b"token_usage_record\"")
            && !bytes.windows(25).any(|w| w == b"latest_token_usage_record");
        let has_token_count = bytes.windows(11).any(|w| w == b"token_count");
        let has_turn_context = bytes.windows(12).any(|w| w == b"turn_context");
        let has_session_meta = bytes.windows(12).any(|w| w == b"session_meta");
        if !(has_record || has_token_count || has_turn_context || has_session_meta) {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue; // mid-write truncated tail or malformed line: skip
        };
        let payload = value.get("payload");
        match value.get("type").and_then(|v| v.as_str()) {
            Some("session_meta") => {
                if let Some(obj) = payload.and_then(|p| p.as_object()) {
                    let id = obj
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned);
                    let psid = obj
                        .get("session_id")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToOwned::to_owned);
                    if session_id.is_none() {
                        session_id = id.clone().or(psid.clone());
                        ledger_session = psid.clone().or(id);
                    } else if ledger_session.is_none() {
                        ledger_session = psid.or(id);
                    }
                }
            }
            Some("turn_context") => {
                if let Some(obj) = payload.and_then(|p| p.as_object()) {
                    let info = obj.get("info").and_then(|v| v.as_object());
                    for key in ["model", "model_name"] {
                        if let Some(m) = obj.get(key).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                            current_model = Some(m.to_owned());
                            break;
                        }
                        if let Some(m) = info.and_then(|i| i.get(key)).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                            current_model = Some(m.to_owned());
                            break;
                        }
                    }
                }
            }
            Some("token_usage_record") => {
                let Some(obj) = payload.and_then(|p| p.as_object()) else {
                    continue;
                };
                let created_ms = parse_timestamp_ms(value.get("timestamp"));
                let thread_id = obj.get("thread_id").and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
                let response_id = obj.get("response_id").and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
                if thread_id.is_empty() || response_id.is_empty() {
                    continue;
                }
                if let Some(sid) = session_id.as_deref() {
                    if thread_id != sid {
                        continue;
                    }
                }
                if let Some(psid) = obj.get("session_id").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()) {
                    // Ledger session (first meta's payload.session_id, else
                    // its id) — NOT the file session id. Fork records carry
                    // the parent id here.
                    if ledger_session.as_deref().is_some_and(|ls| psid != ls) {
                        continue;
                    }
                }
                let (Some(ms), Some(usage), Some(thread_total)) = (
                    created_ms,
                    obj.get("usage").and_then(parse_usage_strict),
                    obj.get("thread_token_usage").and_then(parse_usage_strict),
                ) else {
                    continue;
                };
                // thread total must cover the delta component-wise.
                if thread_total.input < usage.input || thread_total.output < usage.output {
                    continue;
                }
                records.push(RawRow {
                    created_ms: ms,
                    usage,
                    model_raw: current_model.clone().or_else(|| {
                        obj.get("model").and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(ToOwned::to_owned)
                    }),
                    response_id: Some(response_id.to_owned()),
                });
            }
            Some("event_msg") => {
                let Some(obj) = payload.and_then(|p| p.as_object()) else {
                    continue;
                };
                if obj.get("type").and_then(|v| v.as_str()) != Some("token_count") {
                    continue;
                }
                let (Some(ms), Some(info)) = (
                    parse_timestamp_ms(value.get("timestamp")),
                    obj.get("info").and_then(|v| v.as_object()),
                ) else {
                    continue;
                };
                let Some(usage) = info.get("last_token_usage").and_then(parse_usage_lenient) else {
                    continue;
                };
                if usage.input == 0 && usage.output == 0 {
                    continue;
                }
                let model = info
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        info.get("model_name")
                            .and_then(|v| v.as_str())
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToOwned::to_owned)
                    })
                    .or_else(|| current_model.clone());
                token_counts.push(RawRow { created_ms: ms, usage, model_raw: model, response_id: None });
            }
            _ => {}
        }
    }
    // token_count rows only bill in files with zero usage records.
    if !records.is_empty() {
        token_counts.clear();
    }
    let _ = non_empty_str;
    (records, token_counts)
}

fn collect_files(root: &Path, out: &mut Vec<PathBuf>) {
    // sessions/ recursive.
    let sessions = root.join("sessions");
    let mut stack = vec![sessions];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(path);
            }
        }
    }
    // archived_sessions/ flat.
    if let Ok(entries) = fs::read_dir(root.join("archived_sessions")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(path);
            }
        }
    }
}

fn codex_home(home: &Path) -> PathBuf {
    std::env::var("CODEX_HOME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"))
}

/// Scan the Codex history and aggregate a cost snapshot.
pub fn fetch_codex_cost(home: &Path) -> Result<CostSnapshot> {
    let root = codex_home(home);
    if !root.join("sessions").is_dir() && !root.join("archived_sessions").is_dir() {
        anyhow::bail!("Codex sessions directory not found under {}.", root.display());
    }
    let mut files = Vec::new();
    collect_files(&root, &mut files);
    if files.is_empty() {
        anyhow::bail!("No Codex session transcripts found under {}.", root.display());
    }

    // Parallelize across files with std threads (no extra deps).
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 64);
    let chunk = files.len().div_ceil(workers);
    let mut record_rows: Vec<RawRow> = Vec::new();
    let mut fallback_rows: Vec<RawRow> = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for part in files.chunks(chunk.max(1)) {
            handles.push(scope.spawn(move || {
                let mut recs = Vec::new();
                let mut fall = Vec::new();
                for path in part {
                    let (mut r, mut f) = scan_file(path);
                    recs.append(&mut r);
                    fall.append(&mut f);
                }
                (recs, fall)
            }));
        }
        for handle in handles {
            if let Ok((mut r, mut f)) = handle.join() {
                record_rows.append(&mut r);
                fallback_rows.append(&mut f);
            }
        }
    });

    let catalog = load_catalog(&catalog_path());

    // Global dedup by response_id (fork-copied rows cannot double-bill).
    let mut by_response: HashMap<String, RawRow> = HashMap::new();
    for row in record_rows {
        let key = row.response_id.clone().unwrap_or_default();
        match by_response.get(&key) {
            Some(prev) if prev.usage.input + prev.usage.output >= row.usage.input + row.usage.output => {}
            _ => {
                by_response.insert(key, row);
            }
        }
    }

    let mut cost_rows: Vec<CostRow> = Vec::with_capacity(by_response.len() + fallback_rows.len());
    for row in by_response.into_values() {
        let model_key = row.model_raw.as_deref().and_then(normalize_model_id);
        let cost = price_usage(&row.usage, model_key.as_deref(), row.created_ms, catalog.as_ref()).unwrap_or(0.0);
        cost_rows.push(CostRow {
            created_ms: row.created_ms,
            cost,
            tokens: row.usage.input + row.usage.output,
            model: Some(model_key.unwrap_or_else(|| "unknown".to_string())),
        });
    }
    for row in fallback_rows {
        let model_key = row.model_raw.as_deref().and_then(normalize_model_id);
        let cost = price_usage(&row.usage, model_key.as_deref(), row.created_ms, catalog.as_ref()).unwrap_or(0.0);
        cost_rows.push(CostRow {
            created_ms: row.created_ms,
            cost,
            tokens: row.usage.input + row.usage.output,
            model: Some(model_key.unwrap_or_else(|| "unknown".to_string())),
        });
    }
    if cost_rows.is_empty() {
        anyhow::bail!("No Codex token usage found under {}.", root.display());
    }
    let mut snapshot = snapshot_from_rows("codex", "local", &cost_rows, chrono::Utc::now());
    snapshot.provenance = Some("listPriceEstimate".to_string());
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_cost::round_money;
    use std::io::Write;

    fn usage(input: i64, cached: i64, write: i64, output: i64, reasoning: i64) -> Usage {
        Usage { input, cached, write, output, reasoning }
    }

    #[test]
    fn strict_usage_rejects_bad_shapes() {
        let good = serde_json::json!({"input_tokens": 100, "cached_input_tokens": 10, "output_tokens": 5, "reasoning_output_tokens": 2});
        assert!(parse_usage_strict(&good).is_some());
        // cached > input
        let bad = serde_json::json!({"input_tokens": 100, "cached_input_tokens": 101, "output_tokens": 5});
        assert!(parse_usage_strict(&bad).is_none());
        // reasoning > output
        let bad = serde_json::json!({"input_tokens": 100, "output_tokens": 5, "reasoning_output_tokens": 6});
        assert!(parse_usage_strict(&bad).is_none());
        // booleans and floats rejected
        let bad = serde_json::json!({"input_tokens": true, "output_tokens": 5});
        assert!(parse_usage_strict(&bad).is_none());
        let bad = serde_json::json!({"input_tokens": 10.5, "output_tokens": 5});
        assert!(parse_usage_strict(&bad).is_none());
        // negative rejected, missing required rejected
        let bad = serde_json::json!({"input_tokens": -1, "output_tokens": 5});
        assert!(parse_usage_strict(&bad).is_none());
        let bad = serde_json::json!({"output_tokens": 5});
        assert!(parse_usage_strict(&bad).is_none());
    }

    #[test]
    fn lenient_usage_clamps() {
        let v = serde_json::json!({"input_tokens": 100, "cached_input_tokens": 500, "output_tokens": 5, "reasoning_output_tokens": 50});
        assert_eq!(parse_usage_lenient(&v), Some(usage(100, 100, 0, 5, 5)));
    }

    #[test]
    fn normalize_model_aliases() {
        assert_eq!(normalize_model_id("gpt-5.6"), Some("gpt-5.6-sol".to_string()));
        assert_eq!(normalize_model_id("openai/gpt-5.6-terra"), Some("gpt-5.6-terra".to_string()));
        assert_eq!(normalize_model_id("gpt-reserve"), Some("gpt-5.6-luna".to_string()));
        assert_eq!(normalize_model_id("gpt-5.6-sol-2026-08-01"), Some("gpt-5.6-sol".to_string()));
        assert_eq!(normalize_model_id(""), None);
        assert_eq!(normalize_model_id("unknown"), None);
        assert_eq!(normalize_model_id("cline-pass/foo"), None);
        assert_eq!(normalize_model_id("opencode/bar"), Some("bar".to_string()));
    }

    #[test]
    fn worked_example_prices_sol_row() {
        // Pricing spec §5: 16468 in (9984 cached), 288 out at gpt-5.6-sol → 0.0356896.
        let u = usage(16468, 9984, 0, 288, 50);
        let cost = price_usage(&u, Some("gpt-5.6-sol"), 1_788_000_000_000, None).expect("priced");
        assert!((cost - 0.0356896).abs() < 1e-9, "got {cost}");
        assert_eq!(round_money(cost), 0.03569);
    }

    #[test]
    fn historical_override_predates_cutoff() {
        let u = usage(100_000, 0, 0, 0, 0);
        // Before 2026-08-21 sol was $5/M in.
        let old = price_usage(&u, Some("gpt-5.6-sol"), 1_787_000_000_000, None).expect("priced");
        assert!((old - 0.5).abs() < 1e-9, "got {old}");
        let new = price_usage(&u, Some("gpt-5.6-sol"), 1_788_000_000_000, None).expect("priced");
        assert!((new - 0.4).abs() < 1e-9, "got {new}");
    }

    #[test]
    fn long_context_flips_whole_request() {
        let u = usage(300_000, 0, 0, 0, 0);
        let cost = price_usage(&u, Some("gpt-5.6-sol"), 1_788_000_000_000, None).expect("priced");
        assert!((cost - 300_000.0 * 8.0 / 1_000_000.0).abs() < 1e-9, "got {cost}");
    }

    #[test]
    fn unknown_model_is_unpriced() {
        let u = usage(1000, 0, 0, 100, 0);
        assert_eq!(price_usage(&u, Some("gpt-7-zzz"), 1_788_000_000_000, None), None);
        assert_eq!(price_usage(&u, None, 1_788_000_000_000, None), None);
    }

    #[test]
    fn autoreview_is_plan_included() {
        let u = usage(1000, 0, 0, 100, 0);
        assert_eq!(price_usage(&u, Some("codex-auto-review"), AUTOREVIEW_FREE_SINCE_MS, None), Some(0.0));
    }

    fn write_session(dir: &Path, name: &str, lines: &[&str]) -> PathBuf {
        let sessions = dir.join("sessions").join("2026").join("09").join("22");
        fs::create_dir_all(&sessions).expect("mkdir");
        let path = sessions.join(name);
        let mut f = fs::File::create(&path).expect("create");
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        path
    }

    fn record_line(ordinal: u32, response: &str, input: i64, output: i64) -> String {
        format!(
            r#"{{"timestamp":"2026-09-22T06:20:15.000Z","ordinal":{ordinal},"type":"token_usage_record","payload":{{"thread_id":"t1","response_id":"{response}","usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":{output}}},"thread_token_usage":{{"input_tokens":{input},"output_tokens":{output}}}}}}}"#
        )
    }

    #[test]
    fn scan_file_bills_records_with_turn_model() {
        let dir = std::env::temp_dir().join(format!("codexbar-codex-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_session(&dir, "a.jsonl", &[
            r#"{"timestamp":"2026-09-22T06:20:07.000Z","ordinal":0,"type":"session_meta","payload":{"id":"t1"}}"#,
            r#"{"timestamp":"2026-09-22T06:20:11.000Z","ordinal":1,"type":"turn_context","payload":{"turn_id":"u1","model":"gpt-5.6-sol"}}"#,
            &record_line(2, "resp_a", 1000, 100),
            &record_line(3, "resp_a", 2000, 200),
        ]);
        let (records, fallback) = scan_file(&dir.join("sessions").join("2026").join("09").join("22").join("a.jsonl"));
        let _ = fs::remove_dir_all(&dir);
        assert!(fallback.is_empty());
        // Same response twice in one file: caller dedups globally; both rows surface here.
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].model_raw.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(records[0].created_ms, 1790058015000);
    }

    #[test]
    fn scan_file_token_count_only_exec_style() {
        let dir = std::env::temp_dir().join(format!("codexbar-codex-tc-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_session(&dir, "e.jsonl", &[
            r#"{"timestamp":"2026-09-22T06:20:07.000Z","ordinal":0,"type":"session_meta","payload":{"id":"t9"}}"#,
            r#"{"timestamp":"2026-09-22T06:20:11.000Z","ordinal":1,"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":500,"output_tokens":50},"total_token_usage":{"input_tokens":500,"output_tokens":50}}}}"#,
        ]);
        let (records, fallback) = scan_file(&dir.join("sessions").join("2026").join("09").join("22").join("e.jsonl"));
        let _ = fs::remove_dir_all(&dir);
        assert!(records.is_empty());
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].usage.input, 500);
    }

    #[test]
    fn scan_file_accepts_fork_parent_session_id() {
        // Fork files carry the PARENT id in payload.session_id while the
        // file session (first meta id) equals the fork thread id. Records
        // must be kept (checked against the ledger session, not the file id).
        let dir = std::env::temp_dir().join(format!("codexbar-codex-fork-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_session(&dir, "f.jsonl", &[
            r#"{"timestamp":"2026-09-22T06:20:07.000Z","ordinal":0,"type":"session_meta","payload":{"id":"fork-thread","session_id":"parent-session"}}"#,
            r#"{"timestamp":"2026-09-22T06:20:15.000Z","ordinal":1,"type":"token_usage_record","payload":{"thread_id":"fork-thread","session_id":"parent-session","response_id":"resp_f","usage":{"input_tokens":100,"output_tokens":10},"thread_token_usage":{"input_tokens":100,"output_tokens":10}}}"#,
        ]);
        let path = dir.join("sessions").join("2026").join("09").join("22").join("f.jsonl");
        let (records, fallback) = scan_file(&path);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(records.len(), 1);
        assert!(fallback.is_empty());
    }

    #[test]
    fn scan_file_rejects_thread_mismatch() {
        let dir = std::env::temp_dir().join(format!("codexbar-codex-mm-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_session(&dir, "m.jsonl", &[
            r#"{"timestamp":"2026-09-22T06:20:07.000Z","ordinal":0,"type":"session_meta","payload":{"id":"t1"}}"#,
            &record_line(1, "resp_x", 100, 10).replace("\"thread_id\":\"t1\"", "\"thread_id\":\"other\""),
        ]);
        let (records, _) = scan_file(&dir.join("sessions").join("2026").join("09").join("22").join("m.jsonl"));
        let _ = fs::remove_dir_all(&dir);
        assert!(records.is_empty());
    }

    #[test]
    fn fetch_codex_cost_missing_home_errors() {
        let dir = std::env::temp_dir().join(format!("codexbar-codex-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        assert!(fetch_codex_cost(&dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn catalog_load_rejects_bad_version() {
        let dir = std::env::temp_dir().join(format!("codexbar-codex-cat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("models-dev-v1.json");
        fs::write(&path, r#"{"version": 999, "catalog": {"providers": {}}}"#).expect("write");
        assert!(load_catalog(&path).is_none());
        fs::write(
            &path,
            r#"{"version": 1, "catalog": {"providers": {"openai": {"models": {"my-model": {"cost": {"input": 2.0, "output": 3.0}}}}}}}"#,
        )
        .expect("write");
        let cat = load_catalog(&path).expect("catalog");
        assert!((cat["my-model"].input_per_m - 2.0).abs() < 1e-12);
        let _ = fs::remove_dir_all(&dir);
    }
}
