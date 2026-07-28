//! Windowed, id-keyed artwork for the Local Library (the perf contract:
//! a 16K-track library must never decode a cover it does not show).
//!
//! Split out of `local_library_qt.rs` (phase-24 modularization) and made
//! source-aware:
//!
//!  - a LOCAL cover path resolves to the `qbz-library` 256px THUMBNAIL
//!    (generated once, cached on disk);
//!  - a PLEX thumb (`/library/...` / `/photo/...`) resolves to the tokenized
//!    server-side transcode URL and is served through the SAME shared image
//!    cache the Home/queue covers use (`artwork_qt`), so a Plex cover is
//!    downloaded once per size and then read from disk like any other.
//!
//! Both arms end at a `file://` path — QML `Image` decodes it natively and
//! asynchronously, and no token ever reaches QML.

use std::collections::HashSet;
use std::path::Path;

use crate::local_state::state;

/// Decode size for browse thumbnails (matches the `qbz-library` thumbnail
/// size, so the local and Plex arms produce visually identical cards).
const THUMB_PX: u32 = 256;

/// The result of one window pass.
pub struct ArtworkWindow {
    /// (artKey, "file://…") — ready to emit immediately.
    pub hits: Vec<(String, String)>,
    /// (artKey, tokenized Plex URL) — not on disk yet; the bridge downloads
    /// these off the Qt thread and re-resolves them to `file://`.
    pub plex_misses: Vec<(String, String)>,
}

/// Resolve the mounted window's artKeys. BLOCKING (image decode + disk) —
/// the bridge runs it on `spawn_blocking`. Keys with no cover are dropped, so
/// the QML map only ever grows with real hits.
pub fn resolve_window_blocking(keys: Vec<String>) -> ArtworkWindow {
    let sources: Vec<(String, String)> = state(|s| {
        keys.iter()
            .filter_map(|k| s.art_index.get(k).map(|p| (k.clone(), p.clone())))
            .collect()
    });
    let mut hits = Vec::with_capacity(sources.len());
    let mut plex_misses = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (key, src) in sources {
        if !seen.insert(key.clone()) {
            continue;
        }
        // --- Plex arm: a server-relative thumb path, never a file ---------
        if crate::local_plex::is_thumb_path(&src) {
            let url = crate::local_plex::thumb_url(&src, Some(THUMB_PX));
            if url.is_empty() {
                continue; // no creds — nothing to fetch.
            }
            let cached = crate::artwork_qt::cached_path(&url);
            if cached.is_empty() {
                plex_misses.push((key, url));
            } else {
                hits.push((key, cached));
            }
            continue;
        }
        // --- Local arm: thumbnail the on-disk cover -----------------------
        let path = Path::new(&src);
        if !path.exists() {
            continue;
        }
        match qbz_library::get_or_generate_thumbnail(path) {
            Ok(thumb) => hits.push((key, format!("file://{}", thumb.display()))),
            // No thumbnail (unsupported source): fall back to the original
            // file so the card is not blank.
            Err(_) => hits.push((key, format!("file://{src}"))),
        }
    }
    ArtworkWindow { hits, plex_misses }
}

/// Download the Plex misses into the shared image cache and return the
/// resolved `(key, file://…)` pairs. Async (network) — the bridge awaits it
/// on the tokio runtime, never on the Qt thread.
pub async fn fetch_plex_misses(misses: Vec<(String, String)>) -> Vec<(String, String)> {
    if misses.is_empty() {
        return Vec::new();
    }
    let urls: Vec<String> = misses.iter().map(|(_, u)| u.clone()).collect();
    crate::artwork_qt::download_missing(urls).await;
    misses
        .into_iter()
        .filter_map(|(key, url)| {
            let path = crate::artwork_qt::cached_path(&url);
            (!path.is_empty()).then_some((key, path))
        })
        .collect()
}
