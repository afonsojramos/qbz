//! Artwork pipeline — SOURCE-AWARE resolution for every QBZ art source.
//!
//! Every surface (Home rails, search, playlists, the queue panel, the
//! now-playing bar, the ambient triad) hands this module an `art_url` and
//! gets back a `file://` path QML can decode. Those urls are NOT all HTTP,
//! and treating them as if they were is what left local and Plex covers
//! blank. The taxonomy is the same one the Slint frontend routes on
//! (`crates/qbz/src/queue.rs` `load_artwork`, `artwork.rs`
//! `fetch_and_decode_ref`):
//!
//! | shape                          | class              | resolution              |
//! |--------------------------------|--------------------|-------------------------|
//! | `http://` / `https://`         | [`ArtUrl::Http`]   | disk cache, else download|
//! | `file:///…`                    | [`ArtUrl::LocalFile`] | strip + stat, NO fetch |
//! | `/abs/path/cover.jpg`          | [`ArtUrl::LocalFile`] | stat, NO fetch        |
//! | `/library/…` `/photo/…`        | [`ArtUrl::Plex`]   | tokenize, then as HTTP  |
//! | Plex path, no server/token     | [`ArtUrl::PlexUnconfigured`] | unresolvable  |
//!
//! A `file://` url can never be handed to reqwest ("builder error for url"),
//! and a bare `/library/...` path has no scheme at all — both are resolved
//! WITHOUT the network here, and [`download_missing`] drops them instead of
//! trying to GET them.
//!
//! Plex credentials come from `local_plex` (the same store the Local Library
//! grid reads) and the thumb url is built by `local_plex::thumb_url`, so a
//! cover fetched for the Albums grid and the same cover in the queue share
//! ONE cache entry ([`PLEX_THUMB_PX`] is the single transcode size).
//!
//! ## Repeat visits resolve from RAM
//!
//! `ImageCacheService::get` is not a read: it stats the file AND runs an
//! `UPDATE … SET last_accessed` on the shared SQLite connection, behind the
//! process-wide cache mutex that the 16 concurrent downloaders also hold to
//! `store`. The window dispatches call it once per visible card, on the Qt
//! GUI thread, on EVERY window pass — so a second visit to a view queued
//! dozens of write transactions behind the downloaders instead of resolving
//! the already-on-disk covers straight away (the "grey squares that fill in
//! unevenly" report). [`RESOLVED`] memoizes url -> cached file path so a
//! repeat lookup is a read-lock + one stat, with no SQLite and no contention
//! with the download pool. Entries are dropped when the file is gone (another
//! process may evict the shared cache), so the memo can never serve a dead
//! path.
//!
//! POC-NOTE: on download completion the WHOLE section list is republished
//! (a `homeSectionsJson` swap) — per-row QAbstractListModel updates are
//! the documented follow-up. No RGBA decode in Rust: QML `Image` decodes
//! `file://` asynchronously and natively. Cache eviction (`evict`, 200 MB cap
//! in the Slint app) is still not wired here — POC scale.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, RwLock};

use qbz_cache::ImageCacheService;
use tokio::sync::Semaphore;

use crate::home_qt::HomeSection;

/// Matches artwork.rs `MAX_CONCURRENT`.
const MAX_CONCURRENT: usize = 16;
/// Matches artwork.rs `HTTP_TIMEOUT` (a hung request on a captive portal
/// must not pin a semaphore permit indefinitely).
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Server-side transcode size for EVERY Plex thumb this process fetches.
/// One size app-wide is deliberate: the cache key is the final tokenized url,
/// so a second size would mean a second download of the same cover. 256 px is
/// the Local Library grid's card size and is ample for the queue rows / the
/// now-playing bar / the sidebar dock, which all render well under it.
pub const PLEX_THUMB_PX: u32 = 256;

/// Memo ceiling. The memo is an accelerator, not a source of truth, so it is
/// cleared wholesale rather than carrying LRU bookkeeping.
const MEMO_CAP: usize = 8192;

/// Shared client (artwork.rs: "instead of reqwest::get building a fresh
/// client + TLS state per image").
static HTTP: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .expect("reqwest client")
});

/// The process-wide image cache (`~/.cache/qbz/images`, shared with the
/// Slint/Tauri apps). `Connection` is not Sync, hence the Mutex; `get` /
/// `store` are short blocking calls, fine under it.
static CACHE: OnceLock<Mutex<ImageCacheService>> = OnceLock::new();

/// Raw art url -> its resolved file in the disk cache. See the module docs:
/// this is what makes a repeat window pass synchronous.
static RESOLVED: LazyLock<RwLock<HashMap<String, PathBuf>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

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

// ---------------------------------------------------------------------------
// URL taxonomy
// ---------------------------------------------------------------------------

/// What an `art_url` actually is. See the table in the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtUrl {
    /// Nothing to resolve (empty, or a shape we do not understand).
    Empty,
    /// A fetchable http(s) url — Qobuz CDN covers, and Plex thumbs that have
    /// already been tokenized by [`classify`].
    Http(String),
    /// A file ALREADY ON DISK: the RAW filesystem path (no `file://`, no
    /// percent-encoding). Local Library covers, `qbz-library` thumbnails and
    /// offline-download covers. Never downloaded.
    LocalFile(String),
    /// A Plex server-relative thumb resolved against the configured server:
    /// the tokenized fetch url, handled exactly like [`ArtUrl::Http`] from
    /// here on (so its cache entry is shared with the Local Library grid).
    Plex(String),
    /// A Plex thumb with no usable server / token — unresolvable until the
    /// user connects Plex. Distinct from [`ArtUrl::Empty`] so the miss is
    /// logged for what it is instead of looking like a dead download.
    PlexUnconfigured,
}

/// Classify an `art_url`. Cheap for every class except a Plex path, which
/// reads the Plex settings row to build the tokenized url (memoized by
/// [`cached_path`] afterwards, so this happens once per url).
pub fn classify(url: &str) -> ArtUrl {
    let url = url.trim();
    if url.is_empty() {
        return ArtUrl::Empty;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return ArtUrl::Http(url.to_string());
    }
    // A `file://` url is ALREADY on disk — reqwest cannot even build a
    // request for it ("builder error for url"), so it must never be fetched.
    if let Some(path) = url.strip_prefix("file://") {
        return ArtUrl::LocalFile(path.to_string());
    }
    // A Plex thumb is server-relative; it needs base url + token to exist.
    // `local_plex::thumb_url` is the SAME builder the Local Library grid
    // uses, so both surfaces produce identical cache keys.
    if crate::local_plex::is_thumb_path(url) {
        let fetch = crate::local_plex::thumb_url(url, Some(PLEX_THUMB_PX));
        return if fetch.is_empty() {
            ArtUrl::PlexUnconfigured
        } else {
            ArtUrl::Plex(fetch)
        };
    }
    if url.starts_with('/') {
        return ArtUrl::LocalFile(url.to_string());
    }
    ArtUrl::Empty
}

/// `file://` URI for a raw filesystem path.
///
/// Qt's `QUrl` parses `#` as a fragment and `?` as a query, so a cover under
/// `…/Album #1/cover.jpg` resolves to nothing when the path is concatenated
/// raw. Percent-encode exactly those two plus `%` itself (first, or the
/// escapes we add would be double-decoded). Anything already carrying the
/// scheme is passed through untouched.
pub fn file_url(path: &str) -> String {
    if path.starts_with("file://") {
        return path.to_string();
    }
    let mut out = String::with_capacity(path.len() + 8);
    out.push_str("file://");
    for ch in path.chars() {
        match ch {
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            _ => out.push(ch),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Memoized disk-cache lookup
// ---------------------------------------------------------------------------

fn memo_get(key: &str) -> Option<PathBuf> {
    let hit = RESOLVED.read().ok()?.get(key).cloned()?;
    if hit.is_file() {
        return Some(hit);
    }
    // Evicted underneath us (the cache dir is shared with the other
    // frontends) — forget it and let the caller re-resolve / re-download.
    if let Ok(mut memo) = RESOLVED.write() {
        memo.remove(key);
    }
    None
}

fn memo_put(key: &str, path: PathBuf) {
    if let Ok(mut memo) = RESOLVED.write() {
        if memo.len() >= MEMO_CAP {
            memo.clear();
        }
        memo.insert(key.to_string(), path);
    }
}

/// The cached file for `fetch`, memoized under the caller's raw `key`.
/// `key` is what repeats across window passes (a `/library/...` path stays
/// the same while its tokenized url is rebuilt every time), so it is the
/// memo key; `fetch` is the http url the image cache is keyed by.
fn disk_path(key: &str, fetch: &str) -> Option<PathBuf> {
    if let Some(path) = memo_get(key) {
        return Some(path);
    }
    let path = cache().and_then(|c| c.lock().ok()?.get(fetch))?;
    memo_put(key, path.clone());
    Some(path)
}

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// Resolve any art url to a `file://` path QML can decode ("" when it cannot
/// be resolved without the network, or at all). SYNCHRONOUS — no download,
/// and on a repeat call no SQLite either.
pub fn cached_path(url: &str) -> String {
    match classify(url) {
        ArtUrl::Empty | ArtUrl::PlexUnconfigured => String::new(),
        // Already on disk: this is the whole fix for local covers. Stat it so
        // a stale library row yields "" (placeholder) instead of a broken
        // Image source.
        ArtUrl::LocalFile(path) => {
            if Path::new(&path).is_file() {
                file_url(&path)
            } else {
                String::new()
            }
        }
        ArtUrl::Http(fetch) | ArtUrl::Plex(fetch) => disk_path(url, &fetch)
            .map(|p| file_url(&p.to_string_lossy()))
            .unwrap_or_default(),
    }
}

/// Fill `art_path` from the disk cache for every card that already has one.
/// Returns the urls still missing (deduped, non-empty).
pub fn attach_cached(sections: &mut [HomeSection]) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for section in sections.iter_mut() {
        for card in section.items.iter_mut() {
            if card.art_url.is_empty() {
                continue;
            }
            let hit = cached_path(&card.art_url);
            if !hit.is_empty() {
                card.art_path = hit;
            } else if seen.insert(card.art_url.clone()) {
                missing.push(card.art_url.clone());
            }
        }
    }
    missing
}

/// Download whatever in `urls` is actually fetchable, storing each into the
/// image cache. Local files are skipped (they are already on disk) and Plex
/// paths are tokenized first — neither is ever handed to reqwest. Returns
/// when all downloads settled (failures are logged and skipped — the cards
/// keep their placeholder).
pub async fn download_missing(urls: Vec<String>) {
    // (memo key = the caller's raw url, http url to GET), deduped by the
    // fetch url so two rows sharing a cover download it once.
    let mut jobs: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for url in urls {
        match classify(&url) {
            ArtUrl::Http(fetch) | ArtUrl::Plex(fetch) => {
                if seen.insert(fetch.clone()) {
                    jobs.push((url, fetch));
                }
            }
            // Already on disk — `cached_path` resolved it synchronously.
            ArtUrl::LocalFile(_) => {}
            ArtUrl::PlexUnconfigured => {
                log::debug!("[qbz-qt] plex artwork unresolvable (no server/token): {url}");
            }
            ArtUrl::Empty => {}
        }
    }
    if jobs.is_empty() {
        return;
    }
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut handles = Vec::with_capacity(jobs.len());
    for (key, url) in jobs {
        let sem = Arc::clone(&semaphore);
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore open");
            // A concurrent pass may have stored it while we queued.
            if disk_path(&key, &url).is_some() {
                return;
            }
            let response = match HTTP.get(&url).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    log::warn!("[qbz-qt] artwork download failed ({url}): {e}");
                    return;
                }
            };
            // Without this an error page (a 401 from Plex, a 404 from the
            // CDN) is stored AS the cover and the card stays grey forever.
            let response = match response.error_for_status() {
                Ok(resp) => resp,
                Err(e) => {
                    log::warn!("[qbz-qt] artwork download rejected ({url}): {e}");
                    return;
                }
            };
            let bytes = match response.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("[qbz-qt] artwork read failed ({url}): {e}");
                    return;
                }
            };
            if bytes.is_empty() {
                log::warn!("[qbz-qt] artwork empty body ({url})");
                return;
            }
            let Some(cache) = cache() else {
                return;
            };
            let stored = cache
                .lock()
                .ok()
                .and_then(|guard| match guard.store(&url, &bytes) {
                    Ok(path) => Some(path),
                    Err(e) => {
                        log::warn!("[qbz-qt] artwork store failed ({url}): {e}");
                        None
                    }
                });
            if let Some(path) = stored {
                // Seed the memo so the republish that follows resolves from
                // RAM instead of re-querying SQLite for every card.
                memo_put(&key, path);
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

/// Attach the current track's artwork to the NowPlayingModel: a disk hit (or
/// a local file, which is always a hit) applies immediately; a miss downloads
/// in the background and then republishes.
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
    // Nothing fetchable (a missing local cover, Plex not connected): clear
    // the bar instead of leaving the previous track's cover behind.
    if matches!(
        classify(url),
        ArtUrl::Empty | ArtUrl::LocalFile(_) | ArtUrl::PlexUnconfigured
    ) {
        crate::now_playing::set_artwork_path(String::new());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_urls_are_fetchable() {
        assert_eq!(
            classify("https://static.qobuz.com/images/covers/a.jpg"),
            ArtUrl::Http("https://static.qobuz.com/images/covers/a.jpg".into())
        );
    }

    #[test]
    fn file_urls_resolve_to_a_raw_path_and_are_never_fetched() {
        assert_eq!(
            classify("file:///home/u/.local/share/qbz/thumbnails/deadbeef.jpg"),
            ArtUrl::LocalFile("/home/u/.local/share/qbz/thumbnails/deadbeef.jpg".into())
        );
        assert_eq!(
            classify("/home/u/Music/Album/cover.jpg"),
            ArtUrl::LocalFile("/home/u/Music/Album/cover.jpg".into())
        );
    }

    #[test]
    fn relative_and_empty_urls_are_empty() {
        assert_eq!(classify(""), ArtUrl::Empty);
        assert_eq!(classify("   "), ArtUrl::Empty);
        assert_eq!(classify("covers/a.jpg"), ArtUrl::Empty);
    }

    #[test]
    fn file_url_escapes_what_qurl_would_eat() {
        assert_eq!(file_url("/m/Album #1/c.jpg"), "file:///m/Album %231/c.jpg");
        assert_eq!(file_url("/m/a?b/c.jpg"), "file:///m/a%3Fb/c.jpg");
        assert_eq!(file_url("/m/100%/c.jpg"), "file:///m/100%25/c.jpg");
        assert_eq!(file_url("file:///m/a.jpg"), "file:///m/a.jpg");
    }

    #[test]
    fn missing_local_file_resolves_to_placeholder() {
        assert_eq!(cached_path("/nonexistent/qbz/cover.jpg"), "");
    }
}
