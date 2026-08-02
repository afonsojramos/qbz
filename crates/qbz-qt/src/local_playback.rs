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
    match qbz_plex::plex_resolve_track_media(cfg.base_url, cfg.token, rating_key.clone()).await {
        Ok(media) => {
            if let Err(e) = runtime.core().player().play_data(media.bytes, play_id) {
                log::error!("[qbz-qt] plex play: play_data {play_id} failed: {e}");
                return false;
            }
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
async fn play_rows(runtime: &Runtime, tracks: Vec<LocalTrack>, start: usize, shuffle: bool) {
    if tracks.is_empty() {
        return;
    }
    let queue: Vec<QueueTrack> = tracks.iter().map(local_queue_track).collect();
    let start = start.min(queue.len() - 1);
    let first = queue[start].clone();
    if shuffle {
        runtime.core().set_shuffle(true).await;
        crate::now_playing::set_shuffle(true);
    }
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
    let tracks: Vec<LocalTrack> = tokio::task::spawn_blocking(move || match k.as_str() {
        "track" => ident
            .parse::<i64>()
            .ok()
            .and_then(find_track_blocking)
            .into_iter()
            .collect(),
        "folder" => with_db(|db| db.list_folder_tracks_recursive(&ident, false)).unwrap_or_default(),
        _ => fetch_album_tracks_blocking(&ident),
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
