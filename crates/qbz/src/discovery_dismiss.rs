//! Per-tag discovery dismiss store.
//!
//! The "You may also like" sidebar section lets the user thumbs-down a
//! candidate. The rejection is sticky for that genre tag, so the same
//! tag never re-suggests the artist on future runs. A single JSON file
//! at the shared QBZ data path holds the dismissals — small enough for
//! a full read on each discovery call, no SQLite needed at this layer.
//!
//! Names are stored normalized (trim + lowercase + collapse whitespace)
//! so casing/whitespace differences from MB and Qobuz collide on the
//! same key.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Map of normalized-lowercased tag -> list of normalized artist names
/// the user has dismissed for that tag.
#[derive(Default, Serialize, Deserialize)]
struct DismissStore {
    #[serde(flatten)]
    by_tag: HashMap<String, Vec<String>>,
}

fn store_path() -> Option<PathBuf> {
    Some(
        dirs::data_dir()?
            .join("qbz")
            .join("discovery_dismiss.json"),
    )
}

fn load_store() -> DismissStore {
    let Some(path) = store_path() else {
        return DismissStore::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => DismissStore::default(),
    }
}

fn write_store(store: &DismissStore) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("[qbz-slint] discovery-dismiss dir failed: {e}");
            return;
        }
    }
    match serde_json::to_vec_pretty(store) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[qbz-slint] discovery-dismiss write failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-slint] discovery-dismiss serialize failed: {e}"),
    }
}

/// Return the set of dismissed normalized names for `tag` (which is
/// expected lowercase). Used as a filter input by the discovery
/// pipeline.
pub fn dismissed_for_tag(tag: &str) -> HashSet<String> {
    let store = load_store();
    store
        .by_tag
        .get(tag)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// Record a dismissal. The caller is expected to pass the tag already
/// lowercased and the name already normalized via
/// `qbz_core::normalize_artist_name`.
pub fn dismiss(tag: &str, normalized_name: &str) {
    let mut store = load_store();
    let entry = store.by_tag.entry(tag.to_string()).or_default();
    if !entry.iter().any(|n| n == normalized_name) {
        entry.push(normalized_name.to_string());
    }
    write_store(&store);
}
