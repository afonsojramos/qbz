//! Persistence for the artist network sidebar open/closed preference.
//!
//! A tiny JSON file at the shared QBZ data path so the sidebar reopens
//! in the same state the user left it. Loaded on demand; written on
//! every toggle. Failures degrade silently — at worst the sidebar
//! starts closed on the next launch.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Prefs {
    #[serde(default)]
    open: bool,
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("network_sidebar.json"))
}

/// Read the persisted open/closed flag. Returns `false` (closed) when
/// the file does not exist or cannot be read.
#[allow(dead_code)] // wired by the artist controller in phase 4 (sidebar layout)
pub fn load_open() -> bool {
    let Some(path) = store_path() else {
        return false;
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Prefs>(&bytes)
            .map(|p| p.open)
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Persist the open/closed flag. Called on every sidebar toggle.
#[allow(dead_code)] // wired by the artist controller in phase 3 (network button)
pub fn set_open(open: bool) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("[qbz-slint] network-sidebar prefs dir failed: {e}");
            return;
        }
    }
    let prefs = Prefs { open };
    match serde_json::to_vec_pretty(&prefs) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[qbz-slint] network-sidebar prefs write failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-slint] network-sidebar prefs serialize failed: {e}"),
    }
}
