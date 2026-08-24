//! Local Library shared state: the per-user `library.db` handle helper, the
//! persisted toolbar prefs, and the in-process document cache the loaders
//! fill and the playback/artwork paths read back.
//!
//! Split out of `local_library_qt.rs` (phase-24 modularization). ADR-006:
//! nothing here re-implements scanning or grouping — every read goes through
//! `qbz_library::LibraryDatabase`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use qbz_app::user_data::UserDataPaths;
use qbz_library::{LibraryDatabase, LibraryError, LocalTrack};
use qbz_source::SourceId;

use crate::local_rows::{AlbumRow, ArtistRow, LocalCounts, TrackRow, TreeNode};

/// Tracks-tab page size. The Tracks table is the 16K-row freeze surface, so
/// it stays server-paginated; the other tabs are bounded and full-load.
pub const TRACKS_PAGE: u64 = 500;

/// Independent Phase-A offsets. Phase E replaces them with catalog keysets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackSourceOffsets {
    pub local: u64,
    pub plex: u64,
    pub jellyfin: u64,
    pub subsonic: u64,
}

impl TrackSourceOffsets {
    pub fn add_assign(&mut self, consumed: Self) {
        self.local += consumed.local;
        self.plex += consumed.plex;
        self.jellyfin += consumed.jellyfin;
        self.subsonic += consumed.subsonic;
    }
}

#[derive(Debug, Clone)]
pub struct TracksLoadRequest {
    pub generation: u64,
    pub offsets: TrackSourceOffsets,
    pub query: String,
    pub sort: String,
    pub group: String,
}

// ---------------------------------------------------------------------------
// Database access (library_db.rs `with_db` 1:1 — a fresh connection per op on
// the calling BLOCKING thread; `LibraryDatabase` holds a non-Send rusqlite
// handle, so it never crosses an await point).
// ---------------------------------------------------------------------------

/// `unwrap_or(0)` = the GUEST profile — kept identical to
/// `library_db_qt::db_path`, which carries the full reasoning. Short version:
/// `?`-ing out here made the whole local library invisible on a machine that
/// has never completed a Qobuz login, and `users/0/` is the profile
/// `activate_offline` and `adopt_guest_profile` already use for that user.
/// The two helpers open the SAME file and must never disagree about where it
/// is.
pub fn db_path() -> Option<PathBuf> {
    let uid = UserDataPaths::load_last_user_id().unwrap_or(0);
    Some(
        dirs::data_dir()?
            .join("qbz")
            .join("users")
            .join(uid.to_string())
            .join("library.db"),
    )
}

pub fn with_db<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&LibraryDatabase) -> Result<R, LibraryError>,
{
    let path = db_path()?;
    if !path.exists() {
        return None;
    }
    let db = match LibraryDatabase::open(&path) {
        Ok(db) => db,
        Err(e) => {
            log::error!("[qbz-qt] local library open failed: {e}");
            return None;
        }
    };
    match f(&db) {
        Ok(r) => Some(r),
        Err(e) => {
            log::error!("[qbz-qt] local library query failed: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct LocalState {
    pub albums: Vec<AlbumRow>,
    pub folders: Vec<AlbumRow>,
    pub artists: Vec<ArtistRow>,
    pub tracks: Vec<TrackRow>,
    /// The RAW rows behind `tracks` (local + Plex). Playback reads these
    /// instead of re-querying by id — a Plex row has no `local_tracks` id to
    /// re-query with, and the local path saves an N-query loop.
    pub tracks_raw: Vec<LocalTrack>,
    /// The RAW rows behind the OPEN detail pane (album or folder) — same
    /// rationale as `tracks_raw`: a context-menu enqueue on a Plex detail row
    /// has no `local_tracks` id to re-query with.
    pub detail_raw: Vec<LocalTrack>,
    pub tracks_offsets: TrackSourceOffsets,
    /// Search/sort generation. An older worker may not mutate or publish.
    pub tracks_generation: u64,
    pub tracks_query: String,
    pub tracks_sort: String,
    /// Tracks-tab grouping ("off" | "album" | "artist" | "name") — persisted
    /// (locallibrary_ui `tracks_group`). It is part of the immutable paged
    /// query: otherwise a later page can sort ahead of the visible prefix.
    pub tracks_group: String,
    pub tracks_has_more: bool,
    /// The FULL flattened tree (visible derivation applies the rail search).
    pub tree: Vec<TreeNode>,
    pub tree_search: String,
    /// artKey -> `(the source that owns the row, its RAW artwork token)`.
    ///
    /// It used to hold the token alone, and `artwork_qt::classify` sniffed the
    /// provenance back out of the characters at window time — which is bug 3.
    /// It briefly held a fully resolved `ArtRef` instead, which fixed that but
    /// moved the resolution from ~50 VISIBLE rows to all 1703 rows of the
    /// document, and a Plex row's resolution reads its credentials: measured
    /// 39-92 µs/row against 2 µs for a row that needs none.
    ///
    /// Carrying the SOURCE beside the token keeps both properties. Provenance
    /// survives — nobody sniffs anything — and the resolution happens where it
    /// always did, in `local_artwork::resolve_window_blocking`, over the keys
    /// actually on screen.
    pub art_index: HashMap<String, (SourceId, String)>,
    /// Album identity ("folder" | "metadata") — persisted (locallibrary_ui).
    pub album_mode: String,
    pub counts: LocalCounts,
    /// Synthetic logical album id -> strongly-associated authoritative album
    /// ids. Rebuilt with each Albums query; never persisted and therefore
    /// reversible when source evidence changes.
    pub album_version_ids: HashMap<String, Vec<String>>,
    /// The OPEN local album's group key ("" when nothing is open).
    pub album_id: String,
    /// The OPEN album's VERSIONS — (source-directory key, its tracks). A
    /// "version" is a distinct PHYSICAL copy; metadata identity can fold
    /// several directories into one card, and merging their tracks would
    /// render a duplicated list. Cached so the picker switches with no DB
    /// round-trip (the Slint's `ALBUM_VERSIONS`). See `local_album_actions`.
    pub album_versions: Vec<(String, Vec<LocalTrack>)>,
    /// Index into `album_versions` currently shown by the picker.
    pub album_version_index: usize,
}

static LOCAL: Mutex<Option<LocalState>> = Mutex::new(None);

pub fn state<R>(f: impl FnOnce(&mut LocalState) -> R) -> R {
    let mut guard = LOCAL.lock().unwrap();
    let s = guard.get_or_insert_with(|| {
        let prefs = read_prefs();
        LocalState {
            album_mode: prefs.albums_id_mode,
            tracks_sort: prefs.tracks_sort,
            tracks_group: prefs.tracks_group,
            ..LocalState::default()
        }
    });
    f(s)
}

/// Take the art index out, run `f` with it, put it back — the shape every
/// loader uses so mapping can register covers without a second lock.
pub fn with_art<R>(f: impl FnOnce(&mut HashMap<String, (SourceId, String)>) -> R) -> R {
    let mut art = state(|s| std::mem::take(&mut s.art_index));
    let out = f(&mut art);
    state(|s| s.art_index = art);
    out
}

/// Drop every cached document (logout / user switch).
pub fn reset() {
    *LOCAL.lock().unwrap() = None;
}

// ---------------------------------------------------------------------------
// Toolbar prefs (locallibrary_prefs.rs 1:1 — the SAME json the Slint writes,
// so the two frontends share the user's album-identity + sort choices).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Prefs {
    #[serde(default = "d_off")]
    pub tracks_group: String,
    #[serde(default = "d_default")]
    pub tracks_sort: String,
    #[serde(default = "d_folder")]
    pub albums_id_mode: String,
    #[serde(default)]
    pub ephemeral_folder: Option<String>,
}

fn d_off() -> String {
    "off".into()
}
fn d_default() -> String {
    "default".into()
}
fn d_folder() -> String {
    "folder".into()
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            tracks_group: d_off(),
            tracks_sort: d_default(),
            albums_id_mode: d_folder(),
            ephemeral_folder: None,
        }
    }
}

fn prefs_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("locallibrary_ui.json"))
}

pub fn read_prefs() -> Prefs {
    let Some(path) = prefs_path() else {
        return Prefs::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Prefs::default(),
    }
}

/// Merge this struct's four keys into the on-disk document and publish it
/// ATOMICALLY (`settings_qt::write_json_object_atomic`).
///
/// Two changes from the old whole-struct `std::fs::write`, both because this
/// file is co-owned with the SHIPPING Slint build (`locallibrary_prefs.rs`,
/// same path):
///
/// 1. `fs::write` opens O_TRUNC, so between the truncate and the last byte the
///    file is short or empty. Slint's reader answers a parse failure with
///    `Prefs::default()` and writes those defaults back on its next toolbar
///    change — one unlucky read inside our window resets the user's album
///    identity and sort. The temp-file + `rename(2)` publish closes it: a
///    concurrent reader sees the whole old document or the whole new one.
/// 2. Serializing the struct REPLACED the document, so any key Slint gains
///    later would be dropped by this frontend on the first toolbar click.
///    Merging keeps unknown keys, and `read_json_object` refuses to write at
///    all when the document did not parse rather than rebuilding it.
pub fn write_prefs(p: &Prefs) {
    let Some(path) = prefs_path() else {
        return;
    };
    let Some(mut doc) = crate::settings_qt::read_json_object(&path) else {
        return;
    };
    merge_prefs(&mut doc, p);
    crate::settings_qt::write_json_object_atomic(&path, &doc);
}

fn merge_prefs(doc: &mut serde_json::Map<String, serde_json::Value>, p: &Prefs) {
    if let Ok(serde_json::Value::Object(map)) = serde_json::to_value(p) {
        for (key, value) in map {
            doc.insert(key, value);
        }
    }
}

/// Read-modify-write of ONE document: `edit` sees the prefs as they are on
/// disk RIGHT NOW and the result goes back into the same map.
///
/// The `read_prefs()` -> mutate -> `write_prefs()` shape it replaces has the
/// torn-read hole: a read that lands inside another process's write window
/// returns `Prefs::default()`, and the write that follows commits those
/// defaults over the keys the caller never meant to touch.
fn update_prefs(edit: impl FnOnce(&mut Prefs)) {
    let Some(path) = prefs_path() else {
        return;
    };
    let Some(mut doc) = crate::settings_qt::read_json_object(&path) else {
        return;
    };
    // A valid JSON object whose OWN keys are the wrong shape: repair to the
    // defaults (what every reader on both sides already sees) but say so —
    // this is the one path here that overwrites values it could not read.
    let mut prefs: Prefs = serde_json::from_value(serde_json::Value::Object(doc.clone()))
        .unwrap_or_else(|e| {
            log::warn!("[qbz-qt] locallibrary_ui.json keys unreadable ({e}) — using defaults");
            Prefs::default()
        });
    edit(&mut prefs);
    merge_prefs(&mut doc, &prefs);
    crate::settings_qt::write_json_object_atomic(&path, &doc);
}

// ---------------------------------------------------------------------------
// Toolbar setters / getters (all cheap; the loads that follow are queued by
// the bridge)
// ---------------------------------------------------------------------------

pub fn group_mode() -> qbz_library::album_grouping::AlbumGroupMode {
    qbz_library::album_grouping::AlbumGroupMode::from_pref(&state(|s| s.album_mode.clone()))
}

pub fn set_album_mode(mode: &str) {
    let mode = if mode == "metadata" { "metadata" } else { "folder" };
    state(|s| s.album_mode = mode.to_string());
    update_prefs(|p| p.albums_id_mode = mode.to_string());
    // The grouping IS the query: `LocalSource` resolves an album's tracks
    // through `get_album_tracks_metadata` or `get_album_tracks` depending on
    // it. Leaving the source on its default would make album PLAYBACK use a
    // different track list than the grid just rendered.
    crate::source_wiring::sync_album_group_mode();
}

pub fn album_mode() -> String {
    state(|s| s.album_mode.clone())
}

pub fn set_tracks_sort(sort: &str) {
    state(|s| s.tracks_sort = sort.to_string());
    update_prefs(|p| p.tracks_sort = sort.to_string());
}

pub fn tracks_sort() -> String {
    state(|s| s.tracks_sort.clone())
}

/// Tracks-tab grouping. Persisted through the SAME read-modify-write of
/// `locallibrary_ui.json` the sort and the album identity use — the key was
/// already read back on load and preserved on write, it was only ever missing
/// its setter and its seed (PARITY-DEBT #13).
pub fn set_tracks_group(mode: &str) {
    let mode = match mode {
        "album" | "artist" | "name" => mode,
        _ => "off",
    };
    state(|s| s.tracks_group = mode.to_string());
    update_prefs(|p| p.tracks_group = mode.to_string());
}

pub fn tracks_group() -> String {
    state(|s| s.tracks_group.clone())
}

pub fn set_tracks_query(q: &str) {
    state(|s| s.tracks_query = q.to_string());
}

pub fn tracks_query() -> String {
    state(|s| s.tracks_query.clone())
}

pub fn begin_tracks_load(reset: bool) -> TracksLoadRequest {
    state(|s| {
        if reset {
            s.tracks.clear();
            s.tracks_raw.clear();
            s.tracks_offsets = TrackSourceOffsets::default();
            s.tracks_has_more = false;
            s.tracks_generation = s.tracks_generation.wrapping_add(1);
            if s.tracks_generation == 0 {
                s.tracks_generation = 1;
            }
        }
        TracksLoadRequest {
            generation: s.tracks_generation,
            offsets: s.tracks_offsets,
            query: s.tracks_query.clone(),
            sort: s.tracks_sort.clone(),
            group: s.tracks_group.clone(),
        }
    })
}

pub fn tracks_generation() -> u64 {
    state(|s| s.tracks_generation)
}

pub(crate) fn tracks_request_is_current(
    current_generation: u64,
    current_offsets: TrackSourceOffsets,
    request: &TracksLoadRequest,
) -> bool {
    current_generation == request.generation && current_offsets == request.offsets
}

pub fn commit_tracks_page(
    request: &TracksLoadRequest,
    rows: Vec<TrackRow>,
    raw: Vec<LocalTrack>,
    art: HashMap<String, (SourceId, String)>,
    consumed: TrackSourceOffsets,
    has_more: bool,
) -> Option<Vec<TrackRow>> {
    state(|s| {
        if !tracks_request_is_current(s.tracks_generation, s.tracks_offsets, request) {
            return None;
        }
        s.tracks_offsets.add_assign(consumed);
        s.art_index.extend(art);
        s.tracks.extend(rows);
        s.tracks_raw.extend(raw);
        s.tracks_has_more = has_more;
        Some(s.tracks.clone())
    })
}

pub fn tracks_has_more() -> bool {
    state(|s| s.tracks_has_more)
}

pub fn counts() -> LocalCounts {
    state(|s| s.counts.clone())
}

/// Whether the local library is usable at all (a per-user db with at least
/// one registered folder). Drives the "nothing indexed yet" empty state.
/// A server-only setup is ALSO usable — the browse union has content even
/// with no registered on-disk folder.
pub fn has_library() -> bool {
    let has_folders = with_db(|db| db.get_folders())
        .map(|f| !f.is_empty())
        .unwrap_or(false);
    library_sources_available(
        has_folders,
        crate::local_plex::is_enabled(),
        crate::local_plex::cached_track_count(),
        crate::media_servers_qt::cached_track_count(),
    )
}

fn library_sources_available(
    has_folders: bool,
    plex_enabled: bool,
    plex_tracks: i64,
    remote_tracks: i64,
) -> bool {
    has_folders || (plex_enabled && plex_tracks > 0) || remote_tracks > 0
}

#[cfg(test)]
mod phase_a_tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, Arc};

    use super::{
        library_sources_available, tracks_request_is_current, TrackSourceOffsets,
        TracksLoadRequest,
    };

    #[test]
    fn remote_only_installation_has_a_library() {
        assert!(library_sources_available(true, false, 0, 0));
        assert!(library_sources_available(false, true, 5_137, 0));
        assert!(library_sources_available(false, false, 0, 4_924));
        assert!(library_sources_available(false, false, 0, 6_678));
        assert!(!library_sources_available(false, false, 0, 0));
    }

    #[test]
    fn an_older_query_finishing_after_a_new_one_is_rejected() {
        let generation = Arc::new(AtomicU64::new(1));
        let (old_started_tx, old_started_rx) = mpsc::channel();
        let (allow_old_finish_tx, allow_old_finish_rx) = mpsc::channel();
        let old_generation = Arc::clone(&generation);
        let old_request = TracksLoadRequest {
            generation: 1,
            offsets: TrackSourceOffsets::default(),
            query: "old".to_string(),
            sort: "title-asc".to_string(),
            group: "off".to_string(),
        };
        let old = std::thread::spawn(move || {
            old_started_tx.send(()).unwrap();
            allow_old_finish_rx.recv().unwrap();
            tracks_request_is_current(
                old_generation.load(Ordering::SeqCst),
                TrackSourceOffsets::default(),
                &old_request,
            )
        });

        old_started_rx.recv().unwrap();
        generation.store(2, Ordering::SeqCst);
        let new_request = TracksLoadRequest {
            generation: 2,
            offsets: TrackSourceOffsets::default(),
            query: "new".to_string(),
            sort: "artist-asc".to_string(),
            group: "off".to_string(),
        };
        allow_old_finish_tx.send(()).unwrap();

        assert!(!old.join().unwrap());
        assert!(tracks_request_is_current(
            generation.load(Ordering::SeqCst),
            TrackSourceOffsets::default(),
            &new_request,
        ));
    }

    #[test]
    fn tracks_tab_keeps_an_origin_aware_page_trigger_and_manual_fallback() {
        // Smoke regression (2026-08-23): the compatibility ListView reached
        // exactly three 500-row pages and then stranded the remaining 27,871
        // rows. ListView's visual offset is contentY - originY after repeated
        // model replacement; a raw contentY comparison is not stable. Keep a
        // second, explicit affordance so an omitted Flickable terminal signal
        // can never make the catalog unreachable again.
        let qml = include_str!("../qml/views/local/LocalTracksTab.qml");
        assert!(qml.contains("list.contentY - list.originY"));
        assert!(qml.contains("onAtYEndChanged"));
        assert!(qml.contains("onMovementEnded"));
        assert!(qml.contains("QbzLoadMore"));
        assert!(qml.contains("onClicked: root.requestNextPage(true)"));
    }

    #[test]
    fn tracks_page_republish_keeps_the_visible_track_anchor() {
        // A legacy page append republishes the accumulated rows as JSON. Keep
        // one ListModel mounted and append an immutable prefix in-place so
        // QQuickItemView never receives setModel(). Full-query replacement
        // still follows track identity and retains the clipped-row offset.
        let qml = include_str!("../qml/views/local/LocalTracksTab.qml");
        let view_qml = include_str!("../qml/views/LocalLibraryView.qml");
        assert!(qml.contains("id: legacyEntriesModel"));
        assert!(!qml.contains("dynamicRoles: true"));
        assert!(qml.contains("function canAppendLegacyEntries(nextEntries)"));
        assert!(qml.contains("legacyEntriesModel.append({\"modelData\": nextEntries[i]})"));
        assert!(qml.contains("? root.view.nativeTracksModel : legacyEntriesModel"));
        assert!(!qml.contains("? root.view.nativeTracksModel : root.entries"));
        assert!(qml.contains("function capturePageAnchor()"));
        assert!(qml.contains("String(root.entries[i].row.id)"));
        assert!(qml.contains("cell.y - list.contentY"));
        assert!(!qml.contains("cell.mapToItem(list"));
        assert!(qml.contains("|| root.restoringPageAnchor"));
        assert!(qml.contains("root.pendingPageAnchor = root.capturePageAnchor()"));
        assert!(qml.contains("var anchor = root.pendingPageAnchor"));
        assert!(qml.contains("id: pageAnchorSettle"));
        assert!(qml.contains("id: pageRequestRelease"));
        assert!(qml.contains("pageAnchorSettle.restart()"));
        assert!(qml.contains("var wanted = list.contentY + currentScreenY - anchor.screenY"));
        assert!(qml.contains("function restorePageAnchor(anchor, nextEntries, epoch)"));
        assert!(qml.contains("Qt.callLater(function ()"));
        assert!(qml.contains("list.positionViewAtIndex(target, ListView.Beginning)"));
        assert!(qml.contains("root.publishLegacyEntries(out, anchor, epoch)"));
        assert!(qml.contains("root.restorePageAnchor(anchor, nextEntries, epoch)"));
        assert!(view_qml.contains("readonly property var tracksVisible: tracks"));
        assert!(!view_qml.contains("readonly property var tracksVisible: {"));
    }
}
