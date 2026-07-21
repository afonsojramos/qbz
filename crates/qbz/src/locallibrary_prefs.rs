//! Persistence for the LocalLibrary toolbar choices (Tracks group mode + sort)
//! across restarts. A small json under `<data-dir>/qbz/locallibrary_ui.json`,
//! mirroring `favorites_prefs.rs`. Search queries are transient, not persisted.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use slint::ComponentHandle;

use crate::{AppWindow, LocalLibraryState};

#[derive(Serialize, Deserialize)]
struct Prefs {
    #[serde(default = "d_off")]
    tracks_group: String,
    #[serde(default = "d_default")]
    tracks_sort: String,
    // Album identity mode ("folder" | "metadata") — what one album IS on the
    // Albums tab. Default "folder": compilations/box sets stay whole (the
    // metadata split is opt-in via the header dropdown or Settings).
    #[serde(default = "d_folder")]
    albums_id_mode: String,
    // Path of the last-opened ephemeral folder, re-scanned on startup. None when
    // no ephemeral session is active.
    #[serde(default)]
    ephemeral_folder: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            tracks_group: d_off(),
            tracks_sort: d_default(),
            albums_id_mode: d_folder(),
            ephemeral_folder: None,
        }
    }
}

fn d_off() -> String {
    "off".to_string()
}

fn d_default() -> String {
    "default".to_string()
}

fn d_folder() -> String {
    "folder".to_string()
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("locallibrary_ui.json"))
}

fn read() -> Prefs {
    let Some(path) = store_path() else {
        return Prefs::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Prefs::default(),
    }
}

/// Apply persisted toolbar choices to LocalLibraryState. UI thread.
pub fn load(window: &AppWindow) {
    let p = read();
    let s = window.global::<LocalLibraryState>();
    s.set_tracks_group_mode(p.tracks_group.into());
    s.set_tracks_sort(p.tracks_sort.into());
    s.set_albums_id_mode(p.albums_id_mode.into());
}

fn write(p: &Prefs) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(p) {
        let _ = std::fs::write(&path, json);
    }
}

/// Persist the current toolbar choices read from LocalLibraryState. Preserves
/// the ephemeral-folder path (read-modify-write).
pub fn save(window: &AppWindow) {
    let mut p = read();
    let s = window.global::<LocalLibraryState>();
    p.tracks_group = s.get_tracks_group_mode().into();
    p.tracks_sort = s.get_tracks_sort().into();
    p.albums_id_mode = s.get_albums_id_mode().into();
    write(&p);
}

/// The persisted ephemeral-folder path, if any (rehydrated on startup).
pub fn ephemeral_path() -> Option<String> {
    read().ephemeral_folder.filter(|s| !s.is_empty())
}

/// Persist (or clear) the ephemeral-folder path (read-modify-write).
pub fn save_ephemeral_path(path: Option<&str>) {
    let mut p = read();
    p.ephemeral_folder = path.map(|s| s.to_string());
    write(&p);
}
