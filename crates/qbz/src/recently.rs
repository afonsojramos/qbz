//! Recently-played store.
//!
//! A small JSON file at the shared QBZ data path holding the last few
//! played tracks AND the last few played albums, newest first. Discover
//! Home renders two sections from it — recently-played tracks (slim
//! cards) and recently-played albums. The playback session calls
//! [`record`] when a track starts.
//!
//! The album history is a SEPARATE list with its own cap (#567): deriving
//! albums from the 24-track window collapsed long albums into ~4 distinct
//! album cards, starving the "Recently Played Albums" rail. Both lists are
//! deduplicated by id at record time. Persisted format is an object
//! `{ "tracks": [...], "albums": [...] }`; the legacy format (a bare track
//! array) is migrated on read by deriving the album list from the track
//! window exactly as before, so old stores lose nothing.
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

/// How many recent albums to keep — independent of the track cap, so a
/// string of long albums cannot shrink the distinct-album history (#567).
const MAX_RECENT_ALBUMS: usize = 24;

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

/// One recently-played album. Since #567 this is its OWN persisted history
/// (deduplicated by album id at record time, capped at [`MAX_RECENT_ALBUMS`]),
/// no longer derived from the 24-track window at read time. Every field takes
/// a serde default so entries written by older builds (or the legacy-derive
/// path) stay readable if fields are added later.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecentAlbum {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub artist: String,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub quality_tier: String,
    #[serde(default)]
    pub quality_label: String,
    #[serde(default)]
    pub genre: String,
    /// Raw ISO release date; localized at render time.
    #[serde(default)]
    pub release_date: String,
    /// "qobuz" | "plex" | "local" — see RecentTrack::source.
    #[serde(default)]
    pub source: String,
}

/// The persisted store shape since #567: the track history plus the separate
/// album history. The legacy shape (a bare `Vec<RecentTrack>` array) is
/// handled by [`read_store`]'s fallback branch.
#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentStore {
    #[serde(default)]
    tracks: Vec<RecentTrack>,
    #[serde(default)]
    albums: Vec<RecentAlbum>,
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("recently_played.json"))
}

/// Derive an album list from a track window by first-occurrence de-dup —
/// the pre-#567 behaviour, kept ONLY for the one-time legacy migration in
/// [`read_store`]. A track with no album id is skipped.
fn derive_albums(tracks: &[RecentTrack]) -> Vec<RecentAlbum> {
    let mut albums: Vec<RecentAlbum> = Vec::new();
    for track in tracks {
        if track.album_id.is_empty() || albums.iter().any(|a| a.id == track.album_id) {
            continue;
        }
        albums.push(RecentAlbum {
            id: track.album_id.clone(),
            title: track.album_title.clone(),
            artist: track.album_artist.clone(),
            artwork_url: track.album_artwork_url.clone(),
            quality_tier: track.quality_tier.clone(),
            quality_label: track.quality_label.clone(),
            genre: track.genre.clone(),
            release_date: track.release_date.clone(),
            source: track.source.clone(),
        });
    }
    albums
}

/// Read the whole store. Missing / unreadable file -> empty. A legacy bare
/// track array (pre-#567) migrates additively: the album list is derived from
/// the track window exactly as the old reader did; the next write persists
/// the new object shape.
fn read_store() -> RecentStore {
    let Some(path) = store_path() else {
        return RecentStore::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return RecentStore::default();
    };
    if let Ok(store) = serde_json::from_slice::<RecentStore>(&bytes) {
        return store;
    }
    let tracks: Vec<RecentTrack> = serde_json::from_slice(&bytes).unwrap_or_default();
    let albums = derive_albums(&tracks);
    RecentStore { tracks, albums }
}

/// Persist the whole store (pretty JSON, best-effort with logged warnings).
fn write_store(store: &RecentStore) {
    let Some(path) = store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("[qbz-slint] recently-played store dir failed: {e}");
            return;
        }
    }
    match serde_json::to_vec_pretty(store) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[qbz-slint] recently-played write failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-slint] recently-played serialize failed: {e}"),
    }
}

/// Load the recently-played tracks, newest first. Returns an empty list
/// when the store does not exist yet or cannot be read.
pub fn load() -> Vec<RecentTrack> {
    read_store().tracks
}

/// Recently-played albums, newest first, from the dedicated album history
/// (legacy stores derive it from the track window once — see [`read_store`]).
pub fn load_albums() -> Vec<RecentAlbum> {
    read_store().albums
}

/// Remove every recently-played entry whose `album_id` is in `album_ids`.
/// Used when a Local Library folder is deleted so its albums/tracks no longer
/// linger in Recently Played. Prunes BOTH the track and the album histories.
/// Returns how many track entries were removed.
pub fn prune_albums(album_ids: &[String]) -> usize {
    if album_ids.is_empty() {
        return 0;
    }
    let mut store = read_store();
    let tracks_before = store.tracks.len();
    let albums_before = store.albums.len();
    store.tracks.retain(|t| !album_ids.iter().any(|k| k == &t.album_id));
    store.albums.retain(|a| !album_ids.iter().any(|k| k == &a.id));
    let removed = tracks_before - store.tracks.len();
    if removed > 0 || albums_before != store.albums.len() {
        write_store(&store);
    }
    removed
}

/// Record a played track at the front of the track history (dedup by track
/// id, capped at [`MAX_RECENT`]) and its album at the front of the album
/// history (dedup by album id, capped at [`MAX_RECENT_ALBUMS`]). Called by
/// the playback session when a track starts.
#[allow(dead_code)] // wired by the playback session
pub fn record(track: RecentTrack) {
    let mut store = read_store();
    if !track.album_id.is_empty() {
        store.albums.retain(|a| a.id != track.album_id);
        store.albums.insert(
            0,
            RecentAlbum {
                id: track.album_id.clone(),
                title: track.album_title.clone(),
                artist: track.album_artist.clone(),
                artwork_url: track.album_artwork_url.clone(),
                quality_tier: track.quality_tier.clone(),
                quality_label: track.quality_label.clone(),
                genre: track.genre.clone(),
                release_date: track.release_date.clone(),
                source: track.source.clone(),
            },
        );
        store.albums.truncate(MAX_RECENT_ALBUMS);
    }
    store.tracks.retain(|t| t.id != track.id);
    store.tracks.insert(0, track);
    store.tracks.truncate(MAX_RECENT);
    write_store(&store);
}
