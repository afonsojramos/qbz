//! My QBZ — custom-cover upload / remove (Phase-2 Slice 7).
//!
//! Mirrors Tauri's `v2_mixtape_upload_custom_cover` /
//! `v2_mixtape_remove_custom_cover` (spec 40 §7) 1:1, and the album
//! custom-cover convention (same artwork cache dir, same 1000×1000 Lanczos3
//! resize, same failure-safe "persist before deleting the previous file"
//! ordering):
//!
//! - **Upload:** validate the picked file's extension (png/jpg/jpeg/webp),
//!   read the previous `custom_artwork_path` (to delete after persist), decode
//!   + `resize(1000, 1000, Lanczos3)`, save as `mixtape_custom_{safe_id}_{epoch_secs}.jpg`
//!   in `qbz_library::get_artwork_cache_dir()`, persist via
//!   `repo::set_custom_artwork(Some(dest))`, then delete the previous file only
//!   if it differs.
//! - **Remove:** read previous, `repo::set_custom_artwork(None)`, delete prev.
//!
//! Frontend-agnostic (ADR-005/006): the persistence is `qbz_mixtape::repo`
//! reached directly through `crate::library_db::with_db`; no Tauri command
//! wrappers. All blocking work (file decode/resize/IO + DB) runs on a
//! `spawn_blocking` worker; the reload + toast hop back to the event loop.
//!
//! NOTE on webp: the workspace `image` crate is built with only the `jpeg` +
//! `png` features, so a `.webp` source decodes to an error at runtime (the
//! extension is accepted to match the Tauri picker filter, but the decode
//! surfaces the "upload failed" toast). png/jpg/jpeg are the working formats.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::imageops::FilterType;
use slint::ComponentHandle;

use crate::artwork::ImageCache;
use crate::AppWindow;

/// The four extensions the Tauri picker accepts (spec §7.1 step 1).
const ALLOWED_EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

/// Epoch SECONDS (NOT ms) — the custom-cover filename timestamp is in seconds
/// (spec §1.6 / §7.1 step 4).
fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Keep ascii-alphanumeric / `-` / `_`; everything else becomes `_` (spec
/// §7.1 step 5). The collection id is a UUID, so this is normally a no-op.
fn safe_id(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

/// Read the stored `custom_artwork_path` for a collection (used to delete the
/// previous file after a new one persists). Runs synchronously via `with_db`.
fn get_prev_path(id: &str) -> Option<String> {
    crate::library_db::with_db(|db| {
        Ok(db.with_connection(|conn| {
            qbz_mixtape::repo::get_custom_artwork(conn, id).unwrap_or(None)
        }))
    })
    .flatten()
}

/// Persist `path` (or clear with None) into `custom_artwork_path`. Returns true
/// on success. Runs synchronously via `with_db`.
fn set_custom_artwork(id: &str, path: Option<&str>) -> bool {
    let id = id.to_string();
    let path = path.map(|p| p.to_string());
    crate::library_db::with_db(move |db| {
        db.with_connection(|conn| {
            qbz_mixtape::repo::set_custom_artwork(conn, &id, path.as_deref())
        })
        .map_err(|e| {
            qbz_library::LibraryError::Database(format!("set_custom_artwork failed: {e}"))
        })
    })
    .is_some()
}

/// Decode `source`, resize to 1000×1000 (Lanczos3), and save as a JPEG at
/// `dest`. Returns an error string on any failure (decode / resize / save).
fn resize_and_save(source: &Path, dest: &Path) -> Result<(), String> {
    let img = image::open(source).map_err(|e| format!("decode failed: {e}"))?;
    let resized = img.resize(1000, 1000, FilterType::Lanczos3);
    resized
        .to_rgb8()
        .save_with_format(dest, image::ImageFormat::Jpeg)
        .map_err(|e| format!("save failed: {e}"))
}

/// The blocking upload body (extension validation → resize/save → persist →
/// delete-prev). Returns Ok(dest) or Err(reason). Mirrors the Tauri command
/// step order exactly (persist BEFORE deleting the previous file, and only
/// delete when it differs).
fn do_upload(id: &str, source_path: &str) -> Result<String, String> {
    let source = PathBuf::from(source_path);
    if !source.exists() {
        return Err("source file not found".to_string());
    }
    // 1 — extension validation.
    let ext_ok = source
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| ALLOWED_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false);
    if !ext_ok {
        return Err("unsupported image type".to_string());
    }

    // 2 — read previous path (to delete after persist).
    let prev = get_prev_path(id);

    // 3-6 — build the destination filename in the shared artwork cache dir.
    let artwork_dir = qbz_library::get_artwork_cache_dir();
    let filename = format!("mixtape_custom_{}_{}.jpg", safe_id(id), epoch_secs());
    let dest = artwork_dir.join(&filename);

    // 7 — decode + resize + save.
    resize_and_save(&source, &dest)?;

    // 8 — persist the new path.
    let dest_str = dest.to_string_lossy().to_string();
    if !set_custom_artwork(id, Some(&dest_str)) {
        // Persist failed — clean up the orphan we just wrote, surface the error.
        let _ = std::fs::remove_file(&dest);
        return Err("failed to save cover".to_string());
    }

    // 9 — delete the previous file AFTER persist, only if it differs.
    if let Some(prev) = prev {
        if prev != dest_str {
            let _ = std::fs::remove_file(&prev);
        }
    }

    Ok(dest_str)
}

/// The blocking remove body: read previous → clear → delete prev file.
fn do_remove(id: &str) -> Result<(), String> {
    let prev = get_prev_path(id);
    if !set_custom_artwork(id, None) {
        return Err("failed to clear cover".to_string());
    }
    if let Some(prev) = prev {
        let _ = std::fs::remove_file(&prev);
    }
    Ok(())
}

/// Hero overflow "Set custom cover": open the native image picker, then upload
/// + resize + persist + reload. Toasts success/failure (spec §10 item 3).
pub fn upload(
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    image_cache: ImageCache,
    id: String,
) {
    handle.clone().spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title("Choose a cover image")
            .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
            .await
        else {
            return; // user cancelled — no toast.
        };
        let source = file.path().to_string_lossy().to_string();

        let upload_id = id.clone();
        let result = tokio::task::spawn_blocking(move || do_upload(&upload_id, &source))
            .await
            .unwrap_or_else(|e| Err(format!("upload task panicked: {e}")));

        match result {
            Ok(_) => {
                crate::toast::success_weak(&weak, "Cover updated");
                reload(weak.clone(), handle.clone(), image_cache.clone(), id);
            }
            Err(e) => {
                log::warn!("[qbz-slint] myqbz_cover upload failed: {e}");
                crate::toast::error_weak(&weak, "Failed to upload cover");
            }
        }
    });
}

/// Hero overflow "Remove custom cover": clear + delete the file + reload.
pub fn remove(
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    image_cache: ImageCache,
    id: String,
) {
    handle.clone().spawn(async move {
        let remove_id = id.clone();
        let result = tokio::task::spawn_blocking(move || do_remove(&remove_id))
            .await
            .unwrap_or_else(|e| Err(format!("remove task panicked: {e}")));

        match result {
            Ok(()) => {
                crate::toast::success_weak(&weak, "Cover removed");
                reload(weak.clone(), handle.clone(), image_cache.clone(), id);
            }
            Err(e) => {
                log::warn!("[qbz-slint] myqbz_cover remove failed: {e}");
                crate::toast::error_weak(&weak, "Failed to remove cover");
            }
        }
    });
}

/// Reload the open detail view so the hero reflects the new cover. Re-runs the
/// detail navigator's load/apply/artwork path for the same id (Tauri's
/// "-> reload"). The `set_view` inside `navigate` is harmless (we're already on
/// the detail view).
fn reload(
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    image_cache: ImageCache,
    id: String,
) {
    let _ = weak.upgrade_in_event_loop(move |w| {
        let _ = &w; // keep the closure's capture explicit.
        crate::myqbz_detail::navigate(w.as_weak(), handle.clone(), image_cache.clone(), id);
    });
}
