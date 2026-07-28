//! Sidebar playlists + folders controller — Slint-free port of
//! `crates/qbz/src/sidebar.rs` (load / sort / search / expand / rebuild).
//!
//! Model: Qobuz playlists (`get_user_playlists`) + folder metadata from the
//! per-user library.db (`qbz-library` queries — same ones `folders.rs`
//! wraps), session-only expand state (Tauri parity — not persisted),
//! 5-option sort with direction toggle (#657), recursive name search.
//! Entries publish as ONE JSON document (`sidebarJson`).
//!
//! POC-NOTEs:
//! - LOCAL playlists (library.db `local:<uuid>` entities), the offline
//!   D11.b synthesis, hidden-playlist filtering needs the hidden flag (it
//!   IS read and applied), move-to-folder, folder edit/delete, context
//!   menus, and the mini-state folder flyout: out of scope. Playlist
//!   click only marks the row active (no playlist view yet).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use qbz_app::settings::pinned_items::{PinnedItemsService, PinnedItem, DB_FILE_NAME};
use qbz_app::shell::AppRuntime;
use qbz_app::user_data::UserDataPaths;
use qbz_core::LoggingAdapter;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Pinned items store (PinnedItemsService, ADR-006 backend) — same lifecycle
// as the local-favorites store.
// ---------------------------------------------------------------------------

static PINNED: OnceLock<Mutex<PinnedItemsService>> = OnceLock::new();
/// The per-user base dir the pinned store was bound to (also where the
/// discover-prefs DB lives) — stashed so Home can read section prefs.
static USER_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Bind `<dir>/pinned_items.db` on every session activation (mirrors
/// `pinned::init_for_user`; fail-open).
pub fn init_pinned(base_dir: &Path) {
    let _ = USER_DIR.set(base_dir.to_path_buf());
    let path = base_dir.join(DB_FILE_NAME);
    match PinnedItemsService::new(&path) {
        Ok(service) => {
            let _ = PINNED.set(Mutex::new(service));
            log::info!("[qbz-qt] pinned items store opened");
        }
        Err(e) => log::error!("[qbz-qt] pinned items store open failed: {e}"),
    }
}

pub fn is_pinned(kind: &str, id: &str) -> bool {
    PINNED
        .get()
        .map(|s| s.lock().unwrap().is_pinned(kind, id))
        .unwrap_or(false)
}

/// All pinned items, most-recent first (PinnedItemsService::list) — feeds
/// the Home "Pinned" rail.
pub fn list_pinned() -> Vec<PinnedItem> {
    PINNED
        .get()
        .and_then(|s| s.lock().unwrap().list().ok())
        .unwrap_or_default()
}

/// The per-user base dir (None before any session activation).
pub fn user_dir() -> Option<std::path::PathBuf> {
    USER_DIR.get().cloned()
}

/// Toggle the pin state of an album/artist/playlist. Returns the new
/// state, or None with no store bound / on error.
pub fn toggle_pin(kind: &str, id: &str, title: &str, subtitle: &str, artwork_url: &str) -> Option<bool> {
    let service = PINNED.get()?.lock().unwrap();
    if service.is_pinned(kind, id) {
        service.unpin(kind, id).ok()?;
        Some(false)
    } else {
        service
            .pin(&PinnedItem {
                kind: kind.to_string(),
                id: id.to_string(),
                title: title.to_string(),
                subtitle: subtitle.to_string(),
                artwork_url: artwork_url.to_string(),
                pinned_at: 0,
            })
            .ok()?;
        Some(true)
    }
}

// ---------------------------------------------------------------------------
// Sidebar model
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct SidebarEntry {
    pub kind: String, // "folder" | "playlist"
    pub id: String,
    pub name: String,
    pub expanded: bool,
    pub count: i32,
    pub indent: bool,
    #[serde(rename = "folderId")]
    pub folder_id: String,
    /// Up to 4 de-duplicated cover urls for the 2x2 micro-collage.
    pub covers: Vec<String>,
}

#[derive(Clone)]
struct SidebarPlaylist {
    id: u64,
    name: String,
    tracks_count: u32,
    cover_urls: Vec<String>,
    position: i32,
}

#[derive(Default, Clone)]
struct SidebarData {
    playlists: Vec<SidebarPlaylist>,
    /// (id, name), hidden folders already excluded.
    folders: Vec<(String, String)>,
    /// playlist id -> folder id.
    folder_map: HashMap<u64, String>,
    hidden_playlists: HashSet<u64>,
}

static CACHE: Mutex<Option<SidebarData>> = Mutex::new(None);
/// Session-only folder expand state (matches Tauri — not persisted).
static EXPANDED: std::sync::LazyLock<Mutex<HashSet<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));
/// (option, asc) — session scope (sidebar.rs SORT; Tauri persists in
/// localStorage, Slint keeps it in-session).
static SORT: std::sync::LazyLock<Mutex<(String, bool)>> =
    std::sync::LazyLock::new(|| Mutex::new(("name".to_string(), true)));
static SEARCH: Mutex<String> = Mutex::new(String::new());

/// Fetch playlists (Qobuz) + folders + membership + hidden set (library.db).
pub async fn load(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    log::debug!("[qbz-qt] sidebar load: fetching user playlists");
    let playlists: Vec<SidebarPlaylist> = match runtime.core().get_user_playlists().await {
        Ok(pls) => pls
            .into_iter()
            .map(|p| {
                let cover_urls = {
                    let source = [&p.images300, &p.images150, &p.images]
                        .into_iter()
                        .flatten()
                        .find(|v| !v.is_empty());
                    let mut out: Vec<String> = Vec::new();
                    if let Some(list) = source {
                        for url in list {
                            if !url.is_empty() && !out.contains(url) {
                                out.push(url.clone());
                            }
                            if out.len() == 4 {
                                break;
                            }
                        }
                    }
                    out
                };
                SidebarPlaylist {
                    id: p.id,
                    name: p.name,
                    tracks_count: p.tracks_count,
                    cover_urls,
                    position: 0,
                }
            })
            .collect(),
        Err(e) => {
            log::warn!("[qbz-qt] sidebar playlists load failed: {e}");
            Vec::new()
        }
    };

    log::debug!("[qbz-qt] sidebar load: playlists fetch settled, reading folders");
    let (folders, folder_map, positions, hidden_playlists) = folders_blocking();
    log::debug!("[qbz-qt] sidebar load: folders read");
    let mut playlists = playlists;
    for p in &mut playlists {
        if let Some(pos) = positions.get(&p.id) {
            p.position = *pos;
        }
    }
    let (n_playlists, n_folders) = (playlists.len(), folders.len());
    *CACHE.lock().unwrap() = Some(SidebarData {
        playlists,
        folders,
        folder_map,
        hidden_playlists,
    });
    log::info!("[qbz-qt] sidebar loaded: {n_playlists} playlists, {n_folders} folders");
}

/// library.db queries (folders.rs equivalents), run inline — rusqlite is
/// sync but the three reads are tiny.
fn folders_blocking() -> (Vec<(String, String)>, HashMap<u64, String>, HashMap<u64, i32>, HashSet<u64>) {
    let Some(uid) = UserDataPaths::load_last_user_id() else {
        return Default::default();
    };
    let Some(path) = dirs::data_dir().map(|p| {
        p.join("qbz")
            .join("users")
            .join(uid.to_string())
            .join("library.db")
    }) else {
        return Default::default();
    };
    if !path.exists() {
        return Default::default();
    }
    let Ok(db) = qbz_library::LibraryDatabase::open(&path) else {
        return Default::default();
    };
    let folders: Vec<(String, String)> = db
        .get_all_playlist_folders()
        .unwrap_or_default()
        .into_iter()
        .filter(|f| !f.is_hidden)
        .map(|f| (f.id, f.name))
        .collect();
    let mut folder_map = HashMap::new();
    let mut positions = HashMap::new();
    let mut hidden = HashSet::new();
    for s in db.get_all_playlist_settings().unwrap_or_default() {
        if let Some(fid) = s.folder_id {
            folder_map.insert(s.qobuz_playlist_id, fid);
        }
        positions.insert(s.qobuz_playlist_id, s.position);
        if s.hidden {
            hidden.insert(s.qobuz_playlist_id);
        }
    }
    (folders, folder_map, positions, hidden)
}

/// SidebarPlaylist order for the active sort (sidebar.rs comparators:
/// `recent`/`playcount` keep API order = newest-first; asc==true is always
/// the natural direction).
fn sort_playlists(playlists: &[SidebarPlaylist]) -> Vec<SidebarPlaylist> {
    let (sort, asc) = SORT.lock().unwrap().clone();
    let mut out: Vec<SidebarPlaylist> = playlists.to_vec();
    match sort.as_str() {
        "name" => out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        "tracks" => out.sort_by(|a, b| b.tracks_count.cmp(&a.tracks_count)),
        "custom" => out.sort_by(|a, b| a.position.cmp(&b.position)),
        _ => {}
    }
    if !asc {
        out.reverse();
    }
    out
}

/// Rebuild the flattened entries from cache + expand + search and return
/// them (the caller publishes).
pub fn rebuild() -> Vec<SidebarEntry> {
    let Some(data) = CACHE.lock().unwrap().clone() else {
        return Vec::new();
    };
    let expanded = EXPANDED.lock().unwrap().clone();
    let query = SEARCH.lock().unwrap().clone();
    let searching = !query.is_empty();
    let folder_ids: HashSet<&String> = data.folders.iter().map(|f| &f.0).collect();

    let sorted = sort_playlists(&data.playlists);
    let matches =
        |p: &SidebarPlaylist| !searching || p.name.to_lowercase().contains(query.as_str());

    let entry_for = |p: &SidebarPlaylist, indent: bool, folder_id: &str| SidebarEntry {
        kind: "playlist".into(),
        id: p.id.to_string(),
        name: p.name.clone(),
        expanded: false,
        count: 0,
        indent,
        folder_id: folder_id.to_string(),
        covers: p.cover_urls.clone(),
    };

    let mut entries: Vec<SidebarEntry> = Vec::new();
    for (fid, fname) in &data.folders {
        let members: Vec<&SidebarPlaylist> = sorted
            .iter()
            .filter(|p| data.folder_map.get(&p.id).map(|f| f == fid).unwrap_or(false))
            .filter(|p| matches(p))
            .filter(|p| !data.hidden_playlists.contains(&p.id))
            .collect();
        // While searching, skip folders with no matching playlists.
        if searching && members.is_empty() {
            continue;
        }
        // When searching, force-expand so matches inside are visible.
        let is_exp = searching || expanded.contains(fid);
        entries.push(SidebarEntry {
            kind: "folder".into(),
            id: fid.clone(),
            name: fname.clone(),
            expanded: is_exp,
            count: members.len() as i32,
            indent: false,
            folder_id: String::new(),
            covers: Vec::new(),
        });
        if is_exp {
            for p in members {
                entries.push(entry_for(p, true, fid));
            }
        }
    }
    // Root playlists — no folder, or a folder that no longer exists.
    for p in &sorted {
        let in_folder = data
            .folder_map
            .get(&p.id)
            .map(|f| folder_ids.contains(f))
            .unwrap_or(false);
        if !in_folder && matches(p) && !data.hidden_playlists.contains(&p.id) {
            entries.push(entry_for(p, false, ""));
        }
    }
    entries
}

/// PlaylistView-style sort toggle (#657): re-pick flips direction, a new
/// option starts at its natural direction (asc == true everywhere).
pub fn set_sort(option: &str) {
    let opt = match option {
        "name" | "recent" | "tracks" | "playcount" | "custom" => option,
        _ => "name",
    };
    let mut s = SORT.lock().unwrap();
    if s.0 == opt {
        s.1 = !s.1;
    } else {
        s.0 = opt.to_string();
        s.1 = true;
    }
}

pub fn sort_state() -> (String, bool) {
    SORT.lock().unwrap().clone()
}

pub fn set_search(query: &str) {
    *SEARCH.lock().unwrap() = query.trim().to_lowercase();
}

/// Toggle a folder's expand state (or set it explicitly for the tree's
/// collapse-all). Returns the new state.
pub fn toggle_folder(id: &str) -> bool {
    let mut expanded = EXPANDED.lock().unwrap();
    if !expanded.insert(id.to_string()) {
        expanded.remove(id);
        false
    } else {
        true
    }
}
