use std::sync::Arc;
use axum::extract::{Path, Query};
use axum::Json;
use serde::Deserialize;

use crate::daemon::DaemonCore;

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

fn default_limit() -> u32 { 50 }

pub async fn get_albums(daemon: Arc<DaemonCore>) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    let albums = db.get_albums(false)
        .map_err(|e| format!("Failed to get albums: {}", e))?;
    Ok(Json(serde_json::to_value(albums).unwrap_or_default()))
}

pub async fn get_artists(daemon: Arc<DaemonCore>) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    let artists = db.get_artists()
        .map_err(|e| format!("Failed to get artists: {}", e))?;
    Ok(Json(serde_json::to_value(artists).unwrap_or_default()))
}

pub async fn get_album_tracks(
    daemon: Arc<DaemonCore>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    let tracks = db.get_album_tracks(&key)
        .map_err(|e| format!("Failed to get tracks: {}", e))?;
    Ok(Json(serde_json::to_value(tracks).unwrap_or_default()))
}

pub async fn search_library(
    daemon: Arc<DaemonCore>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    let tracks = db.search(&q.q, q.limit)
        .map_err(|e| format!("Search failed: {}", e))?;
    Ok(Json(serde_json::to_value(tracks).unwrap_or_default()))
}

pub async fn get_stats(daemon: Arc<DaemonCore>) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    let stats = db.get_stats(true)
        .map_err(|e| format!("Stats failed: {}", e))?;
    Ok(Json(serde_json::to_value(stats).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct AddFolderRequest {
    pub path: String,
}

pub async fn get_folders(daemon: Arc<DaemonCore>) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    let folders = db.get_folders()
        .map_err(|e| format!("Failed to get folders: {}", e))?;
    Ok(Json(serde_json::to_value(folders).unwrap_or_default()))
}

pub async fn add_folder(
    daemon: Arc<DaemonCore>,
    Json(req): Json<AddFolderRequest>,
) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    db.add_folder(&req.path)
        .map_err(|e| format!("Failed to add folder: {}", e))?;
    Ok(Json(serde_json::json!({"path": req.path, "status": "added"})))
}

/// Trigger a library scan on all registered folders.
/// Runs in background, returns immediately.
///
/// Note: Full metadata extraction requires the MetadataExtractor from
/// src-tauri/src/library/ which is not yet in the qbz-library crate.
/// This endpoint uses the scanner to find audio files and logs results.
/// Full metadata-aware scanning will be available when MetadataExtractor
/// is moved to the crate.
pub async fn start_scan(daemon: Arc<DaemonCore>) -> Result<Json<serde_json::Value>, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    let folders = db.get_folders()
        .map_err(|e| format!("Failed to get folders: {}", e))?;

    if folders.is_empty() {
        return Ok(Json(serde_json::json!({"status": "no_folders", "message": "No folders configured. Add folders first."})));
    }

    let folder_count = folders.len();

    // Spawn scan in background
    tokio::task::spawn_blocking(move || {
        let scanner = qbz_library::LibraryScanner::new();
        let mut total_files = 0usize;

        for folder in &folders {
            let path = std::path::Path::new(folder);
            if !path.exists() {
                log::warn!("[qbzd] Scan: folder does not exist: {}", folder);
                continue;
            }
            log::info!("[qbzd] Scanning: {}", folder);
            match scanner.scan_directory(path) {
                Ok(result) => {
                    let count = result.audio_files.len();
                    total_files += count;
                    log::info!(
                        "[qbzd] Scanned {}: {} audio files, {} CUE files",
                        folder, count, result.cue_files.len()
                    );
                }
                Err(e) => {
                    log::error!("[qbzd] Scan failed for {}: {}", folder, e);
                }
            }
        }
        log::info!("[qbzd] Library scan complete: {} audio files found", total_files);
    });

    Ok(Json(serde_json::json!({
        "status": "scanning",
        "folders": folder_count,
    })))
}

#[derive(Deserialize)]
pub struct RemoveFolderRequest {
    pub path: String,
}

pub async fn remove_folder(
    daemon: Arc<DaemonCore>,
    Json(req): Json<RemoveFolderRequest>,
) -> Result<&'static str, String> {
    let user = daemon.user.read().await;
    let session = user.as_ref().ok_or("No active session")?;
    let db_path = session.data_dir.join("library.db");

    let db = qbz_library::LibraryDatabase::open(&db_path)
        .map_err(|e| format!("Library DB error: {}", e))?;
    db.remove_folder(&req.path)
        .map_err(|e| format!("Failed to remove folder: {}", e))?;
    Ok("ok")
}
