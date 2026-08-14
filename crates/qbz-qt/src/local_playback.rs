//! Local Library playback: `LocalTrack` -> `QueueTrack` -> `core().set_queue`
//! -> the player's audible seam.
//!
//! Split out of `local_library_qt.rs` (phase-24 modularization) and made
//! source-aware: a queue built here can mix LOCAL files, OFFLINE (Qobuz
//! download) copies and PLEX rows, and the audible step routes per row —
//! local/offline read from disk (`play_data` / `play_dsd_file`), Plex
//! resolves its direct-play part from the server.
//!
//! The PROTECTED audio path is only ENTERED here, never modified: the file
//! bytes are handed to the same `play_data` seam the Slint frontend uses, so
//! sample rate / bit depth stay whatever the decoder found.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_library::LocalTrack;
use qbz_models::QueueTrack;

use crate::local_albums::fetch_album_tracks_blocking;
use crate::local_state::{state, with_db};

type Runtime = Arc<AppRuntime<LoggingAdapter>>;

// ---------------------------------------------------------------------------
// On-disk cover backfill (playback.rs `fill_missing_covers` +
// local_library.rs `find_folder_cover`, both 1:1)
// ---------------------------------------------------------------------------

/// The reference's robust on-disk cover lookup for one folder
/// (`local_library.rs:3110 find_folder_cover`): a known stem first, then a
/// file named after the folder, then any image at all. Stems and extensions
/// are the reference's list verbatim — do not prune it, the order IS the
/// preference.
pub(crate) fn find_folder_cover(folder: &Path) -> Option<String> {
    const STEMS: &[&str] = &[
        "cover",
        "folder",
        "front",
        "art",
        "album",
        "albumart",
        "albumartsmall",
        "thumb",
        "artwork",
        "scan",
        "booklet",
        "title",
    ];
    const EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff"];
    let is_img = |p: &Path| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| EXTS.iter().any(|x| x.eq_ignore_ascii_case(e)))
            .unwrap_or(false)
    };
    let mut entries: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_img(p))
        .collect();
    if entries.is_empty() {
        return None;
    }
    entries.sort();
    let stem_lower = |p: &Path| {
        p.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default()
    };
    let folder_name = folder
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let by_stem = entries
        .iter()
        .find(|p| STEMS.contains(&stem_lower(p).as_str()));
    let by_name = entries
        .iter()
        .find(|p| !folder_name.is_empty() && stem_lower(p) == folder_name);
    by_stem
        .or(by_name)
        .cloned()
        .or_else(|| entries.into_iter().next())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Fill `artwork_path` for the rows that lack one, from a cover image sitting
/// in the track's folder — the port of `playback.rs:1490 fill_missing_covers`,
/// which the reference runs on EVERY local play/enqueue path before it maps
/// rows to `QueueTrack`s (playback.rs:1094/1322/1349/1382, local_library.rs:1826,
/// main.rs:10636, offline_favorites.rs:128). This port had none of it, so a
/// library row whose `artwork_path` is NULL — the offline cache writes
/// `cover.jpg` next to the file without always backfilling the index, and a
/// ripped folder often carries the art only on disk — reached the queue with
/// `artwork_url: None` and the panel had nothing to request.
///
/// BLOCKING (one `read_dir` per distinct folder, memoized), so every caller
/// runs it inside `spawn_blocking`: a NAS-backed library would otherwise stall
/// a tokio worker.
pub(crate) fn fill_missing_covers(tracks: &mut [LocalTrack]) {
    use std::collections::HashMap;
    let mut memo: HashMap<String, Option<String>> = HashMap::new();
    for t in tracks.iter_mut() {
        if t.artwork_path.as_deref().is_some_and(|s| !s.is_empty()) {
            continue;
        }
        let p = Path::new(&t.file_path);
        let folder = if p.is_dir() {
            p.to_path_buf()
        } else {
            match p.parent() {
                Some(d) => d.to_path_buf(),
                None => continue,
            }
        };
        let key = folder.to_string_lossy().into_owned();
        let cover = memo
            .entry(key)
            .or_insert_with(|| find_folder_cover(&folder))
            .clone();
        if cover.is_some() {
            t.artwork_path = cover;
        }
    }
}

// ---------------------------------------------------------------------------
// Queue mapping (playback.rs `local_queue_track` 1:1, all three sources)
// ---------------------------------------------------------------------------

pub fn local_queue_track(t: &LocalTrack) -> QueueTrack {
    let src = match t.source.as_deref() {
        Some("qobuz_download") => "qobuz_download",
        Some("plex") => "plex",
        _ => "local",
    };
    let is_offline = src == "qobuz_download";
    let is_plex = src == "plex";
    // A Plex row carries a RAW server-relative thumb path; it must stay raw
    // so the now-playing bar / queue resolve it from current creds.
    // `file://`-prefixing it poisons it into a local-read miss.
    let artwork_url = t.artwork_path.as_ref().map(|p| {
        if is_plex || p.starts_with("file://") {
            p.clone()
        } else {
            format!("file://{p}")
        }
    });
    let sample_rate_khz = if t.sample_rate >= 1000.0 {
        t.sample_rate / 1000.0
    } else {
        t.sample_rate
    };
    QueueTrack {
        id: if is_offline {
            t.qobuz_track_id.unwrap_or(t.id) as u64
        } else {
            t.id as u64
        },
        title: t.title.clone(),
        version: None,
        artist: t.artist.clone(),
        album: t.album_group_title.clone(),
        album_version: None,
        duration_secs: t.duration_secs,
        artwork_url,
        hires: t.bit_depth.map(|d| d > 16).unwrap_or(false),
        bit_depth: t.bit_depth,
        sample_rate: Some(sample_rate_khz),
        is_local: true,
        // Navigation key. For Plex the track's `album_group_key` is the
        // per-edition split key, which the album cache is NOT keyed by —
        // recover the content-hash key so "go to album" resolves.
        album_id: Some(if is_plex {
            crate::local_plex::album_key_for(&t.artist, &t.album)
        } else {
            t.album_group_key.clone()
        }),
        artist_id: None,
        streamable: true,
        source: Some(src.to_string()),
        parental_warning: false,
        // Plex: the string rating_key the resolve needs (the numeric queue id
        // is a namespaced form). Offline: the local row id.
        source_item_id_hint: if is_plex || is_offline {
            Some(if is_plex {
                t.file_path.clone()
            } else {
                t.id.to_string()
            })
        } else {
            None
        },
        // NO container origin. `"local"` is a kind the Slint never emits and
        // nothing consumes: `playback.rs`'s own `local_queue_track`
        // (:1478-1481) leaves both fields None and lets the per-track ALBUM
        // fallback in `refresh_now_playing_meta` (:1959-1965) land the glyph on
        // the LocalAlbum view. Leaving it stamped also defeated the shared
        // seam — `stamp_context`'s already-stamped test is satisfied by
        // ("local", key), so routing these queues through `set_queue_stamped`
        // would have preserved the invented kind instead of deriving a real
        // one. Post-change: a single-album local queue derives ("album",
        // group_key); a folder / Tracks-tab queue spanning albums derives
        // nothing and falls back per track to the same key.
        context_kind: None,
        context_id: None,
    }
}

// ---------------------------------------------------------------------------
// Audible steps
// ---------------------------------------------------------------------------

/// The audible step for a LOCAL/OFFLINE queue track: read the file and hand
/// the bytes to the player. False when the row can't be resolved (missing db
/// row, unmounted drive) so the caller can fall through.
///
/// DSD (.dsf/.dff) is streamed from disk by the player instead of slurped.
pub async fn play_local_file(runtime: &Runtime, row_id: u64) -> bool {
    // An ephemeral row id has no library.db row (the session store owns it),
    // so route it before any DB lookup — this is the path auto-advance and
    // queue-row clicks take.
    if crate::local_ephemeral::is_ephemeral_id(row_id as i64) {
        return crate::local_ephemeral::play_file(runtime, row_id).await;
    }
    let info = tokio::task::spawn_blocking(move || {
        with_db(|db| db.get_track(row_id as i64))
            .flatten()
            .map(|t| (t.file_path, t.cue_start_secs))
    })
    .await
    .ok()
    .flatten();
    let Some((path, cue)) = info else {
        log::error!("[qbz-qt] local play: track {row_id} not found");
        return false;
    };
    let lower = path.to_lowercase();
    if lower.ends_with(".dsf") || lower.ends_with(".dff") {
        if let Err(e) = runtime
            .core()
            .player()
            .play_dsd_file(PathBuf::from(&path), row_id)
        {
            log::error!("[qbz-qt] local play: play_dsd_file {row_id} failed: {e}");
            return false;
        }
        return true;
    }
    // CUE fast path: every virtual track of a CUE album shares ONE audio
    // file. If that container is already loaded, seek instead of re-reading.
    if let Some(start) = cue.filter(|s| *s > 0.0) {
        let loaded = runtime.core().player().state.current_track_id();
        if runtime.core().player().has_loaded_audio() && loaded == row_id {
            let _ = runtime.core().player().seek(start as u64);
            return true;
        }
    }
    let read_path = path.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        if !Path::new(&read_path).exists() {
            return None;
        }
        std::fs::read(&read_path).ok()
    })
    .await
    .ok()
    .flatten();
    let Some(bytes) = bytes else {
        log::error!("[qbz-qt] local play: file not available at {path} (drive unmounted?)");
        return false;
    };
    if let Err(e) = runtime.core().player().play_data(bytes, row_id) {
        log::error!("[qbz-qt] local play: play_data {row_id} failed: {e}");
        return false;
    }
    if let Some(start) = cue.filter(|s| *s > 0.0) {
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let _ = runtime.core().player().seek(start as u64);
    }
    true
}

/// The audible step for a PLEX queue track: resolve the direct-play part and
/// hand the ORIGINAL bytes to the player (bit-perfect — no transcode is
/// requested).
///
/// POC-NOTE: the Slint frontend Range-streams the part progressively
/// (`remote_stream`, which lives inside the Slint binary) and only falls back
/// to this whole-file download when streaming setup fails. The Qt port has no
/// streaming feeder yet, so it takes the documented fallback path: correct
/// audio, slower first-note on a large FLAC.
pub async fn play_plex_track(runtime: &Runtime, rating_key: String, play_id: u64) -> bool {
    let cfg = crate::local_plex::settings();
    if cfg.base_url.is_empty() || cfg.token.is_empty() {
        log::error!("[qbz-qt] plex play: no Plex credentials configured");
        return false;
    }
    // TIMED, because the shape of this delay does not match its obvious
    // explanation. The whole-file download below is a known POC gap, but the
    // owner measured this path taking LONGER than a Qobuz track that streams
    // over the internet — and 58 MB over a LAN cannot do that. So before any
    // redesign, the log says which segment actually costs: the metadata round
    // trip, the transfer, or the handoff to the player. It also prints the base
    // URL, because `resolve_base_url` can hand back the plex.tv RELAY instead
    // of the LAN address, and a relayed transfer is a round trip through Plex's
    // servers — that alone would outweigh a local download and would explain
    // being slower than remote Qobuz.
    let t0 = std::time::Instant::now();
    let base = cfg.base_url.clone();
    let is_lan = crate::local_plex::is_local_address(&base);

    // PROGRESSIVE FIRST. `plex_resolve_part_url` stops at the URL and the
    // shared feeder Range-streams the original bytes, so audio starts on the
    // first chunk instead of after the whole FLAC is in RAM. The doc on
    // `PlexPartLocation` states the intent outright — "~1s to first audio
    // instead of buffering the whole FLAC into RAM first" — and it went unused
    // here only because the feeder lived inside the Slint binary until it was
    // moved to `qbz_player::remote_stream`.
    //
    // Duration comes from the queue row (the feeder needs it for its buffer
    // maths); 0 is acceptable — it only makes the estimate conservative.
    let duration = runtime
        .core()
        .current_track()
        .await
        .map(|t| t.duration_secs as u64)
        .unwrap_or(0);
    match qbz_plex::plex_resolve_part_url(base.clone(), cfg.token.clone(), rating_key.clone()).await
    {
        Ok(loc) => {
            let resolved = t0.elapsed();
            match qbz_player::remote_stream::stream_remote_track_into_player(
                &runtime.core().player(),
                play_id,
                duration,
                0,
                &loc.part_url,
                "PLEX",
            )
            .await
            {
                Ok(()) => {
                    log::info!(
                        "[qbz-qt][perf] plex play {play_id}: STREAMED — resolve {resolved:?}, \
                         first audio {:?} — base {} ({})",
                        t0.elapsed(),
                        base,
                        if is_lan { "LAN" } else { "NOT a LAN address — relayed?" },
                    );
                    return true;
                }
                // The whole-file path below stays as the fallback, exactly as
                // the reference keeps it: a server that refuses Range, or a
                // part that will not probe, still plays — just slowly.
                Err(e) => log::warn!(
                    "[qbz-qt] plex play {play_id}: streaming failed ({e}) — \
                     falling back to whole-file download"
                ),
            }
        }
        Err(e) => log::warn!("[qbz-qt] plex play: part-url resolve failed ({e}) — full download"),
    }
    match qbz_plex::plex_resolve_track_media(cfg.base_url, cfg.token, rating_key.clone()).await {
        Ok(media) => {
            let fetched = t0.elapsed();
            let bytes = media.bytes.len();
            let t1 = std::time::Instant::now();
            if let Err(e) = runtime.core().player().play_data(media.bytes, play_id) {
                log::error!("[qbz-qt] plex play: play_data {play_id} failed: {e}");
                return false;
            }
            log::info!(
                "[qbz-qt][perf] plex play {play_id}: resolve+fetch {:?} for {} bytes \
                 ({:.1} MB/s), play_data {:?} — base {} ({})",
                fetched,
                bytes,
                (bytes as f64 / 1_048_576.0) / fetched.as_secs_f64().max(0.001),
                t1.elapsed(),
                base,
                if is_lan { "LAN" } else { "NOT a LAN address — relayed?" },
            );
            true
        }
        Err(e) => {
            log::error!("[qbz-qt] plex play: resolve {rating_key} failed: {e}");
            false
        }
    }
}

/// Resolve the Plex track rating key for a queue row (PARITY-DEBT #4, ports
/// `playback.rs:666-680`, commit `b5c1a76e`).
///
/// The string rating_key rides in `source_item_id_hint` on the LocalLibrary
/// path (`local_queue_track` above stamps `file_path`, which for a Plex row IS
/// the raw key). The MyQBZ collections path stamps the per-item ALBUM key there
/// instead (`plex:<hash>` from `qbz_plex::plex_album_key`, for shuffle boundary
/// detection) — that is NOT a track rating key, so ignore any `plex:`-prefixed
/// hint and fall back to the numeric queue id (= rating_key for the common
/// numeric-key case). Using it verbatim made
/// `GET /library/metadata/plex:<hash>` 404: the Plex track never started and
/// the previous track kept playing under the new card.
///
/// A MISSING hint falls back the same way — the reference's `_` arm covers both
/// `None` and the prefixed case, so this no longer refuses the row.
pub(crate) fn plex_rating_key(hint: Option<&str>, track_id: u64) -> String {
    match hint {
        Some(hint) if !hint.starts_with("plex:") => hint.to_string(),
        _ => track_id.to_string(),
    }
}

/// Route ONE queue track to its audible step.
async fn play_audible(runtime: &Runtime, track: &QueueTrack) -> bool {
    match track.source.as_deref() {
        Some("plex") => {
            let rating_key = plex_rating_key(track.source_item_id_hint.as_deref(), track.id);
            play_plex_track(runtime, rating_key, track.id).await
        }
        _ => play_local_file(runtime, track.id).await,
    }
}

/// Source-aware AUDIBLE step for the shared poll-loop advance: when the
/// CURRENT queue track is a local file or a Plex row, play it here and report
/// true; otherwise report false so the Qobuz tier-walk runs unchanged.
///
/// Without this seam, auto-advance to the next local/Plex track goes down
/// `play_track_resolved` and fails with "No Qobuz client available".
pub async fn play_current_if_local(runtime: &Runtime, track_id: u64) -> bool {
    let Some(qt) = runtime.core().current_track().await else {
        return false;
    };
    if qt.id != track_id {
        return false;
    }
    match qt.source.as_deref() {
        Some("local") | Some("plex") => play_audible(runtime, &qt).await,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Queue builders
// ---------------------------------------------------------------------------

/// Set the queue to `tracks` and start `start` (playback.rs
/// `play_local_tracks_now`: `set_queue` then the AUDIBLE step).
pub(crate) async fn play_rows(
    runtime: &Runtime,
    tracks: Vec<LocalTrack>,
    start: usize,
    shuffle: bool,
) {
    if tracks.is_empty() {
        return;
    }
    // Folder-cover backfill BEFORE the mapping (the reference runs it at every
    // local play site: playback.rs:1094 / :1322 / :1349 / :1382). This is the
    // funnel for album / folder / folder-track / Tracks-tab play, so one call
    // here covers all four. Blocking fs, hence spawn_blocking.
    let tracks = tokio::task::spawn_blocking(move || {
        let mut tracks = tracks;
        fill_missing_covers(&mut tracks);
        tracks
    })
    .await
    .unwrap_or_default();
    if tracks.is_empty() {
        return;
    }
    let mut queue: Vec<QueueTrack> = tracks.iter().map(local_queue_track).collect();
    // Shuffle reorders THIS list and starts at the top of the mixed order. The
    // mode alone only randomises what comes next, so the first track the user
    // hears was the folder's #1 every time — owner ruling 2026-08-01, every
    // shuffle must be genuinely random. The caller's anchor is meaningless once
    // the order is mixed, so it is dropped (`play_track_list_in` does the same).
    let start = if shuffle {
        runtime.core().set_shuffle(true).await;
        crate::now_playing::set_shuffle(true);
        crate::playback_qt::xorshift_shuffle(&mut queue);
        0
    } else {
        start.min(queue.len() - 1)
    };
    let first = queue[start].clone();
    // Through the SHARED seam (not `core().set_queue`) so the origin is derived
    // from the queue: one album -> ("album", group_key), a mixed folder queue ->
    // nothing, and the per-track album fallback lands it on the same view.
    crate::playback_qt::set_queue_stamped(runtime, queue, Some(start), None).await;
    crate::playback_qt::publish_queue(runtime).await;
    play_audible(runtime, &first).await;
    crate::playback_qt::refresh_now_playing(runtime).await;
}

/// Play a whole album (its group key, local or `plex:<hash>`) from the top,
/// or from `start_track_id` when a row was clicked.
pub async fn play_album(
    runtime: &Runtime,
    album_id: String,
    start_track_id: Option<i64>,
    shuffle: bool,
) {
    let key = album_id.clone();
    let tracks =
        tokio::task::spawn_blocking(move || fetch_album_tracks_blocking(&key))
            .await
            .unwrap_or_default();
    let start = start_track_id
        .and_then(|tid| tracks.iter().position(|t| t.id == tid))
        .unwrap_or(0);
    play_rows(runtime, tracks, start, shuffle).await;
}

/// Play everything under a folder, recursively, in path order.
pub async fn play_folder(runtime: &Runtime, path: String) {
    let tracks = tokio::task::spawn_blocking(move || {
        with_db(|db| db.list_folder_tracks_recursive(&path, false)).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    play_rows(runtime, tracks, 0, false).await;
}

/// Play a folder's DIRECT tracks starting at the clicked row.
pub async fn play_folder_track(runtime: &Runtime, folder: String, row_id: i64) {
    let tracks = tokio::task::spawn_blocking(move || {
        with_db(|db| db.list_folder_tracks(&folder, false)).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    let start = tracks.iter().position(|t| t.id == row_id).unwrap_or(0);
    play_rows(runtime, tracks, start, false).await;
}

/// Reorder the loaded raw rows into the order the Tracks tab is RENDERING and
/// locate the clicked row inside it.
///
/// `None` — and the caller falls back to the SQL-page order — when the clicked
/// id does not resolve inside the ordered list. Same guard, same reason as
/// `library_qt::order_by_visible` (the port of `playback.rs::order_by_visible`,
/// :3408-3428): better the old queue than a queue that starts on the wrong row.
///
/// Ids arrive as strings because that is what the QML rows carry
/// (`local_rows::TrackRow.id`); unparseable ones are dropped.
pub(crate) fn order_by_visible(
    rows: &[LocalTrack],
    visible_ids: &[String],
    clicked_id: i64,
) -> Option<(Vec<LocalTrack>, usize)> {
    let by_id: std::collections::HashMap<i64, &LocalTrack> =
        rows.iter().map(|t| (t.id, t)).collect();
    let ordered: Vec<LocalTrack> = visible_ids
        .iter()
        .filter_map(|id| id.parse::<i64>().ok())
        .filter_map(|id| by_id.get(&id).map(|t| (*t).clone()))
        .collect();
    let idx = ordered.iter().position(|t| t.id == clicked_id)?;
    Some((ordered, idx))
}

/// Tracks tab row click (PARITY-DEBT #14): the ALREADY-LOADED page set becomes
/// the queue so playback continues down the list, in the order ON SCREEN. No
/// DB re-query — the raw rows are kept by the loader, which is also what makes
/// a merged PLEX row playable (it has no `local_tracks` row to re-query).
///
/// `visible_ids_json` is the JSON string array of the track ids the tab is
/// CURRENTLY rendering, in render order; `clicked_id` is the row that was hit.
///
/// The Tracks tab's SORT is server-side (it defines the pagination order), but
/// its GROUP modes ("by album" / "by artist" / "by name") are a client-side
/// visual reorder on top of the loaded pages — the reference is explicit that
/// they "keep their client-side visual reorder on top" (commit `e379aa65`).
/// Queueing `tracks_raw` therefore played the SQL order while the user was
/// looking at the grouped one: click row 3 of the "Air" group and you heard
/// whatever happened to sit at SQL offset 3. The order can only come from the
/// view (it is derived in QML, like `LibraryView`'s), so it comes down as an
/// id array and the raw rows play the part of the authoritative cache.
pub async fn play_tracks_visible(runtime: &Runtime, visible_ids_json: String, clicked_id: i64) {
    let rows: Vec<LocalTrack> = state(|s| s.tracks_raw.clone());
    if rows.is_empty() {
        return;
    }
    let ids: Vec<String> = serde_json::from_str(&visible_ids_json).unwrap_or_default();
    match order_by_visible(&rows, &ids, clicked_id) {
        Some((ordered, start)) => play_rows(runtime, ordered, start, false).await,
        None => {
            let start = rows.iter().position(|t| t.id == clicked_id).unwrap_or(0);
            play_rows(runtime, rows, start, false).await;
        }
    }
}

/// Look up ONE raw row by id: the loaded Tracks page first, then the open
/// detail pane, then `library.db` (Plex ids never reach the DB — they are
/// namespaced and only ever live in the cached documents).
fn find_track_blocking(row_id: i64) -> Option<LocalTrack> {
    let cached = state(|s| {
        s.tracks_raw
            .iter()
            .chain(s.detail_raw.iter())
            .find(|t| t.id == row_id)
            .cloned()
    });
    if cached.is_some() {
        return cached;
    }
    if crate::local_plex::is_plex_track_id(row_id) {
        return None;
    }
    with_db(|db| db.get_track(row_id)).flatten()
}

/// Row / card "Play next" / "Play later" / "Add to queue".
/// `kind` = "track" | "album" | "folder"; `mode` = "next" | "later" | queue.
pub async fn enqueue(runtime: &Runtime, kind: String, id: String, mode: String) {
    let k = kind.clone();
    let ident = id.clone();
    let tracks: Vec<LocalTrack> = tokio::task::spawn_blocking(move || {
        let mut rows: Vec<LocalTrack> = match k.as_str() {
            "track" => ident
                .parse::<i64>()
                .ok()
                .and_then(find_track_blocking)
                .into_iter()
                .collect(),
            "folder" => {
                with_db(|db| db.list_folder_tracks_recursive(&ident, false)).unwrap_or_default()
            }
            _ => fetch_album_tracks_blocking(&ident),
        };
        // Same backfill the play funnel does — an enqueued row must carry the
        // same cover it would have carried had it been played
        // (reference: main.rs:10636 fills before `enqueue_local_tracks`).
        fill_missing_covers(&mut rows);
        rows
    })
    .await
    .unwrap_or_default();
    if tracks.is_empty() {
        return;
    }
    // Same stamping seam the Qobuz enqueue paths use, so an appended local
    // block carries its own origin instead of inheriting whatever is playing.
    let queue = crate::playback_qt::stamped(
        tracks.iter().map(local_queue_track).collect(),
        None,
    );
    // Same core helpers the Qobuz rows use: "next" inserts at the cursor
    // (reversed so a multi-track insert keeps its order), "later" appends to
    // the block tail, anything else appends.
    //
    // QConnect EXEMPT (contract §6.3): no routed arm and no sync-on-add tail
    // here — this is a LOCAL-only path, so `batch_all_qconnect_castable` is
    // always false and the push would never fire; hooking the arm would only
    // risk the refusal toast on every click (Slint local_playback.rs:437-450
    // is likewise unhooked).
    match mode.as_str() {
        "next" => {
            for t in queue.into_iter().rev() {
                runtime.core().add_track_next(t).await;
            }
        }
        "later" => {
            for t in queue {
                runtime.core().add_track_later(t).await;
            }
        }
        _ => runtime.core().add_tracks(queue).await,
    }
    crate::playback_qt::publish_queue(runtime).await;
}

#[cfg(test)]
mod tests {
    use super::{order_by_visible, plex_rating_key};
    use qbz_library::LocalTrack;

    fn track(id: i64, title: &str) -> LocalTrack {
        LocalTrack {
            id,
            title: title.to_string(),
            ..Default::default()
        }
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn titles(rows: &[LocalTrack]) -> Vec<&str> {
        rows.iter().map(|t| t.title.as_str()).collect()
    }

    /// PARITY-DEBT #14: the queue follows the VISIBLE order (here: reversed by
    /// a group mode), not the SQL page order the loader cached.
    #[test]
    fn queue_follows_the_visible_order() {
        let rows = vec![track(1, "a"), track(2, "b"), track(3, "c")];
        let (ordered, start) =
            order_by_visible(&rows, &ids(&["3", "1", "2"]), 1).expect("clicked row resolves");
        assert_eq!(titles(&ordered), vec!["c", "a", "b"]);
        assert_eq!(start, 1);
    }

    /// Ids the loaded page does not have (a row scrolled out of the raw cache,
    /// a stale report) are dropped rather than poisoning the queue, and the
    /// start index still points at the clicked row AFTER the drop.
    #[test]
    fn unknown_and_unparseable_ids_are_dropped() {
        let rows = vec![track(1, "a"), track(2, "b")];
        let (ordered, start) = order_by_visible(&rows, &ids(&["99", "not-a-number", "2", "1"]), 1)
            .expect("clicked row resolves");
        assert_eq!(titles(&ordered), vec!["b", "a"]);
        assert_eq!(start, 1);
    }

    /// The clicked row not being in the visible list is the one case the
    /// caller must NOT start a queue from: `None` sends it to the SQL-order
    /// fallback instead of playing the wrong track.
    #[test]
    fn clicked_row_outside_the_visible_list_is_none() {
        let rows = vec![track(1, "a"), track(2, "b")];
        assert!(order_by_visible(&rows, &ids(&["2"]), 1).is_none());
        assert!(order_by_visible(&rows, &[], 1).is_none());
    }

    /// LocalLibrary path: the hint IS the raw rating key (`local_queue_track`
    /// stamps `file_path`, which for a Plex row is the server key). Unchanged
    /// by the guard — `playback.rs:673-676` first arm.
    #[test]
    fn raw_hint_is_used_verbatim() {
        assert_eq!(plex_rating_key(Some("12345"), 999), "12345");
        // Non-numeric server keys are legal and must survive untouched.
        assert_eq!(
            plex_rating_key(Some("/library/metadata/771"), 999),
            "/library/metadata/771"
        );
    }

    /// PARITY-DEBT #4: the MyQBZ collections path stamps the ALBUM boundary key
    /// (`qbz_plex::plex_album_key` -> `plex:<hash>`). It is not a rating key, so
    /// it is ignored in favour of the numeric queue id — the `b5c1a76e` fix.
    #[test]
    fn plex_prefixed_hint_falls_back_to_queue_id() {
        assert_eq!(plex_rating_key(Some("plex:deadbeef"), 771), "771");
        // Prefix test is on the LEADING bytes only, exactly like `starts_with`.
        assert_eq!(plex_rating_key(Some("plex:"), 42), "42");
        assert_eq!(plex_rating_key(Some("77plex:1"), 42), "77plex:1");
    }

    /// The reference's `_` arm also covers a missing hint: fall back rather than
    /// refuse the row.
    #[test]
    fn missing_hint_falls_back_to_queue_id() {
        assert_eq!(plex_rating_key(None, 771), "771");
    }
}

// ---------------------------------------------------------------------------
// Source-aware single-track play (Home's Recently-Played rail, 2026-08-13)
// ---------------------------------------------------------------------------

/// Play ONE non-Qobuz track by its queue id, resolving it through its own
/// source. Returns false when the row cannot be resolved.
///
/// WHY THIS EXISTS. `playback_qt::queue_track_for` has exactly two arms — the
/// Qobuz favourites feed, then `get_track` against the catalog — so any id that
/// misses the feed goes to the Qobuz API. For a local or Plex id that is a
/// guaranteed 404 (reported by the owner from Discover's Recently-Played rail:
/// `get_track(1099511673001)` on a `PLEX_TRACK_ID_FLOOR`-namespaced id). The
/// cortinilla has routed by source since it was written and says why in its own
/// comment: handing a local id to the catalog "would 404 or, worse, open
/// someone else's album that happens to share the number". This is that seam
/// for the rails that reach playback through the generic by-id entry.
///
/// ROUTING IS BY THE ROW'S `source`, NEVER BY ID ARITHMETIC. `id >=
/// PLEX_TRACK_ID_FLOOR` is only unambiguous for Plex; plain local ids are small
/// `library.db` rowids that CAN collide with real Qobuz track ids, so a
/// speculative `get_track()` on the local DB could play the wrong file. It is
/// also what keeps Jellyfin/Navidrome a match arm here instead of another sweep
/// through the tree.
///
/// An unresolvable row plays NOTHING and says so. Falling back to the Qobuz
/// path would reproduce the 404 this exists to remove, and falling back to
/// "some other track" is the failure the cortinilla explicitly refuses.
pub async fn play_single_from_source(runtime: &Runtime, track_id: u64, source: &str) -> bool {
    let track = match source {
        // A library row: the DB has everything, and `local_queue_track` builds
        // the QueueTrack the audible step expects.
        "local" | "qobuz_download" => {
            tokio::task::spawn_blocking(move || with_db(|db| db.get_track(track_id as i64)).flatten())
                .await
                .ok()
                .flatten()
        }
        // A Plex row is SYNTHETIC — `local_plex::map_cached_to_local_track`
        // mints it and it never touches library.db, so there is no row to read.
        // The cache is queried by ALBUM, and the recently-played entry carries
        // the album artist/title, which is exactly the key. Resolving through
        // the real cached row is what recovers `rating_key` (the audible step
        // needs it as `source_item_id_hint`, and it is NOT derivable from the
        // namespaced id: the id packs the cache ROWID, the hint is a different
        // column).
        "plex" => {
            let entry = crate::recently_qt::find_track(&track_id.to_string());
            let Some(entry) = entry else {
                log::warn!("[qbz-qt] plex play: {track_id} is not in the recently-played store");
                return false;
            };
            let key = crate::local_plex::album_key_for(&entry.album_artist, &entry.album_title);
            tokio::task::spawn_blocking(move || {
                crate::local_plex::album_tracks(&key)
                    .into_iter()
                    .find(|t| t.id as u64 == track_id)
            })
            .await
            .ok()
            .flatten()
        }
        other => {
            log::warn!("[qbz-qt] play: unknown source {other:?} for track {track_id}");
            return false;
        }
    };
    let Some(track) = track else {
        log::warn!("[qbz-qt] play: {source} track {track_id} could not be resolved");
        return false;
    };
    play_rows(runtime, vec![track], 0, false).await;
    true
}
