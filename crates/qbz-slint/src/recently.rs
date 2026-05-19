//! Recently-played tracks store.
//!
//! A small JSON file at the shared QBZ data path holding the last few
//! played tracks, newest first. The Discover Home renders it as a slim
//! section; the playback session calls [`record`] when a track starts.
//!
//! Until playback is wired the store is simply empty, and the Home
//! section that reads it hides itself — the data path exists end to end
//! so playback only has to call `record`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// How many recent tracks to keep.
const MAX_RECENT: usize = 12;

/// One recently-played track.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentTrack {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork_url: String,
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("recently_played.json"))
}

/// Load the recently-played tracks, newest first. Returns an empty list
/// when the store does not exist yet or cannot be read.
pub fn load() -> Vec<RecentTrack> {
    let Some(path) = store_path() else {
        return Vec::new();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Record a played track at the front of the list. Deduplicates by id and
/// caps the list at [`MAX_RECENT`]. Called by the playback session when a
/// track starts.
#[allow(dead_code)] // wired by the playback session
pub fn record(track: RecentTrack) {
    let Some(path) = store_path() else {
        return;
    };
    let mut list = load();
    list.retain(|t| t.id != track.id);
    list.insert(0, track);
    list.truncate(MAX_RECENT);

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("[qbz-slint] recently-played store dir failed: {e}");
            return;
        }
    }
    match serde_json::to_vec_pretty(&list) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[qbz-slint] recently-played write failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-slint] recently-played serialize failed: {e}"),
    }
}
