//! Playlist folders — local-only organization stored in library.db
//! (shared with the Tauri app). Folders are flat (no nesting); a
//! playlist belongs to at most one folder via
//! `playlist_settings.folder_id`. All ops are blocking (they open the
//! DB), so async callers wrap them in `tokio::task::spawn_blocking`.

use std::collections::HashMap;

use crate::library_db;

#[derive(Clone)]
pub struct FolderInfo {
    pub id: String,
    pub name: String,
}

/// All folders, ordered by their stored position.
pub fn load_folders() -> Vec<FolderInfo> {
    library_db::with_db(|db| db.get_all_playlist_folders())
        .unwrap_or_default()
        .into_iter()
        .map(|f| FolderInfo {
            id: f.id,
            name: f.name,
        })
        .collect()
}

/// playlist id -> folder id, for grouping playlists under folders.
pub fn playlist_folder_map() -> HashMap<u64, String> {
    library_db::with_db(|db| db.get_all_playlist_settings())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| s.folder_id.map(|fid| (s.qobuz_playlist_id, fid)))
        .collect()
}

pub fn create_folder(name: &str) -> Option<FolderInfo> {
    library_db::with_db(|db| db.create_playlist_folder(name, None, None, None)).map(|f| {
        FolderInfo {
            id: f.id,
            name: f.name,
        }
    })
}

pub fn rename_folder(id: &str, name: &str) {
    library_db::with_db(|db| {
        db.update_playlist_folder(id, Some(name), None, None, None, None, None)
    });
}

pub fn delete_folder(id: &str) {
    library_db::with_db(|db| db.delete_playlist_folder(id));
}

/// Move a playlist into `folder_id`, or to root when None.
pub fn move_playlist(playlist_id: u64, folder_id: Option<&str>) {
    library_db::with_db(|db| db.move_playlist_to_folder(playlist_id, folder_id));
}
