//! Persistence for the artist page's per-section release sort, keyed by
//! release_type (album / epSingle / live / …) so a chosen sort sticks
//! across artists and restarts. Small json under
//! `<data-dir>/qbz/artist_ui.json`. Mirrors `favorites_prefs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Prefs {
    /// release_type -> sort (default | newest | oldest | title-asc | title-desc).
    #[serde(default)]
    section_sort: HashMap<String, String>,
}

thread_local! {
    // Lazily-loaded, process-local cache of the prefs file.
    static CACHE: RefCell<Option<Prefs>> = const { RefCell::new(None) };
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("artist_ui.json"))
}

fn read_from_disk() -> Prefs {
    let Some(path) = store_path() else {
        return Prefs::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Prefs::default(),
    }
}

fn with_cache<R>(f: impl FnOnce(&mut Prefs) -> R) -> R {
    CACHE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(read_from_disk());
        }
        f(slot.as_mut().expect("prefs loaded above"))
    })
}

/// Persisted sort for a release bucket, or "default" when unset.
pub fn get_sort(release_type: &str) -> String {
    with_cache(|p| {
        p.section_sort
            .get(release_type)
            .cloned()
            .unwrap_or_else(|| "default".to_string())
    })
}

/// Persist a release bucket's sort. "default" removes the entry to keep the
/// file small. Writes through to disk.
pub fn set_sort(release_type: &str, sort: &str) {
    with_cache(|p| {
        if sort == "default" {
            p.section_sort.remove(release_type);
        } else {
            p.section_sort.insert(release_type.to_string(), sort.to_string());
        }
    });
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = with_cache(|p| serde_json::to_vec_pretty(p).ok());
    if let Some(bytes) = json {
        let _ = std::fs::write(&path, bytes);
    }
}
