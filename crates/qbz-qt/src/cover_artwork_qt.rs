//! Album custom covers + cover file actions — the Qt port of the album half
//! of `crates/qbz/src/custom_artwork.rs` and the cover-menu Rust arms
//! (`main.rs:23167-23239` add/remove, `:23096-23159` open-in-browser/save-as).
//!
//! The store is the SAME file and format the Slint app uses
//! (`<data-dir>/qbz/custom_artwork.json`, `albums: album_id → absolute
//! path`), so a cover set in either app shows in both. The store records the
//! picked file's own path (Slint does not copy/resize into the cache); if
//! the user later moves or deletes that file the override silently stops
//! applying — the reference's accepted behaviour, kept on purpose.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
struct Store {
    // Artist images share this file in the Slint store; the key must round-
    // trip even though this port only writes album covers.
    #[serde(default)]
    artists: HashMap<String, String>,
    #[serde(default)]
    albums: HashMap<String, String>,
}

fn store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("custom_artwork.json"))
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
            log::warn!("[qbz-qt] custom-artwork dir failed: {e}");
            return;
        }
    }
    match serde_json::to_vec_pretty(store) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("[qbz-qt] custom-artwork write failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-qt] custom-artwork serialize failed: {e}"),
    }
}

/// The absolute path of the user-picked cover for `album_id`, if registered.
/// `album_qt::load_album` calls this on every build to seed the header's
/// `hasCustomCover` / `customCoverPath`.
pub fn album_cover(album_id: &str) -> Option<String> {
    load_store().albums.get(album_id).cloned()
}

/// The art URL a card/collage builder should emit for an album: the custom
/// override's absolute path when one is registered AND the file still
/// exists, else the remote URL unchanged. The path flows through the
/// ordinary artwork pipeline (artwork_qt::classify's LocalFile arm resolves
/// it synchronously), so every surface that mounts AlbumCard picks the
/// override up with no QML change.
pub fn prefer_album_cover(album_id: &str, fallback_url: String) -> String {
    match album_cover(album_id) {
        Some(p) if std::path::Path::new(&p).is_file() => p,
        _ => fallback_url,
    }
}

fn set_album_cover(album_id: &str, path: &str) {
    let mut store = load_store();
    store.albums.insert(album_id.to_string(), path.to_string());
    write_store(&store);
}

fn remove_album_cover(album_id: &str) {
    let mut store = load_store();
    if store.albums.remove(album_id).is_some() {
        write_store(&store);
    }
}

/// "Add cover" / "Change cover": native picker, persist the picked path,
/// then re-open the album so the header repaints from the override.
/// Cancel = no-op, no toast (the picker's own affordance).
pub fn add_custom_cover(album_id: String, artwork_url: String) {
    if album_id.is_empty() {
        return;
    }
    crate::spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
            .await
        else {
            return;
        };
        let path = file.path().to_string_lossy().to_string();
        // Blocking IO, but tiny (read + write of a small JSON map); the spawn
        // keeps it off the Qt thread, same as the reference's handler.
        let id = album_id.clone();
        // The hash -> override link is what lets the rest of the app (cards,
        // NPB, queue, mosaics) render this art for this album.
        note_override_key(&artwork_url, &path);
        if tokio::task::spawn_blocking(move || {
            set_album_cover(&id, &path);
        })
        .await
        .is_ok()
        {
            crate::open_album(album_id);
        }
    });
}

/// "Remove cover": drop the override and re-open so the header reverts to
/// the Qobuz artwork.
pub fn remove_custom_cover(album_id: String) {
    if album_id.is_empty() {
        return;
    }
    let id = album_id.clone();
    crate::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || remove_album_cover(&id)).await;
        crate::open_album(album_id);
    });
}

/// "Save as…": native save dialog seeded `{title} - Cover.jpg`. The source
/// is the custom override when one is set, else the pipeline's cached file
/// for the header URL, else one GET of the URL (Slint `main.rs:23108-23159`).
pub fn save_cover_as(album_id: String, title: String, artwork_url: String) {
    crate::spawn(async move {
        let default_name = format!(
            "{} - Cover.jpg",
            if title.is_empty() { "album" } else { &title }
        );
        let Some(dest) = rfd::AsyncFileDialog::new()
            .set_file_name(&default_name)
            .add_filter("JPEG", &["jpg", "jpeg"])
            .save_file()
            .await
        else {
            return;
        };
        let dest_path = dest.path().to_path_buf();

        // Local sources first: the override file, then the artwork cache.
        let local = album_cover(&album_id).filter(|p| !p.is_empty()).or_else(|| {
            let cached = crate::artwork_qt::cached_path(&artwork_url);
            if cached.is_empty() {
                None
            } else {
                Some(cached)
            }
        });
        if let Some(src) = local {
            if let Err(e) = tokio::fs::copy(&src, &dest_path).await {
                log::warn!("[qbz-qt] cover save-as copy failed: {e}");
            }
            return;
        }
        if artwork_url.is_empty() {
            return;
        }
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                log::warn!("[qbz-qt] cover save-as client error: {e}");
                return;
            }
        };
        match client.get(&artwork_url).send().await.and_then(|r| r.error_for_status()) {
            Ok(resp) => match resp.bytes().await {
                Ok(bytes) => {
                    if let Err(e) = tokio::fs::write(&dest_path, &bytes).await {
                        log::warn!("[qbz-qt] cover save-as write failed: {e}");
                    }
                }
                Err(e) => log::warn!("[qbz-qt] cover save-as read failed: {e}"),
            },
            Err(e) => log::warn!("[qbz-qt] cover save-as fetch failed: {e}"),
        }
    });
}

// ---------------------------------------------------------------------------
// Playlist custom covers (Qt-first; no Slint counterpart yet)
// ---------------------------------------------------------------------------
// Separate file on purpose: the Slint store struct round-trips only the keys
// it knows, so a `playlists` key inside custom_artwork.json would be DROPPED
// the next time the Slint app writes an album/artist override. A Qt-owned
// file cannot be clobbered by it.

#[derive(Default, Serialize, Deserialize)]
struct PlaylistStore {
    #[serde(default)]
    playlists: HashMap<String, String>,
}

fn playlist_store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("custom_playlist_covers.json"))
}

fn load_playlist_store() -> PlaylistStore {
    let Some(path) = playlist_store_path() else {
        return PlaylistStore::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => PlaylistStore::default(),
    }
}

fn write_playlist_store(store: &PlaylistStore) {
    let Some(path) = playlist_store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(store) {
        let _ = std::fs::write(&path, json);
    }
}

/// The absolute path of the user-picked cover for `playlist_id`, if set.
pub fn playlist_cover(playlist_id: &str) -> Option<String> {
    load_playlist_store().playlists.get(playlist_id).cloned()
}

/// Playlist twin of [`prefer_album_cover`].
pub fn prefer_playlist_cover(playlist_id: &str, fallback_url: String) -> String {
    match playlist_cover(playlist_id) {
        Some(p) if std::path::Path::new(&p).is_file() => p,
        _ => fallback_url,
    }
}

/// Playlist header "Add/Change cover": native picker, persist, re-open the
/// playlist so every surface repaints from the override.
pub fn add_custom_playlist_cover(playlist_id: String) {
    if playlist_id.is_empty() {
        return;
    }
    crate::spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
            .await
        else {
            return;
        };
        let path = file.path().to_string_lossy().to_string();
        let id = playlist_id.clone();
        if tokio::task::spawn_blocking(move || {
            let mut store = load_playlist_store();
            store.playlists.insert(id, path);
            write_playlist_store(&store);
        })
        .await
        .is_ok()
        {
            crate::open_playlist(playlist_id);
        }
    });
}

/// "Remove cover": drop the override and re-open so the header reverts to
/// the own-art/mosaic rule.
pub fn remove_custom_playlist_cover(playlist_id: String) {
    if playlist_id.is_empty() {
        return;
    }
    let id = playlist_id.clone();
    crate::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            let mut store = load_playlist_store();
            if store.playlists.remove(&id).is_some() {
                write_playlist_store(&store);
            }
        })
        .await;
        crate::open_playlist(playlist_id);
    });
}

// ---------------------------------------------------------------------------
// Propagation layer: artwork-hash -> override (the "everywhere" half)
// ---------------------------------------------------------------------------
// Album overrides are keyed by album id, but most of the app never sees the
// id — cards, the NPB, the queue and mosaics carry only the art URL. The
// bridge between the two is the Qobuz cover hash: every size variant of one
// artwork shares the stem (`.../covers/tr/9l/<hash>_<size>.jpg`), so mapping
// hash -> override path lets the ARTWORK layer answer with the override no
// matter which surface asked or which size it wanted.
//
// The map lives in a Qt-side file (custom_cover_keys.json) for the same
// reason the playlist store does: the Slint store round-trips only known
// keys and would drop an extension on its next write.

#[derive(Default, Serialize, Deserialize)]
struct KeyStore {
    #[serde(default)]
    keys: HashMap<String, String>,
}

fn key_store_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("custom_cover_keys.json"))
}

fn load_key_store() -> KeyStore {
    let Some(path) = key_store_path() else {
        return KeyStore::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => KeyStore::default(),
    }
}

fn write_key_store(store: &KeyStore) {
    let Some(path) = key_store_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(store) {
        let _ = std::fs::write(&path, json);
    }
}

/// The artwork hash of a Qobuz cover URL: the filename stem before the size
/// suffix (`<hash>_600.jpg` -> `<hash>`). None for non-cover URLs.
fn art_key(url: &str) -> Option<String> {
    let stem = url
        .rsplit('/')
        .next()
        .and_then(|f| f.rsplit_once('.').map(|(s, _)| s).or(Some(f)))
        .unwrap_or("");
    if stem.is_empty() {
        return None;
    }
    let (hash, _size) = stem.rsplit_once('_')?;
    if hash.is_empty() {
        None
    } else {
        Some(hash.to_string())
    }
}

/// Record `hash -> override path` for the album whose current art URL is
/// `artwork_url`. Called when a cover is set and on album-page load as a
/// backfill for covers set before this map existed.
pub fn note_override_key(artwork_url: &str, override_path: &str) {
    let Some(key) = art_key(artwork_url) else {
        return;
    };
    let mut store = load_key_store();
    if store.keys.get(&key).map(|s| s.as_str()) == Some(override_path) {
        return;
    }
    store.keys.insert(key, override_path.to_string());
    write_key_store(&store);
}

/// The override path for any art URL whose hash has one ("" when none or the
/// file is gone). `artwork_qt::cached_path` consults this BEFORE the network
/// cache, so every surface renders the custom art.
pub fn override_for_url(url: &str) -> String {
    let Some(key) = art_key(url) else {
        return String::new();
    };
    match load_key_store().keys.get(&key) {
        Some(p) if std::path::Path::new(p).is_file() => p.clone(),
        _ => String::new(),
    }
}
