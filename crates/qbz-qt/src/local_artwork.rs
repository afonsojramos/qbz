//! Windowed, id-keyed artwork for the Local Library (the perf contract:
//! a 16K-track library must never decode a cover it does not show).
//!
//! Split out of `local_library_qt.rs` (phase-24 modularization) and made
//! source-aware. The routing decision is NOT made here — `artwork_qt::classify`
//! owns the one url taxonomy the whole app shares (see its module docs), so a
//! cover resolves identically in the grid, the queue and the now-playing bar:
//!
//!  - a LOCAL cover path resolves to the `qbz-library` 256px THUMBNAIL
//!    (generated once, cached on disk) — never the full-size original, which
//!    is what keeps a 16K library from decoding 3000px jpegs into cards;
//!  - a PLEX thumb (`/library/...` / `/photo/...`) resolves to the tokenized
//!    server-side transcode URL and is served through the SAME shared image
//!    cache the Home/queue covers use (`artwork_qt`), at the SAME transcode
//!    size (`artwork_qt::PLEX_THUMB_PX`), so a Plex cover is downloaded once
//!    for the whole process and then read from disk like any other;
//!  - an http(s) cover (an offline-download row that kept its CDN url) goes
//!    down the same disk-cache path as a Plex thumb.
//!
//! Every arm ends at a `file://` path — QML `Image` decodes it natively and
//! asynchronously, and no token ever reaches QML.
//!
//! Re-entering the view re-reports the same window, and the whole pass is
//! batched into ONE emit: it therefore has to be fast, not merely eventually
//! correct. [`THUMBS`] memoizes source path -> generated thumbnail so the
//! second visit skips `get_or_generate_thumbnail` (which stats the thumbnail
//! dir, `create_dir_all`s it and hashes the path on every call), and the Plex
//! arm rides `artwork_qt`'s own memo for the same reason.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, RwLock};

use crate::artwork_qt::{self, ArtUrl};
use crate::local_state::state;

/// Memo ceiling, mirroring `artwork_qt::MEMO_CAP`: an accelerator, cleared
/// wholesale rather than carrying LRU bookkeeping.
const MEMO_CAP: usize = 8192;

/// Cover source path -> its generated 256px thumbnail. See the module docs.
static THUMBS: LazyLock<RwLock<HashMap<String, PathBuf>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// The result of one window pass.
pub struct ArtworkWindow {
    /// (artKey, "file://…") — ready to emit immediately.
    pub hits: Vec<(String, String)>,
    /// (artKey, fetchable http url) — not on disk yet; the bridge downloads
    /// these off the Qt thread and re-resolves them to `file://`. Named for
    /// its dominant case; an http cover on a Plex-less row lands here too.
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
        match artwork_qt::classify(&src) {
            // --- Plex / remote arm: disk cache, else a background fetch ----
            ArtUrl::Plex(_) | ArtUrl::Http(_) => {
                let cached = artwork_qt::cached_path(&src);
                if cached.is_empty() {
                    // Hand the bridge the RAW source: `download_missing`
                    // re-classifies it, and `cached_path` memoizes under the
                    // raw url, so the re-resolve below is a RAM read.
                    plex_misses.push((key, src));
                } else {
                    hits.push((key, cached));
                }
            }
            // No server / token: nothing to fetch, leave the card blank.
            ArtUrl::PlexUnconfigured | ArtUrl::Empty => {}
            // --- Local arm: thumbnail the on-disk cover -------------------
            ArtUrl::LocalFile(path) => {
                if let Some(thumb) = local_thumbnail(&path) {
                    hits.push((key, artwork_qt::file_url(&thumb.to_string_lossy())));
                }
            }
        }
    }
    ArtworkWindow { hits, plex_misses }
}

/// The 256px thumbnail for a local cover, memoized. Falls back to the
/// original file when no thumbnail can be produced (unsupported source) so
/// the card is not blank; `None` only when the cover itself is gone.
fn local_thumbnail(src: &str) -> Option<PathBuf> {
    if let Ok(memo) = THUMBS.read() {
        if let Some(hit) = memo.get(src) {
            if hit.is_file() {
                return Some(hit.clone());
            }
        }
    }
    let path = Path::new(src);
    if !path.is_file() {
        return None;
    }
    let resolved = qbz_library::get_or_generate_thumbnail(path).unwrap_or_else(|_| path.to_owned());
    if let Ok(mut memo) = THUMBS.write() {
        if memo.len() >= MEMO_CAP {
            memo.clear();
        }
        memo.insert(src.to_string(), resolved.clone());
    }
    Some(resolved)
}

/// Download the misses into the shared image cache and return the resolved
/// `(key, file://…)` pairs. Async (network) — the bridge awaits it on the
/// tokio runtime, never on the Qt thread.
pub async fn fetch_plex_misses(misses: Vec<(String, String)>) -> Vec<(String, String)> {
    if misses.is_empty() {
        return Vec::new();
    }
    let urls: Vec<String> = misses.iter().map(|(_, u)| u.clone()).collect();
    artwork_qt::download_missing(urls).await;
    misses
        .into_iter()
        .filter_map(|(key, url)| {
            let path = artwork_qt::cached_path(&url);
            (!path.is_empty()).then_some((key, path))
        })
        .collect()
}
