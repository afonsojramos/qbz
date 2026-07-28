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

use crate::local_rows::{AlbumRow, ArtistRow, LocalCounts, TrackRow, TreeNode};

/// Tracks-tab page size. The Tracks table is the 16K-row freeze surface, so
/// it stays server-paginated; the other tabs are bounded and full-load.
pub const TRACKS_PAGE: u64 = 500;

// ---------------------------------------------------------------------------
// Database access (library_db.rs `with_db` 1:1 — a fresh connection per op on
// the calling BLOCKING thread; `LibraryDatabase` holds a non-Send rusqlite
// handle, so it never crosses an await point).
// ---------------------------------------------------------------------------

pub fn db_path() -> Option<PathBuf> {
    let uid = UserDataPaths::load_last_user_id()?;
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
    pub tracks_offset: u64,
    pub tracks_query: String,
    pub tracks_sort: String,
    pub tracks_has_more: bool,
    /// The FULL flattened tree (visible derivation applies the rail search).
    pub tree: Vec<TreeNode>,
    pub tree_search: String,
    /// artKey -> the artwork SOURCE: an on-disk cover path, or a raw Plex
    /// `/library/...` thumb path. The artwork window turns these into 256px
    /// thumbnails (local) or tokenized, disk-cached fetches (Plex).
    pub art_index: HashMap<String, String>,
    /// Album identity ("folder" | "metadata") — persisted (locallibrary_ui).
    pub album_mode: String,
    pub counts: LocalCounts,
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
            ..LocalState::default()
        }
    });
    f(s)
}

/// Take the art index out, run `f` with it, put it back — the shape every
/// loader uses so mapping can register covers without a second lock.
pub fn with_art<R>(f: impl FnOnce(&mut HashMap<String, String>) -> R) -> R {
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

pub fn write_prefs(p: &Prefs) {
    let Some(path) = prefs_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(p) {
        let _ = std::fs::write(&path, bytes);
    }
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
    let mut p = read_prefs();
    p.albums_id_mode = mode.to_string();
    write_prefs(&p);
}

pub fn album_mode() -> String {
    state(|s| s.album_mode.clone())
}

pub fn set_tracks_sort(sort: &str) {
    state(|s| s.tracks_sort = sort.to_string());
    let mut p = read_prefs();
    p.tracks_sort = sort.to_string();
    write_prefs(&p);
}

pub fn tracks_sort() -> String {
    state(|s| s.tracks_sort.clone())
}

pub fn set_tracks_query(q: &str) {
    state(|s| s.tracks_query = q.to_string());
}

pub fn tracks_has_more() -> bool {
    state(|s| s.tracks_has_more)
}

pub fn counts() -> LocalCounts {
    state(|s| s.counts.clone())
}

/// Whether the local library is usable at all (a per-user db with at least
/// one registered folder). Drives the "nothing indexed yet" empty state.
/// A Plex-only setup is ALSO usable — the browse union has content even with
/// no registered on-disk folder.
pub fn has_library() -> bool {
    if with_db(|db| db.get_folders()).map(|f| !f.is_empty()).unwrap_or(false) {
        return true;
    }
    crate::local_plex::is_enabled() && crate::local_plex::cached_track_count() > 0
}
