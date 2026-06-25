//! Persistent overrides for artist photos and album covers.
//!
//! The right-click menu on each artwork lets the user pick a local
//! image to replace what Qobuz serves. The replacement is kept in a
//! small JSON store at `<data-dir>/qbz/custom_artwork.json`, keyed by
//! artist name and album id. Two pairs of helpers mirror the
//! `v2_library_set/remove_custom_*` Tauri commands.
//!
//! The store records the absolute file path of the picked image. The
//! decode/render path that actually shows the override lives in
//! `crate::artwork`; this module just owns the persistence layer.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    artists: HashMap<String, String>,
    #[serde(default)]
    albums: HashMap<String, String>,
}

fn store_path() -> Option<PathBuf> {
    Some(
        dirs::data_dir()?
            .join("qbz")
            .join("custom_artwork.json"),
    )
}

fn load_store() -> Store {
    let Some(path) = store_path() else {
        return Store::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

fn write_store(store: &Store) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("[qbz-slint] custom-artwork dir failed: {e}");
            return;
        }
    }
    match serde_json::to_vec_pretty(store) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[qbz-slint] custom-artwork write failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-slint] custom-artwork serialize failed: {e}"),
    }
}

/// Returns the absolute file path of the user-picked artist image,
/// if one is registered. Called by the artist controller to swap the
/// header portrait + flip the menu's "Add" / "Change" / "Remove"
/// rendering.
#[allow(dead_code)] // wired by artist controller
pub fn artist_image(name: &str) -> Option<String> {
    load_store().artists.get(name).cloned()
}

#[allow(dead_code)] // wired by artist controller
pub fn set_artist_image(name: &str, path: &str) {
    let mut store = load_store();
    store.artists.insert(name.to_string(), path.to_string());
    write_store(&store);
}

#[allow(dead_code)] // wired by artist controller
pub fn remove_artist_image(name: &str) {
    let mut store = load_store();
    if store.artists.remove(name).is_some() {
        write_store(&store);
    }
}

/// Returns the absolute file path of the user-picked album cover,
/// if one is registered. Called by the album controller.
#[allow(dead_code)] // wired by album controller
pub fn album_cover(album_id: &str) -> Option<String> {
    load_store().albums.get(album_id).cloned()
}

#[allow(dead_code)] // wired by album controller
pub fn set_album_cover(album_id: &str, path: &str) {
    let mut store = load_store();
    store.albums.insert(album_id.to_string(), path.to_string());
    write_store(&store);
}

#[allow(dead_code)] // wired by album controller
pub fn remove_album_cover(album_id: &str) {
    let mut store = load_store();
    if store.albums.remove(album_id).is_some() {
        write_store(&store);
    }
}
