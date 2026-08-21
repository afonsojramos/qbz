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
///
/// `pub(crate)` under a second name for `source_wiring::EphemeralGlue`: the
/// seam's `LocalSource` resolves an ephemeral album through this store rather
/// than through `library.db`, and it needs the same scan order the pane shows.
pub(crate) fn album_tracks_for(group_key: &str) -> Vec<LocalTrack> {
    album_tracks(group_key)
}

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
    /// Total playing time, already formatted ("1 h 12 min" / "42 min") by the
    /// SAME helper every other Local Library surface uses, so the pane header
    /// and an album card can never render the same duration two ways.
    #[serde(rename = "totalDuration")]
    total_duration: String,
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
                    if let Some(t) = crate::local_rows::art_token(first.source.as_deref(), p) {
                        art.insert(art_key.clone(), t);
                    }
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
        total_duration: crate::local_rows::total_duration(
            tracks.iter().map(|t| t.duration_secs).sum(),
        ),
        multi_album,
        albums,
    }
}

// ---------------------------------------------------------------------------
// Publishing
// ---------------------------------------------------------------------------

/// Longest tab label we will hand the segmented bar. `QbzTabBar` sizes a
/// segment to its text (`QbzTabBar.qml:47` implicitWidth), so an unbounded
/// folder name would push the whole bar off screen next to four one-word
/// siblings.
const LABEL_MAX: usize = 24;

/// What the tab and the nav flyout call this session — the name of the THING
/// that is open, never a verb. ("Now Playing" was considered and rejected: it
/// is false the moment a folder sits open while something else plays, and the
/// tab would then contradict the now-playing bar on the same screen.)
///
/// Most specific first: a single-album session is named by its album, because
/// that is the usual case — a folder IS an album, and a disc IS an album. A
/// multi-album folder, or one whose tags gave no title, falls back to the
/// medium's display name, which is exactly what the pane's own header shows.
fn display_label(doc: &EphemeralDoc) -> String {
    let raw = if !doc.multi_album && doc.albums.len() == 1 && !doc.albums[0].title.is_empty() {
        doc.albums[0].title.as_str()
    } else {
        doc.name.as_str()
    };
    if raw.is_empty() {
        return qbz_i18n::t("Media");
    }
    // Count CHARACTERS, not bytes: truncating a UTF-8 string by byte index
    // panics mid-codepoint, and this text is user data — a Japanese album
    // title is three bytes per character.
    if raw.chars().count() > LABEL_MAX {
        let cut: String = raw.chars().take(LABEL_MAX - 1).collect();
        format!("{cut}…")
    } else {
        raw.to_string()
    }
}

fn publish_doc(doc: &EphemeralDoc, loading: bool) {
    let json = to_json(doc);
    let label = display_label(doc);
    ui(move |mut b| {
        b.as_mut().set_local_ephemeral_active(true);
        b.as_mut()
            .set_local_ephemeral_json(QString::from(json.as_str()));
        b.as_mut()
            .set_local_ephemeral_label(QString::from(label.as_str()));
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
        b.as_mut().set_local_ephemeral_label(QString::from(""));
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
/// Counts USER-initiated opens. The boot restore does not touch it.
static OPEN_SEQ: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

async fn scan(runtime: Option<Runtime>, path: String) {
    if let Some(rt) = &runtime {
        wipe_if_playing(rt).await;
        // `runtime.is_some()` is ALREADY the "the user asked for this"
        // discriminator: `open()` and `open_path()` pass one, `rehydrate()`
        // passes None. Bumping here — before the loading frame below — is what
        // moves the view onto the session's tab, and it must be a SEQUENCE
        // rather than the `active` flag: opening a second folder over a first
        // leaves `active` true, so nothing watching that flag ever fires.
        // Bumping only for a user open is what keeps the boot restore from
        // hijacking whatever tab the user actually opened the view on.
        let seq = OPEN_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        ui(move |mut b| {
            b.as_mut().set_local_ephemeral_open_seq(seq);
        });
    }
    let name = folder_display_name(&path);
    // Header + spinner immediately; the scan of a big folder is not instant.
    publish_doc(
        &EphemeralDoc {
            name: name.clone(),
            path: path.clone(),
            track_count: 0,
            total_duration: String::new(),
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
pub async fn play_all(runtime: &Runtime, shuffle: bool) {
    let mut rows = tracks_snapshot();
    if shuffle {
        // The same shape `lib::play_album(.., shuffle)` gives an indexed
        // album: the QUEUE is shuffled once and playback starts at its head,
        // rather than leaving the order alone and flipping a player mode.
        // That keeps the queue panel honest — what you see is what plays.
        shuffle_in_place(&mut rows);
    }
    play_rows(runtime, rows, 0).await;
}

/// Fisher-Yates over splitmix64. No `rand` dependency (this crate has none),
/// but NOT a throwaway generator either.
///
/// The first version used a bare xorshift64 seeded from the clock, with a
/// comment claiming a folder shuffle "is not a place that needs a good
/// generator". Measured over eight consecutive shuffles of a ten-track album,
/// one track opened FOUR of them. Shuffle quality is something the listener
/// sees directly, so the comment was wrong and this is the fix.
///
/// splitmix64 finalises each state with two xor-shift-multiplies, which
/// avalanche far better than a raw xorshift — and the modulo below reads the
/// LOW bits, which is exactly where a weak generator is weakest. The counter
/// in the seed is what keeps two shuffles inside the same clock tick from
/// producing the same permutation.
fn shuffle_in_place<T>(v: &mut [T]) {
    static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491_4F6C_DD1D);
    let mut state = nanos
        ^ NONCE
            .fetch_add(0x9E37_79B9_7F4A_7C15, std::sync::atomic::Ordering::Relaxed)
            .rotate_left(32);

    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };

    for i in (1..v.len()).rev() {
        v.swap(i, (next() % (i as u64 + 1)) as usize);
    }
}

#[cfg(test)]
mod shuffle_tests {
    use super::*;

    /// Not "is it random" — that is not testable here — but the two failures
    /// that actually shipped: a permutation that loses or duplicates items,
    /// and a first position that is effectively pinned. 2000 shuffles of ten
    /// items should put every item first roughly 200 times; the old xorshift
    /// put one item first 50% of the time in a hand sample.
    #[test]
    fn every_item_survives_and_no_position_is_pinned() {
        let mut firsts = [0usize; 10];
        for _ in 0..2000 {
            let mut v: Vec<usize> = (0..10).collect();
            shuffle_in_place(&mut v);
            let mut seen = v.clone();
            seen.sort_unstable();
            assert_eq!(seen, (0..10).collect::<Vec<_>>(), "shuffle lost or duplicated items");
            firsts[v[0]] += 1;
        }
        // Generous bounds: this guards against a PINNED position, not against
        // a merely mediocre generator. Uniform is 200; anything outside
        // 100..350 means the low bits are not moving.
        for (item, &count) in firsts.iter().enumerate() {
            assert!(
                (100..=350).contains(&count),
                "item {item} opened {count}/2000 shuffles — the distribution is skewed"
            );
        }
    }

    #[test]
    fn the_degenerate_lengths_do_not_panic() {
        let mut empty: Vec<u8> = vec![];
        shuffle_in_place(&mut empty);
        let mut one = vec![7];
        shuffle_in_place(&mut one);
        assert_eq!(one, vec![7]);
    }
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

#[cfg(test)]
mod display_label_tests {
    use super::*;

    fn doc(name: &str, albums: Vec<(&str, &str)>, multi: bool) -> EphemeralDoc {
        EphemeralDoc {
            name: name.to_string(),
            path: "/tmp/x".to_string(),
            track_count: 0,
            total_duration: String::new(),
            multi_album: multi,
            albums: albums
                .into_iter()
                .map(|(title, artist)| EphemeralAlbumBlock {
                    group_key: String::new(),
                    title: title.to_string(),
                    artist: artist.to_string(),
                    meta: String::new(),
                    is_cue: false,
                    art_key: String::new(),
                    tracks: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn one_album_is_named_by_its_album() {
        // The usual case, and the reason the chain starts here: a folder IS
        // an album, and so is a disc.
        let d = doc("Pink Floyd London Live 8 Reunion 2005", vec![("Live 8", "Pink Floyd")], false);
        assert_eq!(display_label(&d), "Live 8");
    }

    #[test]
    fn several_albums_fall_back_to_the_medium_name() {
        let d = doc("dsdsmoke", vec![("A", "x"), ("B", "y")], true);
        assert_eq!(display_label(&d), "dsdsmoke");
    }

    #[test]
    fn an_untitled_album_falls_back_too() {
        // Tags gave no album title: naming the tab "" would render an empty
        // segment with a lone count badge.
        let d = doc("Disc 1", vec![("", "")], false);
        assert_eq!(display_label(&d), "Disc 1");
    }

    #[test]
    fn a_nameless_session_gets_the_generic_word() {
        assert_eq!(display_label(&doc("", vec![], false)), qbz_i18n::t("Media"));
    }

    #[test]
    fn a_long_name_is_elided_by_CHARACTERS_not_bytes() {
        // The elision exists because QbzTabBar sizes a segment to its text.
        let long = "Symphony No. 9 in D minor, Op. 125 — Choral";
        let out = display_label(&doc(long, vec![], false));
        assert_eq!(out.chars().count(), LABEL_MAX);
        assert!(out.ends_with('…'));

        // And the byte-vs-char part is not theoretical: slicing this by byte
        // index would panic mid-codepoint.
        let jp = "交響曲第九番ニ短調作品百二十五合唱付きベートーヴェン不滅の傑作";
        assert!(jp.chars().count() > LABEL_MAX);
        let out = display_label(&doc(jp, vec![], false));
        assert_eq!(out.chars().count(), LABEL_MAX);
    }

    #[test]
    fn a_name_exactly_at_the_cap_is_left_alone() {
        let exact: String = "a".repeat(LABEL_MAX);
        assert_eq!(display_label(&doc(&exact, vec![], false)), exact);
    }
}
