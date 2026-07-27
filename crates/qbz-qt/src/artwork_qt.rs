//! Artwork pipeline for the Home view — a minimal port of the fetch side
//! of `crates/qbz/src/artwork.rs` onto `qbz_cache::ImageCacheService`.
//!
//! Flow: when home sections are built, each card whose `art_url` is
//! already on disk gets `artPath = file://<cached path>` synchronously;
//! the misses are downloaded on tokio tasks (semaphore cap 16, shared
//! reqwest client, 10s timeout — the artwork.rs constants), stored via
//! `ImageCacheService::store`, and the section list is then republished.
//!
//! POC-NOTE: on download completion the WHOLE section list is republished
//! (a `homeSectionsJson` swap) — per-row QAbstractListModel updates are
//! the documented follow-up. No RGBA decode in Rust: QML `Image` decodes
//! `file://` asynchronously and natively. Cache eviction (`evict`,
//! 200 MB cap in the Slint app) is not wired — POC scale.

use std::sync::{Arc, Mutex, OnceLock};

use qbz_cache::ImageCacheService;
use tokio::sync::Semaphore;

use crate::home_qt::HomeSection;

/// Matches artwork.rs `MAX_CONCURRENT`.
const MAX_CONCURRENT: usize = 16;
/// Matches artwork.rs `HTTP_TIMEOUT` (a hung request on a captive portal
/// must not pin a semaphore permit indefinitely).
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Shared client (artwork.rs: "instead of reqwest::get building a fresh
/// client + TLS state per image").
static HTTP: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .expect("reqwest client")
});

/// The process-wide image cache (`~/.cache/qbz/images`, shared with the
/// Slint/Tauri apps). `Connection` is not Sync, hence the Mutex; `get` /
/// `store` are short blocking calls, fine under it.
static CACHE: OnceLock<Mutex<ImageCacheService>> = OnceLock::new();

fn cache() -> Option<&'static Mutex<ImageCacheService>> {
    if CACHE.get().is_none() {
        match ImageCacheService::new() {
            Ok(service) => {
                let _ = CACHE.set(Mutex::new(service));
            }
            Err(e) => {
                log::error!("[qbz-qt] image cache open failed: {e}");
                return None;
            }
        }
    }
    CACHE.get()
}

/// Fill `art_path` from the disk cache for every card that already has one.
/// Returns the urls still missing (deduped, non-empty).
pub fn attach_cached(sections: &mut [HomeSection]) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    for section in sections.iter_mut() {
        for card in section.items.iter_mut() {
            if card.art_url.is_empty() {
                continue;
            }
            let hit = cache().and_then(|c| c.lock().unwrap().get(&card.art_url));
            if let Some(path) = hit {
                card.art_path = format!("file://{}", path.display());
            } else if !missing.contains(&card.art_url) {
                missing.push(card.art_url.clone());
            }
        }
    }
    missing
}

/// Download the missing urls concurrently (semaphore-capped), storing each
/// into the image cache. Returns when all downloads settled (failures are
/// logged and skipped — the cards keep their placeholder).
pub async fn download_missing(urls: Vec<String>) {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut handles = Vec::with_capacity(urls.len());
    for url in urls {
        let sem = Arc::clone(&semaphore);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore open");
            let bytes = match HTTP.get(&url).send().await {
                Ok(resp) => match resp.bytes().await {
                    Ok(b) => b,
                    Err(e) => {
                        log::warn!("[qbz-qt] artwork read failed ({url}): {e}");
                        return;
                    }
                },
                Err(e) => {
                    log::warn!("[qbz-qt] artwork download failed ({url}): {e}");
                    return;
                }
            };
            if let Some(cache) = cache() {
                if let Err(e) = cache.lock().unwrap().store(&url, &bytes) {
                    log::warn!("[qbz-qt] artwork store failed ({url}): {e}");
                }
            }
        }));
    }
    for handle in handles {
        let _ = handle.await;
    }
}

// ---------------------------------------------------------------------------
// Now-playing artwork (phase 4)
// ---------------------------------------------------------------------------

/// `file://<cached path>` when the url is already on disk ("" otherwise) —
/// synchronous, no download. Used for queue rows.
pub fn cached_path(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }
    cache()
        .and_then(|c| c.lock().unwrap().get(url))
        .map(|p| format!("file://{}", p.display()))
        .unwrap_or_default()
}

/// Attach the current track's artwork to the NowPlayingModel: disk hit
/// applies immediately; a miss downloads in the background and then
/// republishes (mirrors `attach_cached` + `download_missing` for one url).
pub fn attach_now_playing(url: &str) {
    if url.is_empty() {
        crate::now_playing::set_artwork_path(String::new());
        return;
    }
    let hit = cached_path(url);
    if !hit.is_empty() {
        crate::now_playing::set_artwork_path(hit);
        return;
    }
    let url = url.to_string();
    crate::spawn(async move {
        download_missing(vec![url.clone()]).await;
        let path = cached_path(&url);
        if !path.is_empty() {
            crate::now_playing::set_artwork_path(path);
        }
    });
}
