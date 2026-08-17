//! Offline-cache actions for the Qt frontend — port of the Slint
//! `crates/qbz/src/offline_cache.rs` action set (cache_track /
//! cache_tracks / cache_album / redownload_album / remove_cached /
//! refresh_cached) with the bridge-signal row sink instead of the Slint
//! event-loop hop.
//!
//! All download machinery is the shared `qbz_offline_cache` crate (CMAF-first
//! downloader, 3-permit semaphore, retry backoff, vault); this module is the
//! frontend glue: pre-flight the limit, insert the queued row, push row
//! status, spawn, toast. Offline copies are always fetched at the top
//! quality tier (reference rule, `offline_cache.rs:138-156`).
//!
//! Row status vocabulary (Slint TrackRow.slint:639-702):
//!   0 = none · 1 = queued · 2 = downloading · 3 = ready · 4 = failed
//! Status 3 rows surface `trackCacheStatusChanged` so any listening view can
//! flip its glyph live; views also seed from `offline_qt::is_cached` at
//! document build time.

use std::sync::Arc;

use qbz_offline_cache::{CacheEvent, CacheEventSink, TrackCacheInfo};
use qbz_offline_cache::OfflineCacheState;

use crate::offline_qt;
use crate::shell_bridge;
use cxx_qt_lib::QString;

/// Push a row's cache status onto every listening view (the Qt fan-out of
/// Slint's `set_row_cache_status`). QML Connections handlers patch the row
/// inside their own document copy.
fn push_status(track_id: u64, status: i32, progress: f64) {
    let id = track_id.to_string();
    shell_bridge::ui(move |mut b| {
        b.as_mut().track_cache_status_changed(
            QString::from(id.as_str()),
            status,
            progress,
        );
    });
}

/// Build a sink that reflects cache + unlock events onto visible rows and
/// surfaces terminal toasts (Slint `row_sink`). Shared by the cache triggers
/// AND the play path (UnlockStart/End → the row padlock in Slint; the Qt
/// v1 does not draw the padlock — the events are still sunk here so the
/// status model stays truthful).
pub fn row_sink() -> CacheEventSink {
    Arc::new(move |ev: CacheEvent| match ev {
        CacheEvent::Started { track_id } => {
            push_status(track_id, 2, 0.0);
        }
        CacheEvent::Progress {
            track_id,
            progress_percent,
            ..
        } => {
            let p = (progress_percent as f64 / 100.0).clamp(0.0, 1.0);
            push_status(track_id, 2, p);
        }
        CacheEvent::Completed { track_id, .. } => {
            offline_qt::mark_cached(track_id, true);
            push_status(track_id, 3, 1.0);
            crate::toast_qt::success(qbz_i18n::t("Cached for offline"));
        }
        CacheEvent::Processed { .. } => {
            // Post-processing done; status already 'ready' from Completed.
        }
        CacheEvent::Failed { track_id, error } => {
            log::warn!("[qbz-qt] offline cache failed for {track_id}: {error}");
            push_status(track_id, 4, 0.0);
            crate::toast_qt::error(qbz_i18n::t("Offline caching failed"));
        }
        CacheEvent::UnlockStart { .. } | CacheEvent::UnlockEnd { .. } => {
            // The padlock row state is not drawn in the Qt port yet.
        }
    })
}

/// Build the DB row metadata from a catalog track (Slint `track_cache_info`).
///
/// `album_fallback` is `(id, title)` of the album this batch came FROM, used
/// only when the track carries no nested album of its own.
///
/// THAT IS NOT AN EDGE CASE — it is the album path's normal state. A `Track`
/// inside an `/album/get` payload has no nested `album` object (the envelope
/// would be repeating itself), so `cache_album` produced rows with
/// `album = NULL, album_id = NULL` for every track it ever queued. Measured on
/// a real index: 34 of 46 rows, three complete albums, all NULL — everything
/// downloaded through the album button since the CMAF path landed.
///
/// The damage is not cosmetic. `album_id` is the key `remove_album`,
/// `redownload_album` and the downloaded-album registry all look rows up by,
/// so an album cached this way could not be removed, refreshed or recognised
/// as downloaded, and the Offline Manager could not group it. The reference
/// has the identical bug (`offline_cache.rs:149-150` + `:311`).
fn track_cache_info(
    track: &qbz_models::Track,
    album_fallback: Option<&(String, String)>,
) -> TrackCacheInfo {
    let (album, album_id) = match track.album.as_ref() {
        Some(a) => (Some(a.title.clone()), Some(a.id.clone())),
        None => match album_fallback {
            Some((id, title)) => (Some(title.clone()), Some(id.clone())),
            None => (None, None),
        },
    };
    TrackCacheInfo {
        track_id: track.id,
        title: track.title.clone(),
        artist: track
            .performer
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        album,
        album_id,
        duration_secs: track.duration as u64,
        quality: "UltraHiRes".to_string(),
        bit_depth: track.maximum_bit_depth,
        sample_rate: track.maximum_sampling_rate,
    }
}

fn spawn_download(off: &Arc<OfflineCacheState>, track_id: u64) {
    let file_path = off.track_file_path(track_id, "flac");
    qbz_offline_cache::spawn_track_cache_download(
        track_id,
        file_path,
        crate::app().core().client(),
        off.fetcher.clone(),
        off.db.clone(),
        off.get_cache_path(),
        off.library_db.clone(),
        row_sink(),
        off.cache_semaphore.clone(),
    );
}

/// Cache a single track for offline playback (Slint `cache_track`).
pub fn cache_track(id: u64) {
    crate::spawn(async move {
        let Some(off) = offline_qt::get().await else {
            crate::toast_qt::error(qbz_i18n::t("Log in to cache tracks offline"));
            return;
        };
        let runtime = crate::app();
        let track = match runtime.core().get_track(id).await {
            Ok(t) => t,
            Err(e) => {
                log::error!("[qbz-qt] cache: get_track {id} failed: {e}");
                crate::toast_qt::error(qbz_i18n::t("Couldn't load that track"));
                return;
            }
        };
        // A `/track/get` payload DOES carry its album, so no fallback here.
        let info = track_cache_info(&track, None);
        let file_path = off.track_file_path(id, "flac");
        let file_path_str = file_path.to_string_lossy().to_string();

        // Pre-flight the cache limit, then insert the queued row.
        {
            let limit = *off.limit_bytes.lock().await;
            let guard = off.db.lock().await;
            let Some(db) = guard.as_ref() else {
                return;
            };
            let root = std::path::PathBuf::from(off.get_cache_path());
            if let Err(e) = qbz_offline_cache::maintenance::check_cache_limit(db, &root, limit) {
                log::warn!("[qbz-qt] cache limit reached: {e}");
                crate::toast_qt::error(
                    qbz_i18n::t("Offline cache is full — free space or raise the limit"),
                );
                return;
            }
            if let Err(e) = db.insert_track(&info, &file_path_str) {
                log::error!("[qbz-qt] cache insert {id} failed: {e}");
                return;
            }
        }

        push_status(id, 1, 0.0);
        spawn_download(&off, id);
    });
}

/// Cache a batch of already-fetched catalog tracks (album flow, multi-select
/// bulk "Make available offline"; Slint `cache_tracks`).
pub fn cache_tracks(tracks: Vec<qbz_models::Track>) {
    cache_tracks_with_album(tracks, None)
}

/// [`cache_tracks`] for a batch that came out of ONE album document, whose
/// `(album_id, album_title)` stamps every row that has no nested album of its
/// own — see `track_cache_info`.
pub fn cache_tracks_with_album(
    tracks: Vec<qbz_models::Track>,
    album_fallback: Option<(String, String)>,
) {
    if tracks.is_empty() {
        return;
    }
    crate::spawn(async move {
        let Some(off) = offline_qt::get().await else {
            crate::toast_qt::error(qbz_i18n::t("Log in to cache tracks offline"));
            return;
        };
        // Pre-flight once for the whole batch (mirrors the reference).
        {
            let limit = *off.limit_bytes.lock().await;
            let guard = off.db.lock().await;
            let Some(db) = guard.as_ref() else {
                return;
            };
            let root = std::path::PathBuf::from(off.get_cache_path());
            if let Err(e) = qbz_offline_cache::maintenance::check_cache_limit(db, &root, limit) {
                log::warn!("[qbz-qt] batch cache limit reached: {e}");
                crate::toast_qt::error(
                    qbz_i18n::t("Offline cache is full — free space or raise the limit"),
                );
                return;
            }
        }
        let count = tracks.len();
        for track in &tracks {
            let id = track.id;
            let info = track_cache_info(track, album_fallback.as_ref());
            let file_path = off.track_file_path(id, "flac");
            let file_path_str = file_path.to_string_lossy().to_string();
            {
                let guard = off.db.lock().await;
                let Some(db) = guard.as_ref() else {
                    return;
                };
                if db.insert_track(&info, &file_path_str).is_err() {
                    continue;
                }
            }
            push_status(id, 1, 0.0);
            spawn_download(&off, id);
        }
        crate::toast_qt::success(qbz_i18n::tf(
            "Caching {} track offline…",
            "Caching {} tracks offline…",
            count as i64,
            &[&count.to_string()],
        ));
    });
}

/// Cache a whole album for offline playback (Slint `cache_album`): fetch its
/// tracks, then batch them.
pub fn cache_album(album_id: String) {
    crate::spawn(async move {
        let runtime = crate::app();
        let album = match runtime.core().get_album(&album_id).await {
            Ok(a) => a,
            Err(e) => {
                log::error!("[qbz-qt] cache: get_album {album_id} failed: {e}");
                crate::toast_qt::error(qbz_i18n::t("Couldn't load that album"));
                return;
            }
        };
        let tracks: Vec<qbz_models::Track> = album
            .tracks
            .map(|c| c.items)
            .unwrap_or_default();
        if tracks.is_empty() {
            crate::toast_qt::error(qbz_i18n::t("This album has no playable tracks"));
            return;
        }
        // Stamp the album identity from the document we just fetched. Without
        // it every row lands with a NULL album_id and the album becomes
        // unremovable, unrefreshable and ungroupable — see `track_cache_info`.
        cache_tracks_with_album(tracks, Some((album.id.clone(), album.title.clone())));
    });
}

/// Re-download an album's offline copies (Slint `redownload_album`,
/// `failed_only = false` from the album menu's "Refresh offline copy").
pub fn redownload_album(album_id: String, failed_only: bool) {
    crate::spawn(async move {
        let Some(off) = offline_qt::get().await else {
            return;
        };
        let targets: Vec<u64> = {
            let guard = off.db.lock().await;
            let Some(db) = guard.as_ref() else {
                return;
            };
            match db.get_album_tracks(&album_id) {
                Ok(tracks) => {
                    let picked = qbz_offline_cache::maintenance::select_redownload_targets(
                        &tracks,
                        failed_only,
                    );
                    let ids: Vec<u64> = picked.iter().map(|t| t.track_id).collect();
                    for id in &ids {
                        let _ = db.reset_track_for_redownload(*id);
                    }
                    ids
                }
                Err(_) => Vec::new(),
            }
        };
        for id in targets {
            push_status(id, 1, 0.0);
            spawn_download(&off, id);
        }
        crate::offline_manager_qt::refresh_if_open().await;
    });
}

/// Re-download ONE track (Slint `redownload_track`): reset its row and spawn
/// the download, skipping a copy that is already in flight.
///
/// Distinct from `refresh_cached` below, which DELETES the copy first: this
/// keeps the row (and its place in the index) and re-fetches into it, which is
/// what the manager's per-row refresh and its bulk arm want — a failed row has
/// nothing on disk to delete, and deleting a ready one would drop the user's
/// only copy if the network is gone by the time the fetch runs.
pub fn redownload_track(id: u64) {
    crate::spawn(async move {
        let Some(off) = offline_qt::get().await else {
            return;
        };
        {
            let guard = off.db.lock().await;
            let Some(db) = guard.as_ref() else {
                return;
            };
            if let Ok(Some(t)) = db.get_track(id) {
                if matches!(t.status, qbz_offline_cache::OfflineCacheStatus::Downloading) {
                    return;
                }
            }
            let _ = db.reset_track_for_redownload(id);
        }
        push_status(id, 1, 0.0);
        spawn_download(&off, id);
        crate::offline_manager_qt::refresh_if_open().await;
    });
}

/// Remove EVERY offline copy of an album (Slint `remove_album`): the shared
/// maintenance sweep does the DB rows + the on-disk bundles, then the library
/// rows and the session set follow it.
pub fn remove_album(album_id: String) {
    crate::spawn(async move {
        let Some(off) = offline_qt::get().await else {
            return;
        };
        let report = {
            let guard = off.db.lock().await;
            let Some(db) = guard.as_ref() else {
                return;
            };
            let root = std::path::PathBuf::from(off.get_cache_path());
            qbz_offline_cache::maintenance::remove_album_cached_tracks(db, &root, &album_id)
        };
        let report = match report {
            Ok(r) => r,
            Err(e) => {
                log::error!("[qbz-qt] remove album {album_id} failed: {e}");
                return;
            }
        };
        {
            let guard = off.library_db.lock().await;
            if let Some(db) = guard.as_ref() {
                for id in &report.removed_track_ids {
                    let _ = db.remove_qobuz_cached_track(*id);
                }
            }
        }
        for id in &report.removed_track_ids {
            offline_qt::mark_cached(*id, false);
            push_status(*id, 0, 0.0);
        }
        crate::toast_qt::success(qbz_i18n::t("Removed album from offline"));
        crate::offline_manager_qt::refresh_if_open().await;
    });
}

/// Open the cache directory in the desktop file manager (Slint `open_folder`).
///
/// `xdg-open` / `open` by hand rather than the `rfd` used elsewhere in the
/// port: rfd opens a PICKER, and this row is "show me the folder". Same two
/// binaries the reference shells out to.
pub fn open_folder() {
    crate::spawn(async move {
        let Some(off) = offline_qt::get().await else {
            return;
        };
        let path = off.get_cache_path();
        let opener = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        if let Err(e) = std::process::Command::new(opener).arg(&path).spawn() {
            log::warn!("[qbz-qt] open offline folder failed: {e}");
        }
    });
}

/// Clear the WHOLE offline cache — DB rows, on-disk bundles and the library
/// rows (Slint `clear_all`).
///
/// The Settings copy is precise about the blast radius and so is this: it
/// removes the cached AUDIO. Purchased downloads live in the user's own music
/// folder and are not touched by `purge_all_cached_files`.
pub fn clear_all() {
    crate::spawn(async move {
        let Some(off) = offline_qt::get().await else {
            return;
        };
        if let Err(e) = qbz_offline_cache::purge_all_cached_files(&off, &off.library_db).await {
            log::error!("[qbz-qt] clear offline cache failed: {e}");
            crate::toast_qt::error(qbz_i18n::t("Couldn't clear the cache"));
            return;
        }
        offline_qt::clear_cached_ids();
        crate::toast_qt::success(qbz_i18n::t("Cache cleared"));
        crate::offline_manager_qt::refresh_if_open().await;
    });
}

/// Remove a track's offline copy (Slint `remove_cached`).
pub fn remove_cached(id: u64) {
    crate::spawn(async move {
        remove_cached_inner(id, true).await;
        crate::offline_manager_qt::refresh_if_open().await;
    });
}

/// Refresh a track's offline copy (Slint `refresh_cached`): delete + re-queue
/// sequenced in ONE task — `insert_track` is not an upsert, so the delete
/// must land first.
pub fn refresh_cached(id: u64) {
    crate::spawn(async move {
        remove_cached_inner(id, false).await;
        cache_track(id);
    });
}

/// The removal body shared by `remove_cached` and `refresh_cached` (Slint
/// `remove_cached_inner`): DB row + on-disk bundle/file + library row.
async fn remove_cached_inner(id: u64, toast: bool) {
    let Some(off) = offline_qt::get().await else {
        return;
    };
    let removed_path = {
        let guard = off.db.lock().await;
        match guard.as_ref() {
            Some(db) => db.delete_track(id).ok().flatten(),
            None => return,
        }
    };
    if let Some(p) = removed_path {
        let path = std::path::Path::new(&p);
        // v2 bundles live in `tracks-cmaf/<id>/` — remove the whole dir.
        let looks_v2 = path
            .parent()
            .and_then(|pp| pp.parent())
            .and_then(|r| r.file_name())
            .and_then(|n| n.to_str())
            == Some("tracks-cmaf");
        if looks_v2 {
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    {
        let guard = off.library_db.lock().await;
        if let Some(db) = guard.as_ref() {
            let _ = db.remove_qobuz_cached_track(id);
        }
    }
    offline_qt::mark_cached(id, false);
    push_status(id, 0, 0.0);
    if toast {
        crate::toast_qt::success(qbz_i18n::t("Removed from offline"));
    }
}
