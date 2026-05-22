//! Sidebar playlists + folders controller. Builds the flattened
//! left-nav list (folder headers with their playlists + root
//! playlists) from the user's Qobuz playlists and the local folder
//! organization (library.db). The loaded data is cached so expand /
//! move operations rebuild the list without re-hitting the network.

use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::folders::FolderInfo;
use crate::{AppWindow, SidebarEntry, SidebarPlaylistItem, SidebarState};

#[derive(Clone)]
pub struct SidebarPlaylist {
    pub id: u64,
    pub name: String,
}

#[derive(Clone, Default)]
pub struct SidebarData {
    pub playlists: Vec<SidebarPlaylist>,
    pub folders: Vec<FolderInfo>,
    pub folder_map: HashMap<u64, String>,
}

/// Session-only folder expand state (matches Tauri — not persisted).
static EXPANDED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
/// Last loaded data, so expand/move rebuild without a refetch.
static CACHE: LazyLock<Mutex<SidebarData>> = LazyLock::new(|| Mutex::new(SidebarData::default()));

pub fn set_loading(window: &AppWindow, loading: bool) {
    window.global::<SidebarState>().set_loading(loading);
}

/// Fetch playlists (Qobuz) + folders + folder membership (local).
pub async fn load<A>(runtime: &AppRuntime<A>) -> SidebarData
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let playlists = match runtime.core().get_user_playlists().await {
        Ok(pls) => pls
            .into_iter()
            .map(|p| SidebarPlaylist {
                id: p.id,
                name: p.name,
            })
            .collect(),
        Err(e) => {
            log::warn!("[qbz-slint] sidebar playlists load failed: {e}");
            Vec::new()
        }
    };
    let (folders, folder_map) = tokio::task::spawn_blocking(|| {
        (
            crate::folders::load_folders(),
            crate::folders::playlist_folder_map(),
        )
    })
    .await
    .unwrap_or_default();
    SidebarData {
        playlists,
        folders,
        folder_map,
    }
}

/// Store the freshly-loaded data and render it.
pub fn apply(window: &AppWindow, data: SidebarData) {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = data;
    }
    rebuild(window);
}

/// Rebuild the flattened entries (+ the folders list for the
/// move-to-folder menu) from the cache + expand state.
pub fn rebuild(window: &AppWindow) {
    let data = CACHE.lock().map(|c| c.clone()).unwrap_or_default();
    let expanded = EXPANDED.lock().map(|e| e.clone()).unwrap_or_default();
    let folder_ids: HashSet<&String> = data.folders.iter().map(|f| &f.id).collect();

    let mut entries: Vec<SidebarEntry> = Vec::new();
    for folder in &data.folders {
        let is_exp = expanded.contains(&folder.id);
        let members: Vec<&SidebarPlaylist> = data
            .playlists
            .iter()
            .filter(|p| data.folder_map.get(&p.id).map(|f| f == &folder.id).unwrap_or(false))
            .collect();
        entries.push(SidebarEntry {
            kind: "folder".into(),
            id: folder.id.clone().into(),
            name: folder.name.clone().into(),
            expanded: is_exp,
            count: members.len() as i32,
            indent: false,
        });
        if is_exp {
            for p in members {
                entries.push(SidebarEntry {
                    kind: "playlist".into(),
                    id: p.id.to_string().into(),
                    name: p.name.clone().into(),
                    expanded: false,
                    count: 0,
                    indent: true,
                });
            }
        }
    }
    // Root playlists — no folder, or a folder that no longer exists.
    for p in &data.playlists {
        let in_folder = data
            .folder_map
            .get(&p.id)
            .map(|f| folder_ids.contains(f))
            .unwrap_or(false);
        if !in_folder {
            entries.push(SidebarEntry {
                kind: "playlist".into(),
                id: p.id.to_string().into(),
                name: p.name.clone().into(),
                expanded: false,
                count: 0,
                indent: false,
            });
        }
    }

    let folders: Vec<SidebarPlaylistItem> = data
        .folders
        .iter()
        .map(|f| SidebarPlaylistItem {
            id: f.id.clone().into(),
            name: f.name.clone().into(),
        })
        .collect();

    let state = window.global::<SidebarState>();
    state.set_entries(ModelRc::new(VecModel::from(entries)));
    state.set_folders(ModelRc::new(VecModel::from(folders)));
    state.set_loading(false);
}

/// Toggle a folder's expanded state, then re-render from cache.
pub fn toggle_folder(window: &AppWindow, folder_id: &str) {
    if let Ok(mut exp) = EXPANDED.lock() {
        if !exp.remove(folder_id) {
            exp.insert(folder_id.to_string());
        }
    }
    rebuild(window);
}

/// Optimistically move a playlist in the cache (folder_id "" = root)
/// and re-render. The DB write happens separately.
pub fn move_playlist_local(window: &AppWindow, playlist_id: u64, folder_id: &str) {
    if let Ok(mut cache) = CACHE.lock() {
        if folder_id.is_empty() {
            cache.folder_map.remove(&playlist_id);
        } else {
            cache.folder_map.insert(playlist_id, folder_id.to_string());
        }
    }
    rebuild(window);
}

/// Highlight the open playlist in the sidebar (or clear with "").
pub fn set_active(window: &AppWindow, id: &str) {
    window.global::<SidebarState>().set_active_id(id.into());
}

/// Whether `id` is one of the user's own playlists — used to gate
/// playlist editing.
pub fn contains(window: &AppWindow, id: &str) -> bool {
    use slint::Model;
    let entries = window.global::<SidebarState>().get_entries();
    entries
        .iter()
        .any(|e| e.kind == "playlist" && e.id == id)
}
