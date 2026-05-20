//! Persistence for the artist network sidebar open/closed preference.
//!
//! A tiny JSON file at the shared QBZ data path so the sidebar reopens
//! in the same state the user left it. Loaded on demand; written on
//! every toggle. Failures degrade silently — at worst the sidebar
//! starts closed on the next launch.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Prefs {
    #[serde(default = "default_open")]
    open: bool,
}

fn default_open() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Self { open: true }
    }
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("network_sidebar.json"))
}

/// Read the persisted open/closed flag. Defaults to `true` (open) on a
/// fresh install — the artist network sidebar is one of the visible
/// improvements over a generic Linux music player, so it should be on
/// out of the box until the user closes it.
pub fn load_open() -> bool {
    let Some(path) = store_path() else {
        return true;
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Prefs>(&bytes)
            .map(|p| p.open)
            .unwrap_or(true),
        Err(_) => true,
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
