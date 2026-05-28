//! Offline cache — the implementation lives in the frontend-agnostic
//! `qbz-offline-cache` crate (ADR-006). This module re-exports it and adds
//! the Tauri-specific glue: a `CacheEventSink` that re-emits the crate's
//! `CacheEvent`s as the exact legacy `offline:caching_*` / `offline:unlock_*`
//! Tauri events the Svelte frontend listens for (1:1 IPC shapes preserved).

pub use qbz_offline_cache::*;

use tauri::{Emitter, Runtime};

/// Resolve the core Qobuz client handle from the `CoreBridge` for the
/// download pipeline. The CMAF download, metadata fetch, and legacy
/// stream-url fallback all use this single core client. Errs when no
/// session/bridge is active.
pub async fn core_client(
    bridge: &crate::core_bridge::CoreBridgeState,
) -> Result<
    std::sync::Arc<tokio::sync::RwLock<Option<qbz_qobuz::QobuzClient>>>,
    String,
> {
    let guard = bridge.0.read().await;
    guard
        .as_ref()
        .map(|b| b.core().client())
        .ok_or_else(|| "CoreBridge not initialized".to_string())
}

/// Build a [`qbz_offline_cache::CacheEventSink`] that re-emits the crate's
/// `CacheEvent`s as the legacy Tauri events. Cheap to build per call (an
/// `Arc`'d closure capturing the `AppHandle`).
pub fn tauri_cache_sink<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> qbz_offline_cache::CacheEventSink {
    use qbz_offline_cache::{CacheEvent, CacheFormat};
    std::sync::Arc::new(move |ev: CacheEvent| match ev {
        CacheEvent::Started { track_id } => {
            let _ = app.emit(
                "offline:caching_started",
                serde_json::json!({ "trackId": track_id }),
            );
        }
        CacheEvent::Progress {
            track_id,
            progress_percent,
            bytes_downloaded,
            total_bytes,
        } => {
            let _ = app.emit(
                "offline:caching_progress",
                serde_json::json!({
                    "trackId": track_id,
                    "progressPercent": progress_percent,
                    "bytesDownloaded": bytes_downloaded,
                    "totalBytes": total_bytes,
                    "status": "downloading",
                }),
            );
        }
        CacheEvent::Completed {
            track_id,
            size,
            format,
        } => {
            // Legacy path omitted `format`; CMAF path set "cmaf". Preserve both.
            let payload = match format {
                CacheFormat::Cmaf => {
                    serde_json::json!({ "trackId": track_id, "size": size, "format": "cmaf" })
                }
                CacheFormat::Flac => serde_json::json!({ "trackId": track_id, "size": size }),
            };
            let _ = app.emit("offline:caching_completed", payload);
        }
        CacheEvent::Processed {
            track_id,
            path,
            format,
        } => {
            let payload = match format {
                CacheFormat::Cmaf => {
                    serde_json::json!({ "trackId": track_id, "path": path, "format": "cmaf" })
                }
                CacheFormat::Flac => serde_json::json!({ "trackId": track_id, "path": path }),
            };
            let _ = app.emit("offline:caching_processed", payload);
        }
        CacheEvent::Failed { track_id, error } => {
            let _ = app.emit(
                "offline:caching_failed",
                serde_json::json!({ "trackId": track_id, "error": error }),
            );
        }
        CacheEvent::UnlockStart { track_id } => {
            let _ = app.emit(
                "offline:unlock_start",
                serde_json::json!({ "trackId": track_id }),
            );
        }
        CacheEvent::UnlockEnd { track_id, success } => {
            let _ = app.emit(
                "offline:unlock_end",
                serde_json::json!({ "trackId": track_id, "success": success }),
            );
        }
    })
}
