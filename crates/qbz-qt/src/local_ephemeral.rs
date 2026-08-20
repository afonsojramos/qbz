//! Ephemeral folders — open a folder OUTSIDE the indexed library, browse it
//! and play it without a single row landing in `library.db`.
//!
//! Qt/QML port of the shipping Slint pair (`crates/qbz/src/ephemeral.rs` +
//! the `EphemeralPane` arm of `crates/qbz/src/local_library.rs`). ADR-006:
//! NOTHING here re-implements scanning, CUE explosion, artwork extraction or
//! album grouping — all of that is `qbz_library::ephemeral`, the same
//! frontend-agnostic module the Slint binary drives. This file owns three
//! things only:
//!
//!  1. the process-global session handle (in-memory, dies with the process);
//!  2. the ONE JSON document the QML pane parses
//!     (`{name, path, trackCount, multiAlbum, albums:[…]}`), built out of the
//!     SAME `local_rows::TrackRow` every other Local Library surface uses;
//!  3. the play seam — the queue is mapped by `local_playback`'s
//!     `local_queue_track` and handed to the SHARED audible step
//!     (`audible_qt`, design 02 §9 stage 3). This file used to carry its own
//!     `play_file`, described in its own doc as "`local_playback::
//!     play_local_file` 1:1 apart from that lookup"; the lookup is now
//!     `LocalSource::track_row`, which reads this session store for any
//!     ephemeral id, so there is no ephemeral arm anywhere in playback. The
//!     PROTECTED audio path is entered there, never modified.
//!
//! Ephemeral track ids are SYNTHETIC and high (`>= 2^48`, the shared
//! `EPHEMERAL_ID_FLOOR`), so they can never collide with a `local_tracks` row
//! id: `is_ephemeral_id` is what lets any playback caller route an id to the
//! in-memory store instead of the DB.
//!
//! What SURVIVES a restart is the folder PATH only (`locallibrary_ui.json`'s
//! `ephemeral_folder`, shared with the Slint frontend); `rehydrate()` re-scans
//! it at boot.
//!
//! POC-NOTE (folder picker): this port has no `rfd` dependency and cxx-qt-lib
//! 0.7 exposes no QFileDialog, so `open()` shells out to the desktop's own
//! folder chooser (zenity / qarma / kdialog / yad — the first one present).
//! Swapping in `rfd::AsyncFileDialog` (what the Slint uses) is a five-line
//! change to `pick_folder_blocking` once the dependency exists.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use cxx_qt_lib::QString;
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_library::ephemeral::{EphemeralLibraryState, EPHEMERAL_ID_FLOOR};
use qbz_library::LocalTrack;
use qbz_models::QueueTrack;
use serde::Serialize;

use crate::local_bridge::ui;
use crate::local_rows::{album_key, map_track, to_json, TrackRow};
use crate::local_state::{read_prefs, with_art, write_prefs};

type Runtime = Arc<AppRuntime<LoggingAdapter>>;

// ---------------------------------------------------------------------------
// Session store (the Slint `crates/qbz/src/ephemeral.rs` 1:1)
// ---------------------------------------------------------------------------

// Every method on `EphemeralLibraryState` takes `&self` and guards its own
// inner Mutex, so a bare LazyLock (no outer lock) is enough.
static STATE: LazyLock<EphemeralLibraryState> = LazyLock::new(EphemeralLibraryState::new);

/// True when `id` is a synthetic ephemeral track id. DB row ids never reach
/// the floor, so this check unambiguously routes a playback request to the
/// in-memory store instead of `library.db`.
pub fn is_ephemeral_id(id: i64) -> bool {
    id >= EPHEMERAL_ID_FLOOR
}

/// Resolve a synthetic id to its cached row (None once the session is gone).
pub fn get_track(id: i64) -> Option<LocalTrack> {
    STATE.get_track(id)
}

/// Every track of the current session, in scan (= display) order.
pub fn tracks_snapshot() -> Vec<LocalTrack> {
    STATE.tracks_snapshot()
}

/// The album grouping key for one ephemeral row — `album_group_key` when set,
/// else `album|||album_artist`. Mirrors the scanner's own fallback so the
/// pane's grouping and the play-album lookup can never disagree.
fn album_key_of(t: &LocalTrack) -> String {
    if !t.album_group_key.is_empty() {
        t.album_group_key.clone()
    } else {
        format!(
            "{}|||{}",
            t.album,
            t.album_artist.as_deref().unwrap_or(&t.artist)
        )
    }
}

/// The tracks of one album block, in scan order.
fn album_tracks(group_key: &str) -> Vec<LocalTrack> {
    STATE
        .tracks_snapshot()
        .into_iter()
        .filter(|t| album_key_of(t) == group_key)
        .collect()
}

// ---------------------------------------------------------------------------
// The document (the QML contract)
// ---------------------------------------------------------------------------

/// `{name, path, trackCount, multiAlbum, albums:[…]}` — the whole pane in one
/// publish, exactly like every other Local Library surface.
#[derive(Serialize)]
struct EphemeralDoc {
    name: String,
    path: String,
    #[serde(rename = "trackCount")]
    track_count: usize,
    /// > 1 album in the folder: the per-album play button appears only then.
    #[serde(rename = "multiAlbum")]
    multi_album: bool,
    albums: Vec<EphemeralAlbumBlock>,
}

/// One album block: header (cover + title/artist/meta + CUE badge) + tracks.
#[derive(Serialize)]
struct EphemeralAlbumBlock {
    #[serde(rename = "groupKey")]
    group_key: String,
    title: String,
    artist: String,
    /// "1998 · 12 tracks" (already formatted + translated).
    meta: String,
    /// Single-file CUE rip (one audio file, virtual tracks).
    #[serde(rename = "isCue")]
    is_cue: bool,
    #[serde(rename = "artKey")]
    art_key: String,
    tracks: Vec<TrackRow>,
}

/// Last path segment (the header's folder name).
fn folder_display_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Group the scanned rows into album blocks (sorted by title) and register
/// every cover in the shared art index, so the pane's covers ride the SAME
/// windowed `artworkWindow` -> `localArtworkReady` path as the grids.
/// Cheap (no I/O) but it takes the local-state lock — never call it on the Qt
/// thread.
fn build_doc(name: &str, path: &str, tracks: &[LocalTrack]) -> EphemeralDoc {
    // BTreeMap keeps a deterministic key order; scan order is preserved
    // inside each group (the scanner already sorted album/disc/track/title).
    let mut groups: BTreeMap<String, Vec<&LocalTrack>> = BTreeMap::new();
    for t in tracks {
        groups.entry(album_key_of(t)).or_default().push(t);
    }
    let multi_album = groups.len() > 1;
    let mut albums: Vec<EphemeralAlbumBlock> = with_art(|art| {
        groups
            .into_iter()
            .map(|(key, group)| {
                let first = group[0];
                let title = if first.album_group_title.is_empty() {
                    first.album.clone()
                } else {
                    first.album_group_title.clone()
                };
                let artist = first
                    .album_artist
                    .clone()
                    .unwrap_or_else(|| first.artist.clone());
                let count = group.len();
                let tracks_label =
                    qbz_i18n::tf("{} track", "{} tracks", count as i64, &[&count.to_string()]);
                let meta = match first.year {
                    Some(y) if y > 0 => format!("{y} · {tracks_label}"),
                    _ => tracks_label,
                };
                // Namespaced so an ephemeral block can never claim the art
                // key of an indexed album that happens to group the same.
                let art_key = album_key(&format!("eph:{key}"));
                if let Some(p) = first.artwork_path.as_ref().filter(|p| !p.is_empty()) {
                    art.insert(art_key.clone(), p.clone());
                }
                let rows: Vec<TrackRow> = group.iter().map(|t| map_track(*t, art)).collect();
                EphemeralAlbumBlock {
                    group_key: key,
                    title,
                    artist,
                    meta,
                    is_cue: first.cue_file_path.is_some() || first.cue_start_secs.is_some(),
                    art_key,
                    tracks: rows,
                }
            })
            .collect()
    });
    albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    EphemeralDoc {
        name: name.to_string(),
        path: path.to_string(),
        track_count: tracks.len(),
        multi_album,
        albums,
    }
}

// ---------------------------------------------------------------------------
// Publishing
// ---------------------------------------------------------------------------

fn publish_doc(doc: &EphemeralDoc, loading: bool) {
    let json = to_json(doc);
    ui(move |mut b| {
        b.as_mut().set_local_ephemeral_active(true);
        b.as_mut()
            .set_local_ephemeral_json(QString::from(json.as_str()));
        b.as_mut().set_local_ephemeral_loading(loading);
    });
}

/// Back to the closed state (`""` parses to null QML-side, which collapses the
/// pane and restores the normal flat/tree browse).
fn publish_closed() {
    ui(|mut b| {
        b.as_mut().set_local_ephemeral_active(false);
        b.as_mut().set_local_ephemeral_loading(false);
        b.as_mut().set_local_ephemeral_json(QString::from(""));
    });
}

/// Persist / forget the folder PATH (the session itself never persists). The
/// same `locallibrary_ui.json` key the Slint frontend writes.
fn save_path(path: Option<&str>) {
    let mut prefs = read_prefs();
    prefs.ephemeral_folder = path.map(|p| p.to_string());
    write_prefs(&prefs);
}

// ---------------------------------------------------------------------------
// Open / clear
// ---------------------------------------------------------------------------

/// Toolbar button: native folder picker -> scan -> pane.
pub fn open() {
    crate::spawn(async move {
        let picked = tokio::task::spawn_blocking(pick_folder_blocking)
            .await
            .ok()
            .flatten();
        let Some(path) = picked else {
            return; // cancelled, or no picker on this desktop (logged below).
        };
        scan(Some(crate::app()), path).await;
    });
}

/// Open a KNOWN path (no picker) — the seam a drag-drop or a CLI argument
/// would use, and what `open()` degrades to on a desktop with no chooser.
pub fn open_path(path: String) {
    if path.is_empty() {
        return;
    }
    crate::spawn(async move {
        scan(Some(crate::app()), path).await;
    });
}

/// Boot: re-open the persisted folder, if any. Silent — a folder that moved
/// away just clears the stale pref.
pub fn rehydrate() {
    let Some(path) = read_prefs().ephemeral_folder.filter(|p| !p.is_empty()) else {
        return;
    };
    crate::spawn(async move {
        scan(None, path).await;
    });
}

/// Clear button: drop the pane, the in-memory store and the persisted path.
pub fn clear() {
    let runtime = crate::app();
    crate::spawn(async move {
        wipe_if_playing(&runtime).await;
        STATE.clear();
        save_path(None);
        publish_closed();
    });
}

/// The scan body shared by the picker, an explicit path and the boot
/// rehydrate. When a runtime is given (a user-driven open), a currently
/// playing ephemeral track is wiped FIRST — the next session reuses its
/// synthetic ids, so a survivor would false-highlight a row in the new folder.
async fn scan(runtime: Option<Runtime>, path: String) {
    if let Some(rt) = &runtime {
        wipe_if_playing(rt).await;
    }
    let name = folder_display_name(&path);
    // Header + spinner immediately; the scan of a big folder is not instant.
    publish_doc(
        &EphemeralDoc {
            name: name.clone(),
            path: path.clone(),
            track_count: 0,
            multi_album: false,
            albums: Vec::new(),
        },
        true,
    );
    let scan_path = path.clone();
    let result = tokio::task::spawn_blocking(move || STATE.open_folder(Path::new(&scan_path))).await;
    match result {
        Ok(Ok(res)) => {
            log::info!(
                "[qbz-qt] ephemeral opened {} ({} tracks, {} skipped)",
                path,
                res.tracks.len(),
                res.skipped_files
            );
            publish_doc(&build_doc(&name, &path, &res.tracks), false);
            save_path(Some(&path));
        }
        Ok(Err(e)) => {
            log::warn!("[qbz-qt] ephemeral open failed: {e}");
            STATE.clear();
            save_path(None);
            publish_closed();
        }
        Err(e) => {
            log::warn!("[qbz-qt] ephemeral scan task failed: {e}");
            publish_closed();
        }
    }
}

/// Stop + drop the queue when what is playing came from the session being
/// replaced or cleared (the Slint's `wipeEphemeralPlaybackArtifacts`).
async fn wipe_if_playing(runtime: &Runtime) {
    let is_eph = runtime
        .core()
        .current_track()
        .await
        .map(|t| is_ephemeral_id(t.id as i64))
        .unwrap_or(false);
    if !is_eph {
        return;
    }
    // Drops everything INCLUDING the current track, stops, republishes the
    // queue and the now-playing chrome.
    crate::queue_qt::clear_queue(runtime).await;
}

// ---------------------------------------------------------------------------
// Folder picker (POC-NOTE at the top of the file)
// ---------------------------------------------------------------------------

/// Ask the desktop for a folder. Returns None on cancel AND when no chooser is
/// installed (logged, so a dead button is never silent). BLOCKING — the caller
/// runs it on `spawn_blocking`.
fn pick_folder_blocking() -> Option<String> {
    let title = qbz_i18n::t("Choose a folder to play");
    let start = dirs::audio_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("/"));
    let start = start.to_string_lossy().into_owned();
    // zenity/qarma go through the XDG portal on modern desktops; kdialog is
    // the KDE native; yad is the last resort.
    let candidates: [(&str, Vec<String>); 4] = [
        (
            "zenity",
            vec![
                "--file-selection".into(),
                "--directory".into(),
                format!("--title={title}"),
                format!("--filename={start}/"),
            ],
        ),
        (
            "qarma",
            vec![
                "--file-selection".into(),
                "--directory".into(),
                format!("--title={title}"),
                format!("--filename={start}/"),
            ],
        ),
        (
            "kdialog",
            vec![
                "--getexistingdirectory".into(),
                start.clone(),
                "--title".into(),
                title.clone(),
            ],
        ),
        (
            "yad",
            vec![
                "--file".into(),
                "--directory".into(),
                format!("--title={title}"),
            ],
        ),
    ];
    for (bin, args) in candidates {
        match std::process::Command::new(bin).args(&args).output() {
            // The chooser ran: a path on stdout, or an empty/failed exit =
            // the user cancelled. Either way we stop looking.
            Ok(out) => {
                if !out.status.success() {
                    return None;
                }
                let picked = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                return (!picked.is_empty()).then_some(picked);
            }
            // Not installed — try the next one.
            Err(_) => continue,
        }
    }
    log::error!(
        "[qbz-qt] ephemeral: no folder chooser found (zenity / qarma / kdialog / yad) — \
         call ephemeralOpenPath(path) with a known folder instead"
    );
    None
}

// ---------------------------------------------------------------------------
// Playback (the local seam — synthetic ids resolve from the in-memory store)
// ---------------------------------------------------------------------------

/// Play-all header button: the whole folder becomes the queue, scan order.
pub async fn play_all(runtime: &Runtime) {
    play_rows(runtime, tracks_snapshot(), 0).await;
}

/// Per-album play (multi-album sessions only): that block becomes the queue.
pub async fn play_album(runtime: &Runtime, group_key: String) {
    play_rows(runtime, album_tracks(&group_key), 0).await;
}

/// Track row click: the track's ALBUM BLOCK becomes the queue, starting there.
pub async fn play_track(runtime: &Runtime, track_id: i64) {
    let Some(track) = get_track(track_id) else {
        log::warn!("[qbz-qt] ephemeral play: track {track_id} is not in the session");
        return;
    };
    let tracks = album_tracks(&album_key_of(&track));
    let start = tracks.iter().position(|t| t.id == track_id).unwrap_or(0);
    play_rows(runtime, tracks, start).await;
}

/// `set_queue` then the AUDIBLE step — the same order `local_playback`'s own
/// queue builders use, with the same `local_queue_track` mapping (so an
/// ephemeral row reaches the queue panel / now-playing bar identical to any
/// other local file).
async fn play_rows(runtime: &Runtime, tracks: Vec<LocalTrack>, start: usize) {
    if tracks.is_empty() {
        return;
    }
    let queue: Vec<QueueTrack> = tracks
        .iter()
        .map(crate::local_playback::local_queue_track)
        .collect();
    let start = start.min(queue.len() - 1);
    let first = queue[start].clone();
    runtime.core().set_queue(queue, Some(start)).await;
    crate::playback_qt::publish_queue(runtime).await;
    // THE shared audible step. This used to call a local `play_file` that was
    // "`local_playback::play_local_file` 1:1 apart from the lookup" — a third
    // copy of the same routine, and the only one whose CUE fast path compared
    // FILE PATHS instead of track ids (i.e. the only one that actually worked).
    // `LocalSource::track_row` reads the session store for an ephemeral id, so
    // the seam resolves these rows with no arm of their own, and `audible_qt`
    // keeps the path comparison for every source.
    if let Err(e) = crate::audible_qt::play_queue_track(runtime, &first).await {
        log::error!("[qbz-qt] ephemeral play: track {} not playable: {e}", first.id);
    }
    crate::playback_qt::refresh_now_playing(runtime).await;
}

