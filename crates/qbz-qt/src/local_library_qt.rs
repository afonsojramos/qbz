//! Local Library data layer — the Qt/QML port of
//! `crates/qbz/src/local_library.rs` (the shipping Slint controller) with
//! the Tauri `LocalLibraryView.svelte` visuals as the reference.
//!
//! ADR-006: NOTHING here re-implements scanning, album identity or the
//! queries — every read goes through the frontend-agnostic `qbz-library`
//! crate (`LibraryDatabase`), exactly like `library_db_qt.rs` does for the
//! playlist rows. What this module owns is:
//!
//!  1. mapping `qbz_library` rows into the flat serde documents the QML
//!     view parses once per publish (the transport shape established in
//!     `library_qt.rs` — ONE JSON string per surface);
//!  2. the folder-tree model (expanded set + lazily fetched levels,
//!     flattened to a windowable array — `local_library.rs`
//!     `apply_tree` / `derive_folder_tree_visible` semantics);
//!  3. windowed, id-keyed artwork: covers are resolved to the
//!     `qbz-library` THUMBNAIL (256px) and emitted per artKey, so a 16K
//!     track library never decodes a full-size cover per row (the
//!     documented Slint freeze on exactly this surface);
//!  4. local playback: `LocalTrack` -> `QueueTrack` (playback.rs
//!     `local_queue_track` 1:1) -> `core().set_queue` -> the player's
//!     `play_data` seam. The PROTECTED audio path is untouched — the file
//!     bytes are handed to the same entry point the Slint uses.
//!
//! POC-NOTEs (deliberate cuts, named for the effort report):
//! - Plex: NOT wired. `get_albums_metadata_page` is called with
//!   `plex_cache_path = None`, so the union degrades to local-only.
//! - Ephemeral folders (the ad-hoc "open a folder" pane), the tag editor,
//!   multi-select bulk bars, mixtapes and A-Z alpha jump strips are out of
//!   scope.
//! - CUE virtual tracks play from their container's start offset (the
//!   Slint's "already-loaded container -> seek only" fast path is ported;
//!   the rest of the CUE handling is the same seek-after-load).
//! - Network-folder exclusion is always `false` here (the Slint keys it on
//!   live connectivity); the index stays the source of truth.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use qbz_app::shell::AppRuntime;
use qbz_app::user_data::UserDataPaths;
use qbz_core::LoggingAdapter;
use qbz_library::album_grouping::AlbumGroupMode;
use qbz_library::{
    AudioFormat, FolderTreeEntry, LibraryDatabase, LibraryError, LocalAlbum, LocalTrack,
};
use qbz_models::QueueTrack;
use serde::Serialize;

/// Tracks-tab page size. The Slint pages the Tracks table (the 16K-row
/// freeze surface); the other tabs are bounded and full-load.
const TRACKS_PAGE: u64 = 500;

// ---------------------------------------------------------------------------
// Database access (library_db.rs `with_db` 1:1 — a fresh connection per op on
// the calling BLOCKING thread; `LibraryDatabase` holds a non-Send rusqlite
// handle, so it never crosses an await point).
// ---------------------------------------------------------------------------

fn db_path() -> Option<PathBuf> {
    let uid = UserDataPaths::load_last_user_id()?;
    Some(
        dirs::data_dir()?
            .join("qbz")
            .join("users")
            .join(uid.to_string())
            .join("library.db"),
    )
}

fn with_db<F, R>(f: F) -> Option<R>
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
// Transport rows (the QML contract)
// ---------------------------------------------------------------------------

/// One album card (Albums tab + Folders flat mode). `id` is the group key
/// of the ACTIVE identity mode — folder or metadata — so the detail query
/// can round-trip it.
#[derive(Clone, Default, Serialize)]
pub struct AlbumRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "allArtists")]
    pub all_artists: String,
    pub year: String,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub format: String,
    #[serde(rename = "artKey")]
    pub art_key: String,
    pub source: String,
    #[serde(rename = "directoryPath")]
    pub directory_path: String,
    /// Number of distinct contributing folders (metadata mode only; > 1 is
    /// the Tauri "album spans N folders" tooltip case).
    #[serde(rename = "folderCount")]
    pub folder_count: u32,
}

/// One track row (Tracks tab, album detail, folder detail).
#[derive(Clone, Default, Serialize)]
pub struct TrackRow {
    /// The local-library row id, as a string (the QML rows are id-keyed).
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    /// Never a Qobuz artist id — local rows have none; kept so the shared
    /// TrackRow.qml "go to artist" arm stays hidden.
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub number: u32,
    pub disc: u32,
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub format: String,
    pub year: String,
    #[serde(rename = "artKey")]
    pub art_key: String,
    /// Filled by the artwork window (never on the bulk publish).
    #[serde(rename = "artPath")]
    pub art_path: String,
    pub source: String,
    pub explicit: bool,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct ArtistRow {
    pub name: String,
    #[serde(rename = "albumCount")]
    pub album_count: u32,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    #[serde(rename = "artKey")]
    pub art_key: String,
}

/// One row of the FLATTENED folder tree (the rail renders a windowed list
/// over this array — never a recursive component).
#[derive(Clone, Default, Serialize)]
pub struct TreeNode {
    pub path: String,
    pub segment: String,
    pub depth: i32,
    #[serde(rename = "isFolder")]
    pub is_folder: bool,
    #[serde(rename = "canExpand")]
    pub can_expand: bool,
    pub expanded: bool,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    #[serde(rename = "artKey")]
    pub art_key: String,
}

#[derive(Clone, Default, Serialize)]
pub struct SubfolderRow {
    pub path: String,
    pub name: String,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    #[serde(rename = "artKey")]
    pub art_key: String,
}

#[derive(Clone, Default, Serialize)]
pub struct FolderDetail {
    pub path: String,
    pub name: String,
    #[serde(rename = "trackCount")]
    pub track_count: u32,
    pub subfolders: Vec<SubfolderRow>,
    pub tracks: Vec<TrackRow>,
}

#[derive(Clone, Default, Serialize)]
pub struct AlbumDetail {
    pub album: AlbumRow,
    pub tracks: Vec<TrackRow>,
}

#[derive(Clone, Default, Serialize)]
pub struct LocalCounts {
    pub albums: i64,
    pub artists: i64,
    pub folders: i64,
    pub tracks: i64,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LocalState {
    albums: Vec<AlbumRow>,
    folders: Vec<AlbumRow>,
    artists: Vec<ArtistRow>,
    tracks: Vec<TrackRow>,
    tracks_offset: u64,
    tracks_query: String,
    tracks_sort: String,
    tracks_has_more: bool,
    /// The FULL flattened tree (visible derivation applies the rail search).
    tree: Vec<TreeNode>,
    tree_search: String,
    /// artKey -> the on-disk artwork source (full-size cover / Plex thumb
    /// path). The window turns these into 256px thumbnails on demand.
    art_index: HashMap<String, String>,
    /// Album identity ("folder" | "metadata") — persisted (locallibrary_ui).
    album_mode: String,
    counts: LocalCounts,
}

static LOCAL: Mutex<Option<LocalState>> = Mutex::new(None);

fn state<R>(f: impl FnOnce(&mut LocalState) -> R) -> R {
    let mut guard = LOCAL.lock().unwrap();
    let s = guard.get_or_insert_with(|| {
        let mut s = LocalState::default();
        s.album_mode = read_prefs().albums_id_mode;
        s.tracks_sort = read_prefs().tracks_sort;
        s
    });
    f(s)
}

// ---------------------------------------------------------------------------
// Toolbar prefs (locallibrary_prefs.rs 1:1 — the SAME json the Slint writes,
// so the two frontends share the user's album-identity + sort choices).
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
struct Prefs {
    #[serde(default = "d_off")]
    tracks_group: String,
    #[serde(default = "d_default")]
    tracks_sort: String,
    #[serde(default = "d_folder")]
    albums_id_mode: String,
    #[serde(default)]
    ephemeral_folder: Option<String>,
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

fn read_prefs() -> Prefs {
    let Some(path) = prefs_path() else {
        return Prefs::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Prefs::default(),
    }
}

fn write_prefs(p: &Prefs) {
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
// Mapping helpers
// ---------------------------------------------------------------------------

fn mmss(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m >= 60 {
        format!("{}:{:02}:{:02}", m / 60, m % 60, s)
    } else {
        format!("{m}:{s:02}")
    }
}

/// Human total ("1 h 12 min" / "42 min") — the Tauri
/// `formatTotalDuration`.
fn total_duration(secs: u64) -> String {
    let mins = secs / 60;
    if mins >= 60 {
        format!("{} h {} min", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}

/// Tier for the shared QualityBadge, mirroring its own JS derivation order
/// (MP3 first, then max / hires / cd) so a local card and a Qobuz card can
/// never disagree.
fn tier_of(format: &AudioFormat, bit_depth: Option<u32>, sample_rate_hz: f64) -> &'static str {
    if matches!(format, AudioFormat::Mp3) {
        return "mp3";
    }
    let khz = if sample_rate_hz >= 1000.0 {
        sample_rate_hz / 1000.0
    } else {
        sample_rate_hz
    };
    match bit_depth {
        Some(b) if b >= 24 && khz > 96.0 => "max",
        Some(b) if b >= 24 => "hires",
        Some(_) => "cd",
        None if khz >= 44.1 => "cd",
        None => "",
    }
}

fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

pub fn album_key(id: &str) -> String {
    format!("album:{id}")
}
pub fn track_key(id: i64) -> String {
    format!("track:{id}")
}
pub fn folder_key(path: &str) -> String {
    format!("folder:{path}")
}

fn map_album(a: LocalAlbum, art: &mut HashMap<String, String>) -> AlbumRow {
    let key = album_key(&a.id);
    if let Some(p) = a.artwork_path.as_ref().filter(|p| !p.is_empty()) {
        art.insert(key.clone(), p.clone());
    }
    let folder_count = a
        .source_folders
        .as_deref()
        .map(|s| s.split(',').filter(|x| !x.trim().is_empty()).count() as u32)
        .unwrap_or(0);
    AlbumRow {
        quality_tier: tier_of(&a.format, a.bit_depth, a.sample_rate).into(),
        quality_detail: crate::home_qt::quality_detail_from_parts(a.bit_depth, Some(a.sample_rate)),
        format: a.format.to_string(),
        year: a.year.map(|y| y.to_string()).unwrap_or_default(),
        duration: total_duration(a.total_duration_secs),
        track_count: a.track_count,
        art_key: key,
        source: if a.source == "plex" {
            "plex".into()
        } else if a.source == "qobuz_download" {
            "offline".into()
        } else {
            "local".into()
        },
        directory_path: a.directory_path,
        all_artists: a.all_artists,
        folder_count,
        id: a.id,
        title: a.title,
        artist: a.artist,
    }
}

fn map_track(t: &LocalTrack, art: &mut HashMap<String, String>) -> TrackRow {
    let key = track_key(t.id);
    if let Some(p) = t.artwork_path.as_ref().filter(|p| !p.is_empty()) {
        art.insert(key.clone(), p.clone());
    }
    TrackRow {
        id: t.id.to_string(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album_group_title.clone(),
        album_id: t.album_group_key.clone(),
        artist_id: String::new(),
        number: t.track_number.unwrap_or(0),
        disc: t.disc_number.unwrap_or(1),
        duration: mmss(t.duration_secs),
        quality_tier: tier_of(&t.format, t.bit_depth, t.sample_rate).into(),
        quality_detail: crate::home_qt::quality_detail_from_parts(t.bit_depth, Some(t.sample_rate)),
        format: t.format.to_string(),
        year: t.year.map(|y| y.to_string()).unwrap_or_default(),
        art_key: key,
        art_path: String::new(),
        source: match t.source.as_deref() {
            Some("plex") => "plex".into(),
            Some("qobuz_download") => "offline".into(),
            _ => "local".into(),
        },
        explicit: false,
        is_favorite: false,
    }
}

fn group_mode() -> AlbumGroupMode {
    AlbumGroupMode::from_pref(&state(|s| s.album_mode.clone()))
}

// ---------------------------------------------------------------------------
// Loads (each is a BLOCKING body; the bridge wraps them in spawn_blocking)
// ---------------------------------------------------------------------------

/// Whether the local library is usable at all (a per-user db with at least
/// one registered folder). Drives the "nothing indexed yet" empty state.
pub fn has_library() -> bool {
    with_db(|db| db.get_folders()).map(|f| !f.is_empty()).unwrap_or(false)
}

/// Albums tab: the FULL metadata/folder-grouped set in one page. Search,
/// sort and grouping derive QML-side over the parsed array (the
/// `library_qt.rs` transport rationale: album counts are bounded — the
/// unbounded surface is Tracks, which stays server-paginated).
pub fn load_albums_blocking() -> Result<Vec<AlbumRow>, String> {
    let mode = group_mode();
    let page = with_db(|db| {
        db.get_albums_metadata_page(
            0,
            100_000,
            None,
            "artist",
            "asc",
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
            /* plex_cache_path (POC-NOTE: Plex not wired) */ None,
            mode,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    let rows = state(|s| {
        let mut art = std::mem::take(&mut s.art_index);
        let rows: Vec<AlbumRow> = page
            .albums
            .into_iter()
            .map(|a| map_album(a, &mut art))
            .collect();
        s.art_index = art;
        s.counts.albums = page.total as i64;
        s.albums = rows.clone();
        rows
    });
    Ok(rows)
}

/// Folders tab, FLAT mode: the album grid grouped by DIRECTORY
/// (local_library.rs `fetch_folder_albums` — `get_albums_with_full_filter`,
/// never the metadata grouping).
pub fn load_folders_blocking() -> Result<Vec<AlbumRow>, String> {
    let albums = with_db(|db| {
        db.get_albums_with_full_filter(
            /* include_hidden */ false,
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    let rows = state(|s| {
        let mut art = std::mem::take(&mut s.art_index);
        let rows: Vec<AlbumRow> = albums.into_iter().map(|a| map_album(a, &mut art)).collect();
        s.art_index = art;
        s.counts.folders = rows.len() as i64;
        s.folders = rows.clone();
        rows
    });
    Ok(rows)
}

pub fn load_artists_blocking() -> Result<Vec<ArtistRow>, String> {
    let artists = with_db(|db| {
        db.get_artists_with_filter(
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    let rows: Vec<ArtistRow> = artists
        .into_iter()
        .map(|a| ArtistRow {
            art_key: format!("artist:{}", a.name),
            name: a.name,
            album_count: a.album_count,
            track_count: a.track_count,
        })
        .collect();
    state(|s| {
        s.counts.artists = rows.len() as i64;
        s.artists = rows.clone();
    });
    Ok(rows)
}

/// Tracks tab, ONE page. `reset` clears the accumulator (a new search or
/// sort); otherwise the page is appended (load-more on scroll).
pub fn load_tracks_page_blocking(reset: bool) -> Result<(Vec<TrackRow>, bool), String> {
    let (offset, query, sort) = state(|s| {
        if reset {
            s.tracks.clear();
            s.tracks_offset = 0;
        }
        (s.tracks_offset, s.tracks_query.clone(), s.tracks_sort.clone())
    });
    let page = with_db(|db| {
        db.search_with_filter_page(
            query.trim(),
            offset,
            TRACKS_PAGE,
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
            &sort,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    let has_more = page.len() as u64 == TRACKS_PAGE;
    let all = state(|s| {
        let mut art = std::mem::take(&mut s.art_index);
        let rows: Vec<TrackRow> = page.iter().map(|t| map_track(t, &mut art)).collect();
        s.art_index = art;
        s.tracks_offset += rows.len() as u64;
        s.tracks.extend(rows);
        s.tracks_has_more = has_more;
        s.tracks.clone()
    });
    Ok((all, has_more))
}

/// The tab badges. Cheap: the Tracks count never materializes the table.
pub fn load_counts_blocking() -> LocalCounts {
    let tracks = with_db(|db| db.count_all_local_tracks()).unwrap_or(0) as i64;
    state(|s| {
        s.counts.tracks = tracks;
        s.counts.clone()
    })
}

// ---- Folder tree ----------------------------------------------------------

fn entry_to_node(e: &FolderTreeEntry, depth: i32, art: &mut HashMap<String, String>) -> TreeNode {
    match e {
        FolderTreeEntry::Folder {
            path,
            segment,
            track_count_under,
            artwork,
        } => {
            let key = folder_key(path);
            if let Some(a) = artwork.as_ref().filter(|a| !a.is_empty()) {
                art.insert(key.clone(), a.clone());
            }
            TreeNode {
                path: path.clone(),
                segment: segment.clone(),
                depth,
                is_folder: true,
                can_expand: *track_count_under > 0,
                expanded: false,
                track_count: *track_count_under,
                art_key: key,
            }
        }
        FolderTreeEntry::Track { path, segment } => TreeNode {
            path: path.clone(),
            segment: segment.clone(),
            depth,
            is_folder: false,
            can_expand: false,
            expanded: false,
            track_count: 0,
            art_key: String::new(),
        },
    }
}

/// Seed the tree with the registered library folders (depth 0).
pub fn load_tree_roots_blocking() -> Vec<TreeNode> {
    let roots = with_db(|db| {
        let paths = db.get_folders()?;
        let mut out = Vec::with_capacity(paths.len());
        for p in paths {
            let cnt = db.count_folder_tracks_recursive(&p, false)?;
            out.push((p, cnt));
        }
        Ok::<_, LibraryError>(out)
    })
    .unwrap_or_default();
    let nodes: Vec<TreeNode> = roots
        .into_iter()
        .map(|(p, cnt)| TreeNode {
            art_key: folder_key(&p),
            segment: basename(&p),
            path: p,
            depth: 0,
            is_folder: true,
            can_expand: cnt > 0,
            expanded: false,
            track_count: cnt,
        })
        .collect();
    state(|s| s.tree = nodes.clone());
    nodes
}

/// Collapse: drop the contiguous descendant block (pure UI, no query).
pub fn tree_collapse(path: &str) -> Vec<TreeNode> {
    state(|s| {
        if let Some(pos) = s.tree.iter().position(|n| n.path == path) {
            let depth = s.tree[pos].depth;
            s.tree[pos].expanded = false;
            let mut end = pos + 1;
            while end < s.tree.len() && s.tree[end].depth > depth {
                end += 1;
            }
            s.tree.drain(pos + 1..end);
        }
        s.tree.clone()
    })
}

/// Expand: fetch ONE child level and splice it in (the Tauri tree's lazy
/// `list_folder_children`; never a recursive preload).
pub fn tree_expand_blocking(path: &str) -> Vec<TreeNode> {
    let children = with_db(|db| db.list_folder_children(path, false)).unwrap_or_default();
    state(|s| {
        let Some(pos) = s.tree.iter().position(|n| n.path == path) else {
            return s.tree.clone();
        };
        let depth = s.tree[pos].depth;
        s.tree[pos].expanded = true;
        let mut art = std::mem::take(&mut s.art_index);
        let nodes: Vec<TreeNode> = children
            .iter()
            .map(|e| entry_to_node(e, depth + 1, &mut art))
            .collect();
        s.art_index = art;
        for (i, n) in nodes.into_iter().enumerate() {
            s.tree.insert(pos + 1 + i, n);
        }
        s.tree.clone()
    })
}

pub fn tree_collapse_all() -> Vec<TreeNode> {
    state(|s| {
        s.tree.retain(|n| n.depth == 0);
        for n in s.tree.iter_mut() {
            n.expanded = false;
        }
        s.tree.clone()
    })
}

pub fn set_tree_search(query: &str) {
    state(|s| s.tree_search = query.trim().to_lowercase());
}

/// The rail's VISIBLE set: the flattened tree filtered by the rail search,
/// keeping every match AND its ancestors so the tree stays navigable
/// (local_library.rs `derive_folder_tree_visible` 1:1).
pub fn tree_visible() -> Vec<TreeNode> {
    state(|s| {
        if s.tree_search.is_empty() {
            return s.tree.clone();
        }
        let q = &s.tree_search;
        let matches: Vec<String> = s
            .tree
            .iter()
            .filter(|n| n.segment.to_lowercase().contains(q))
            .map(|n| n.path.clone())
            .collect();
        s.tree
            .iter()
            .filter(|n| {
                n.segment.to_lowercase().contains(q)
                    || matches.iter().any(|m| m.starts_with(&format!("{}/", n.path)))
            })
            .cloned()
            .collect()
    })
}

/// The right pane of tree mode: subfolders (cover cards) + the folder's
/// DIRECT tracks + the recursive count shown next to the name.
pub fn load_folder_detail_blocking(path: &str) -> FolderDetail {
    let children = with_db(|db| db.list_folder_children(path, false)).unwrap_or_default();
    let tracks = with_db(|db| db.list_folder_tracks(path, false)).unwrap_or_default();
    let count = with_db(|db| db.count_folder_tracks_recursive(path, false)).unwrap_or(0);
    state(|s| {
        let mut art = std::mem::take(&mut s.art_index);
        let subfolders: Vec<SubfolderRow> = children
            .iter()
            .filter_map(|e| match e {
                FolderTreeEntry::Folder {
                    path,
                    segment,
                    track_count_under,
                    artwork,
                } => {
                    let key = folder_key(path);
                    if let Some(a) = artwork.as_ref().filter(|a| !a.is_empty()) {
                        art.insert(key.clone(), a.clone());
                    }
                    Some(SubfolderRow {
                        path: path.clone(),
                        name: segment.clone(),
                        track_count: *track_count_under,
                        art_key: key,
                    })
                }
                FolderTreeEntry::Track { .. } => None,
            })
            .collect();
        let rows: Vec<TrackRow> = tracks.iter().map(|t| map_track(t, &mut art)).collect();
        s.art_index = art;
        FolderDetail {
            path: path.to_string(),
            name: basename(path),
            track_count: count,
            subfolders,
            tracks: rows,
        }
    })
}

/// The local album-detail pane (header + track list). `id` is the group key
/// of the ACTIVE identity mode.
pub fn load_album_detail_blocking(id: &str) -> Option<AlbumDetail> {
    let mode = group_mode();
    let tracks = with_db(|db| match mode {
        AlbumGroupMode::Metadata => db.get_album_tracks_metadata(id),
        AlbumGroupMode::Folder => db.get_album_tracks(id),
    })?;
    if tracks.is_empty() {
        return None;
    }
    let album = state(|s| {
        s.albums
            .iter()
            .chain(s.folders.iter())
            .find(|a| a.id == id)
            .cloned()
    })
    .unwrap_or_else(|| {
        // Not in a loaded page (deep link / folder card): synthesize the
        // header from the tracks themselves.
        let first = &tracks[0];
        AlbumRow {
            id: id.to_string(),
            title: first.album_group_title.clone(),
            artist: first
                .album_artist
                .clone()
                .unwrap_or_else(|| first.artist.clone()),
            year: first.year.map(|y| y.to_string()).unwrap_or_default(),
            track_count: tracks.len() as u32,
            duration: total_duration(tracks.iter().map(|t| t.duration_secs).sum()),
            quality_tier: tier_of(&first.format, first.bit_depth, first.sample_rate).into(),
            quality_detail: crate::home_qt::quality_detail_from_parts(
                first.bit_depth,
                Some(first.sample_rate),
            ),
            format: first.format.to_string(),
            art_key: album_key(id),
            source: "local".into(),
            ..Default::default()
        }
    });
    Some(state(|s| {
        let mut art = std::mem::take(&mut s.art_index);
        if let Some(p) = tracks
            .iter()
            .find_map(|t| t.artwork_path.as_ref().filter(|p| !p.is_empty()))
        {
            art.insert(album.art_key.clone(), p.clone());
        }
        let rows: Vec<TrackRow> = tracks.iter().map(|t| map_track(t, &mut art)).collect();
        s.art_index = art;
        AlbumDetail {
            album,
            tracks: rows,
        }
    }))
}

// ---------------------------------------------------------------------------
// Windowed artwork (id-keyed + evicted, the perf contract)
// ---------------------------------------------------------------------------

/// Resolve the mounted window's artKeys to 256px THUMBNAIL paths. Returns
/// `(key, file://path)` pairs; keys with no cover (or a cover that cannot
/// be thumbnailed) are dropped, so the QML map only ever grows with real
/// hits. BLOCKING (image decode) — the bridge runs it off the Qt thread.
pub fn artwork_window_blocking(keys: Vec<String>) -> Vec<(String, String)> {
    let sources: Vec<(String, String)> = state(|s| {
        keys.iter()
            .filter_map(|k| s.art_index.get(k).map(|p| (k.clone(), p.clone())))
            .collect()
    });
    let mut out = Vec::with_capacity(sources.len());
    let mut seen: HashSet<String> = HashSet::new();
    for (key, src) in sources {
        if !seen.insert(key.clone()) {
            continue;
        }
        let path = Path::new(&src);
        // A Plex thumb is a server-relative path, not a file — skip (Plex
        // is not wired, POC-NOTE).
        if src.starts_with('/') && !path.exists() {
            continue;
        }
        match qbz_library::get_or_generate_thumbnail(path) {
            Ok(thumb) => out.push((key, format!("file://{}", thumb.display()))),
            Err(_) => {
                // No thumbnail (unsupported source): fall back to the
                // original file so the card is not blank.
                if path.exists() {
                    out.push((key, format!("file://{src}")));
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Playback (the PROTECTED audio path is only ENTERED here, never modified)
// ---------------------------------------------------------------------------

/// `LocalTrack` -> `QueueTrack` — playback.rs `local_queue_track` 1:1 for
/// the local/offline arms (Plex is not wired).
fn local_queue_track(t: &LocalTrack) -> QueueTrack {
    let src = match t.source.as_deref() {
        Some("qobuz_download") => "qobuz_download",
        Some("plex") => "plex",
        _ => "local",
    };
    let is_offline = src == "qobuz_download";
    let artwork_url = t.artwork_path.as_ref().map(|p| {
        if p.starts_with("file://") {
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
        album_id: Some(t.album_group_key.clone()),
        artist_id: None,
        streamable: true,
        source: Some(src.to_string()),
        parental_warning: false,
        source_item_id_hint: if is_offline {
            Some(t.id.to_string())
        } else {
            None
        },
        context_kind: Some("local".to_string()),
        context_id: Some(t.album_group_key.clone()),
    }
}

/// Set the queue to `tracks` and start `start`. Mirrors playback.rs
/// `play_local_tracks_now`: `set_queue` then the AUDIBLE step, which for a
/// local file is the player's `play_data` seam (never
/// `play_track_resolved`, which needs a Qobuz client).
async fn play_rows(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<LocalTrack>,
    start: usize,
    shuffle: bool,
) {
    if tracks.is_empty() {
        return;
    }
    let queue: Vec<QueueTrack> = tracks.iter().map(local_queue_track).collect();
    let start = start.min(queue.len() - 1);
    let play_id = queue[start].id;
    if shuffle {
        runtime.core().set_shuffle(true).await;
        crate::now_playing::set_shuffle(true);
    }
    runtime.core().set_queue(queue, Some(start)).await;
    crate::playback_qt::publish_queue(runtime).await;
    play_local_file(runtime, play_id).await;
    crate::playback_qt::refresh_now_playing(runtime).await;
}

/// The audible step for a LOCAL queue track: read the file and hand the
/// bytes to the player. Returns false when the row can't be resolved
/// (missing db row, unmounted drive) so the caller can fall through.
///
/// DSD (.dsf/.dff) is streamed from disk by the player instead of slurped
/// (qbz-dsd Phase 1's `play_dsd_file` seam).
pub async fn play_local_file(runtime: &Arc<AppRuntime<LoggingAdapter>>, row_id: u64) -> bool {
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
    // file. If that container is already loaded, seek instead of re-reading
    // (a multi-hundred-MB re-decode per row click).
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

/// Source-aware AUDIBLE step for the shared poll-loop advance: when the
/// CURRENT queue track is a local file, play it from disk and report true;
/// otherwise report false so the Qobuz tier-walk runs unchanged.
///
/// This is the one seam the shared playback controller needs (see the
/// "GLUE NEEDED" note in the phase report): without it, auto-advance to the
/// next local track goes down `play_track_resolved` and fails with
/// "No Qobuz client available".
pub async fn play_current_if_local(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
) -> bool {
    let Some(qt) = runtime.core().current_track().await else {
        return false;
    };
    if qt.id != track_id {
        return false;
    }
    match qt.source.as_deref() {
        Some("local") => play_local_file(runtime, track_id).await,
        _ => false,
    }
}

/// Play a whole local album (its group key) from the top, or from
/// `start_track_id` when a row was clicked.
pub async fn play_album(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: String,
    start_track_id: Option<i64>,
    shuffle: bool,
) {
    let mode = group_mode();
    let key = album_id.clone();
    let tracks = tokio::task::spawn_blocking(move || {
        with_db(|db| match mode {
            AlbumGroupMode::Metadata => db.get_album_tracks_metadata(&key),
            AlbumGroupMode::Folder => db.get_album_tracks(&key),
        })
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    let start = start_track_id
        .and_then(|tid| tracks.iter().position(|t| t.id == tid))
        .unwrap_or(0);
    play_rows(runtime, tracks, start, shuffle).await;
}

/// Play everything under a folder, recursively, in path order (the tree
/// pane's circular Play).
pub async fn play_folder(runtime: &Arc<AppRuntime<LoggingAdapter>>, path: String) {
    let tracks = tokio::task::spawn_blocking(move || {
        with_db(|db| db.list_folder_tracks_recursive(&path, false)).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    play_rows(runtime, tracks, 0, false).await;
}

/// Play a folder's DIRECT tracks starting at the clicked row.
pub async fn play_folder_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    folder: String,
    row_id: i64,
) {
    let tracks = tokio::task::spawn_blocking(move || {
        with_db(|db| db.list_folder_tracks(&folder, false)).unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    let start = tracks.iter().position(|t| t.id == row_id).unwrap_or(0);
    play_rows(runtime, tracks, start, false).await;
}

/// Tracks tab row click: the ALREADY-LOADED page set becomes the queue so
/// playback continues down the list (local_library.rs' instant path — no
/// DB re-query).
pub async fn play_tracks_from(runtime: &Arc<AppRuntime<LoggingAdapter>>, row_id: i64) {
    let ids: Vec<i64> = state(|s| {
        s.tracks
            .iter()
            .filter_map(|t| t.id.parse::<i64>().ok())
            .collect()
    });
    let rows = tokio::task::spawn_blocking(move || {
        with_db(|db| {
            let mut out = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(t) = db.get_track(id)? {
                    out.push(t);
                }
            }
            Ok::<_, LibraryError>(out)
        })
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    let start = rows.iter().position(|t| t.id == row_id).unwrap_or(0);
    play_rows(runtime, rows, start, false).await;
}

/// Row / card "Play next" / "Play later" / "Add to queue".
pub async fn enqueue(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    kind: String,
    id: String,
    mode: String,
) {
    let group = group_mode();
    let k = kind.clone();
    let ident = id.clone();
    let tracks = tokio::task::spawn_blocking(move || {
        with_db(|db| match k.as_str() {
            "track" => {
                let rid: i64 = ident.parse().unwrap_or(0);
                Ok(db.get_track(rid)?.into_iter().collect::<Vec<_>>())
            }
            "folder" => db.list_folder_tracks_recursive(&ident, false),
            _ => match group {
                AlbumGroupMode::Metadata => db.get_album_tracks_metadata(&ident),
                AlbumGroupMode::Folder => db.get_album_tracks(&ident),
            },
        })
        .unwrap_or_default()
    })
    .await
    .unwrap_or_default();
    if tracks.is_empty() {
        return;
    }
    let queue: Vec<QueueTrack> = tracks.iter().map(local_queue_track).collect();
    // Same core helpers the Qobuz rows use (playback_qt::enqueue_single_track):
    // "next" inserts at the cursor (reversed so a multi-track insert keeps its
    // order), "later" appends to the #442 block tail, anything else appends.
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

// ---------------------------------------------------------------------------
// Toolbar state setters (all cheap; the loads that follow are queued by the
// bridge)
// ---------------------------------------------------------------------------

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

/// Drop every cached document (logout / user switch).
pub fn reset() {
    *LOCAL.lock().unwrap() = None;
}

// ---------------------------------------------------------------------------
// Serialization helpers (the bridge publishes strings, never structs)
// ---------------------------------------------------------------------------

pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}
