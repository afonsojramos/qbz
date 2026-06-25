//! Recently-played store.
//!
//! A small JSON file at the shared QBZ data path holding the last few
//! played tracks, newest first. Discover Home renders two sections from
//! it — recently-played tracks (slim cards) and recently-played albums
//! (derived by de-duplicating the track history by album). The playback
//! session calls [`record`] when a track starts.
//!
//! Until playback is wired the store is simply empty and the Home
//! sections that read it hide themselves — the data path exists end to
//! end so playback only has to call `record`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

/// How many recent tracks to keep.
const MAX_RECENT: usize = 24;

/// Album-level metadata captured when an album is fetched for playback,
/// keyed by album id. The queue track itself (a `qbz_models::QueueTrack`)
/// carries no genre / release-date and — for the `album/get` path — no
/// per-track quality, so `record_recent` looks the album up here to stamp
/// the Recently Played card with genre, release date, and quality badge.
/// Matches Tauri's `album_to_card_meta`, which reads these off the `Album`.
#[derive(Clone, Debug, Default)]
pub struct AlbumMeta {
    pub genre: String,
    /// Raw ISO release date (e.g. "2021-05-14"); localized at render time.
    pub release_date: String,
    /// "hires" | "cd" | "" — drives the album card quality badge.
    pub quality_tier: String,
    /// "Hi-Res: 24-bit / 96 kHz" — quality badge hover tooltip.
    pub quality_label: String,
}

static ALBUM_META: LazyLock<Mutex<HashMap<String, AlbumMeta>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Cache album-level metadata for `album_id`, so a subsequent play of any
/// of its tracks records genre / release date / quality on the recent card.
/// Called from the playback album-fetch paths.
pub fn remember_album_meta(album_id: &str, meta: AlbumMeta) {
    if album_id.is_empty() {
        return;
    }
    if let Ok(mut map) = ALBUM_META.lock() {
        map.insert(album_id.to_string(), meta);
    }
}

/// Look up cached album-level metadata for `album_id` (if any).
pub fn album_meta(album_id: &str) -> Option<AlbumMeta> {
    ALBUM_META.lock().ok().and_then(|map| map.get(album_id).cloned())
}

/// One recently-played track, with the album it belongs to and enough
/// context (quality, ids) that re-playing it or rendering its album
/// card does not depend on a re-fetch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentTrack {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork_url: String,
    #[serde(default)]
    pub album_id: String,
    #[serde(default)]
    pub album_title: String,
    #[serde(default)]
    pub album_artist: String,
    #[serde(default)]
    pub album_artwork_url: String,
    /// "hires" | "cd" | "" — drives the album card quality badge.
    #[serde(default)]
    pub quality_tier: String,
    /// "Hi-Res: 24-bit / 96 kHz" — quality badge hover tooltip.
    #[serde(default)]
    pub quality_label: String,
    /// Album genre, for the Recently Played album card overlay. Empty for
    /// entries recorded before genre capture (serde default).
    #[serde(default)]
    pub genre: String,
    /// Raw ISO album release date, localized to "MMM D, YYYY" at render.
    /// Empty for entries recorded before release-date capture.
    #[serde(default)]
    pub release_date: String,
    /// Artist id for navigation / scrobble context.
    #[serde(default)]
    pub artist_id: Option<u64>,
    /// Origin: "qobuz" | "plex" | "local". Drives source-aware artwork
    /// (PlexThumb / local file) and routing for the Recently Played cards.
    /// Empty for pre-source entries (serde default) → treated as "qobuz".
    #[serde(default)]
    pub source: String,
}

/// One recently-played album, derived from the track history.
#[derive(Clone, Debug)]
pub struct RecentAlbum {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artwork_url: String,
    pub quality_tier: String,
    pub quality_label: String,
    pub genre: String,
    /// Raw ISO release date; localized at render time.
    pub release_date: String,
    /// "qobuz" | "plex" | "local" — see RecentTrack::source.
    pub source: String,
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

/// Recently-played albums, newest first, de-duplicated from the track
/// history. A track with no album id is skipped.
pub fn load_albums() -> Vec<RecentAlbum> {
    let mut albums: Vec<RecentAlbum> = Vec::new();
    for track in load() {
        if track.album_id.is_empty() || albums.iter().any(|a| a.id == track.album_id) {
            continue;
        }
        albums.push(RecentAlbum {
            id: track.album_id,
            title: track.album_title,
            artist: track.album_artist,
            artwork_url: track.album_artwork_url,
            quality_tier: track.quality_tier,
            quality_label: track.quality_label,
            genre: track.genre,
            release_date: track.release_date,
            source: track.source,
        });
    }
    albums
}

/// Remove every recently-played entry whose `album_id` is in `album_ids`.
/// Used when a Local Library folder is deleted so its albums/tracks no longer
/// linger in Recently Played. Returns how many track entries were removed.
pub fn prune_albums(album_ids: &[String]) -> usize {
    if album_ids.is_empty() {
        return 0;
    }
    let Some(path) = store_path() else {
        return 0;
    };
    let mut list = load();
    let before = list.len();
    list.retain(|t| !album_ids.iter().any(|k| k == &t.album_id));
    let removed = before - list.len();
    if removed > 0 {
        if let Ok(json) = serde_json::to_vec_pretty(&list) {
            let _ = std::fs::write(&path, json);
        }
    }
    removed
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
