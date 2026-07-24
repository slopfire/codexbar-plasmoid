//! Local OpenCode install detection and database paths.
//!
//! Rate-limit windows for OpenCode Go **must not** be estimated from this SQLite
//! history. Messages billed under `providerID = opencode-go` are often pay-as-you-go
//! API usage with no $12/$30/$60 subscription plan. Mapping local spend onto those
//! limits produced fake full remaining bars for users without a Go sub.
//!
//! Subscription rolling/weekly/monthly percents come only from the live
//! opencode.ai plan API (see `providers/opencodego.rs`). Token spend for the
//! plasmoid cost footer comes from `local_cost.rs`.

use std::fs;
use std::path::{Path, PathBuf};

pub fn local_paths(home: &Path) -> (PathBuf, PathBuf) {
    let root = home.join(".local/share/opencode");
    (root.join("auth.json"), resolve_database_path(&root))
}

/// Resolve the OpenCode SQLite database under `root`.
///
/// The current app writes `opencode-stable.db`; older builds used `opencode.db`,
/// and alternate channels (e.g. nightly) write `opencode-<channel>.db`. Prefer
/// the stable name, then legacy, then any channel variant so detection survives
/// renames. Falls back to the stable name so error messages stay sensible.
fn resolve_database_path(root: &Path) -> PathBuf {
    let stable = root.join("opencode-stable.db");
    let legacy = root.join("opencode.db");
    if stable.exists() {
        return stable;
    }
    if legacy.exists() {
        return legacy;
    }
    if let Ok(entries) = fs::read_dir(root) {
        let mut variants: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
                    return false;
                };
                name.starts_with("opencode-")
                    && name.ends_with(".db")
                    && !name.ends_with("-shm")
                    && !name.ends_with("-wal")
            })
            .collect();
        if !variants.is_empty() {
            variants.sort();
            return variants.remove(0);
        }
    }
    stable
}

/// True when a local OpenCode database is present (used for messaging / cost).
pub fn can_read_local_usage(home: &Path) -> bool {
    let (_, database_path) = local_paths(home);
    database_path.exists()
}
