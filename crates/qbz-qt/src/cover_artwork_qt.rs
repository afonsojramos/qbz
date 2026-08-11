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
pub fn add_custom_cover(album_id: String) {
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
