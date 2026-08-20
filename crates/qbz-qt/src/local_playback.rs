//! Local Library playback: `LocalTrack` -> `QueueTrack` -> `core().set_queue`
//! -> the player's audible seam.
//!
//! Split out of `local_library_qt.rs` (phase-24 modularization). A queue built
//! here can mix LOCAL files, EPHEMERAL rows, OFFLINE (Qobuz download) copies
//! and PLEX rows.
//!
//! THE AUDIBLE STEP IS NO LONGER HERE (design 02 §9 stage 3). This file used
//! to own `play_local_file`, `play_plex_track` and a `plex_rating_key` ladder,
//! and `local_album_actions` + `local_ephemeral` each carried their own copy of
//! the same routine. All three are gone: `qbz_source::SourceRegistry::playback`
//! claims the row and answers with a `PlaybackTicket`, and `audible_qt`
//! performs it. What is left here is queue BUILDING — mapping rows, ordering
//! them, stamping context — plus `play_audible`, which is now one call.
//!
//! The PROTECTED audio path is entered in `audible_qt`, never modified: the
//! bytes go to the same `play_data` seam the Slint frontend uses, so sample
//! rate / bit depth stay whatever the decoder found.

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
        // GENUINELY always true, and this is not a stub (contract §5.3 F5).
        // This row is a FILE ON DISK — a local scan, a Plex item, or a
        // `qobuz_download` — and Qobuz's streaming rights do not reach it.
        // `qbz-core` takes the offline tier BEFORE it asks any availability
        // question (core.rs:716), which is exactly why a track Qobuz PULLED
        // still plays from here. Marking these `false` would delete the user's
        // own downloads from the queue — a worse bug than the one D5 fixes, and
        // it would bite hardest in offline mode, where this is the only copy
        // that exists.
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

/// Route ONE queue track to its audible step — through `qbz-source`.
///
/// This used to `match track.source.as_deref()`, with a `Some("plex")` arm
/// carrying its own rating-key ladder (`plex_rating_key`, deleted with it) and
/// a `_` arm that handed `track.id` to `play_local_file`. Both were wrong in a
/// way only the seam can see:
///
/// - the `_` arm caught **offline** rows, whose `QueueTrack.id` is the *Qobuz*
///   catalog id while the `library.db` row id rides in `source_item_id_hint`
///   (`local_queue_track` above, `:161-167` and `:199-206`). It looked the row
///   up by the wrong number and missed. `LocalSource::row_id` decides that on
///   evidence instead;
/// - the `plex:`-prefixed-hint rule was one engineer's reconstruction of a
///   five-id table nobody had written down, and its fallback is right for the
///   MyQBZ path and wrong for the LocalLibrary one. `PlexSource` owns that
///   table now, and a shape it recognises but rejects is a named error at the
///   moment of the mistake rather than a 404 two layers down.
///
/// `SourceRegistry::playback` claims the row ONCE and answers with a ticket;
/// `audible_qt` performs it. No source is branched on by hand here any more.
async fn play_audible(runtime: &Runtime, track: &QueueTrack) -> bool {
    match crate::audible_qt::play_queue_track(runtime, track).await {
        Ok(played) => played,
        Err(e) => {
            log::error!("[qbz-qt] local play: track {} not playable: {e}", track.id);
            false
        }
    }
}

/// What the local/Plex audible step did with a queue row.
///
/// This used to be a `bool`, and the two `false`s meant OPPOSITE things: "this
/// row is not mine, go run the Qobuz walk" and "this row IS mine and it cannot
/// play". The caller could only treat both as the first, so a local file on a
/// share that had gone away fell through to the Qobuz tier-walk, failed there
/// with an unrelated error ("No Qobuz client available"), and
/// `auto_skip_unavailable` then declined to skip because that error is not
/// terminal-unavailable. Net effect: one unreachable local file stopped
/// playback dead, silently — the exact shape of "a track that cannot play must
/// never be blocking".
pub enum LocalPlay {
    /// Not a local/Plex row. The Qobuz path must run, unchanged.
    NotLocal,
    /// Playing.
    Played,
    /// Ours, and it cannot play. Carries a reason phrased so the shared skip
    /// walk recognises it as terminal (see `is_terminal_unavailable`).
    Unavailable(String),
}

/// Source-aware AUDIBLE step for the shared poll-loop advance.
///
/// Without this seam, auto-advance to the next local/Plex track goes down
/// `play_track_resolved` and fails with "No Qobuz client available".
pub async fn play_current_if_local(runtime: &Runtime, track_id: u64) -> LocalPlay {
    let Some(qt) = runtime.core().current_track().await else {
        return LocalPlay::NotLocal;
    };
    if qt.id != track_id {
        return LocalPlay::NotLocal;
    }
    // WHO OWNS THIS ROW is asked ONCE, of the registry (design 02 §3.1's
    // vocabulary table), instead of matched against two hand-written words.
    //
    // The old test was `Some("local") | Some("plex")`, and it is IC-12: six
    // source words really occur on a queue row, and the four it missed —
    // `"user"`, `"ephemeral"`, `"qobuz_download"`, `"qobuz_purchase"` — all
    // name rows that are FILES ON DISK. Auto-advance onto one of them returned
    // `NotLocal`, fell through to the Qobuz tier-walk and died there with "No
    // Qobuz client available": an error about a service that has nothing to do
    // with the track, which `auto_skip_unavailable` then declines to skip
    // because it is not terminal-unavailable. One ephemeral track could stop
    // playback dead.
    //
    // `claim` refuses to guess, so a row nobody owns stays `NotLocal` and the
    // Qobuz path runs exactly as before.
    let claimed = qbz_source::registry().claim(&qbz_source::RawRef::from_queue_track(&qt));
    let ours = match claimed {
        Ok(item) => item.source() != qbz_source::SourceId::QOBUZ,
        Err(_) => false,
    };
    if !ours {
        return LocalPlay::NotLocal;
    }
    if play_audible(runtime, &qt).await {
        LocalPlay::Played
    } else {
        // The wording is load-bearing: `is_terminal_unavailable` matches on
        // "no longer available", which is what routes this into the SAME
        // bounded skip walk a pulled Qobuz track takes. One seam for both,
        // which is the point — to the user they are the same failure.
        LocalPlay::Unavailable(format!("local file no longer available (track {track_id})"))
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
    // Local rows are never dropped by the seam — `local_queue_track` sets
    // `streamable: true` (a file on disk is outside Qobuz's rights) and
    // `is_track_blacklisted` fails open for any source that is not "qobuz" — so
    // the anchor always comes back and always names `first`. Answered anyway
    // rather than discarded with a `let _`: the day a Qobuz row reaches this
    // path, this returns instead of trying to play a track the core dropped.
    if crate::playback_qt::set_queue_stamped(runtime, queue, Some(start), None)
        .await
        .is_none()
    {
        log::warn!("[qbz-qt] local play: the queue was filtered to nothing");
        return;
    }
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
    use super::order_by_visible;
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
        // The media servers resolve through the SEAM, not through a
        // `LocalTrack`: their rows live in the shared remote cache and have no
        // `library.db` row to mint one from. `registry.tracks_of` claims the
        // namespaced id and hands back a queue row directly, which is the
        // shape this function was building by hand for the other sources.
        "jellyfin" | "subsonic" | "navidrome" | "gonic" | "airsonic" | "astiga" => {
            let raw = qbz_source::RawRef {
                source: qbz_source::SourceId::from_word(source),
                kind: Some(qbz_source::ItemKind::Track),
                id: track_id.to_string(),
                is_local: Some(true),
                ..Default::default()
            };
            return match qbz_source::registry().tracks_of(&raw).await {
                Ok(tracks) if !tracks.is_empty() => {
                    let first = tracks[0].clone();
                    if crate::playback_qt::set_queue_stamped(runtime, tracks, Some(0), None)
                        .await
                        .is_none()
                    {
                        log::warn!("[qbz-qt] {source} play: the queue was filtered to nothing");
                        return false;
                    }
                    crate::playback_qt::publish_queue(runtime).await;
                    let played = play_audible(runtime, &first).await;
                    crate::playback_qt::refresh_now_playing(runtime).await;
                    played
                }
                Ok(_) => {
                    log::warn!("[qbz-qt] {source} play: track {track_id} resolved to nothing");
                    false
                }
                Err(e) => {
                    log::warn!("[qbz-qt] {source} play: track {track_id} not resolvable: {e}");
                    false
                }
            };
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
