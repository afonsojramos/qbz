//! Settings > Local Library — the folder table, the scan engine, maintenance,
//! the danger zone, and the Plex fields the panel needs on top of what the
//! `QbzLocal` bridge already publishes.
//!
//! ADR-006: nothing here re-implements scanning, folder identity or Plex.
//! Folder CRUD is `qbz_library::LibraryDatabase`; the scan is
//! `qbz_library::scan_with_progress` (the SAME engine the Slint frontend
//! drives, cancellation and cleanup included); Plex rides `crate::local_plex`
//! (wired this round) and the `QbzLocal` invokables.
//!
//! Deltas vs `settings/LocalLibrarySettings.slint` (reported, not hidden):
//! - "Add folder" is a PATH FIELD + button, not the native folder chooser:
//!   the picker helper lives private in `local_ephemeral.rs` and the port has
//!   no `rfd` dependency. `open_path`-style behaviour, same end state.
//! - The per-folder EDIT modal (`LibFolderEditModal.slint`: alias + network
//!   overrides) is not ported, so no edit affordance is shown.
//! - The Plex PIN flow (generate code / link code / open sign-in) is not
//!   ported; the manual server-url + token path is what this wires.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{LazyLock, Mutex};

use qbz_library::{LibraryDatabase, ScanEvent};
use serde::Serialize;

// ---------------------------------------------------------------------------
// Transport rows (the `library` sub-document of the settings snapshot)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct FolderRow {
    pub id: i64,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub path: String,
    /// Unix seconds; 0 = never scanned. QML formats it (locale-aware).
    #[serde(rename = "lastScan")]
    pub last_scan: i64,
    pub enabled: bool,
    #[serde(rename = "isNetwork")]
    pub is_network: bool,
    /// Network folders only: whether the mount answers right now.
    pub accessible: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct PlexFields {
    /// The persisted (resolved) server url — prefills the address field.
    #[serde(rename = "serverUrl")]
    pub server_url: String,
    /// A token is stored (the token itself NEVER leaves Rust).
    #[serde(rename = "hasToken")]
    pub has_token: bool,
    #[serde(rename = "metadataWrite")]
    pub metadata_write: bool,
    /// Section collapse state (ui_prefs, same idea as the scrobbler header).
    pub collapsed: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct Snapshot {
    pub folders: Vec<FolderRow>,
    pub scanning: bool,
    pub processed: i32,
    pub total: i32,
    pub file: String,
    pub cleaning: bool,
    #[serde(rename = "cleanupStatus")]
    pub cleanup_status: String,
    pub clearing: bool,
    /// Inline result of the last add/remove (empty when there is nothing to say).
    pub status: String,
    pub plex: PlexFields,
}

// ---------------------------------------------------------------------------
// Live state
// ---------------------------------------------------------------------------

static SCANNING: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static PROCESSED: AtomicU32 = AtomicU32::new(0);
static TOTAL: AtomicU32 = AtomicU32::new(0);
static CURRENT_FILE: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
static CLEANING: AtomicBool = AtomicBool::new(false);
static CLEANUP_STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
static CLEARING: AtomicBool = AtomicBool::new(false);
static STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

fn set_status(text: String) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

/// Open (creating if needed) the per-user `library.db`.
///
/// `local_state::with_db` deliberately returns `None` when the file does not
/// exist yet (a browse read must never create an empty library); adding the
/// FIRST folder is exactly the case that has to create it, so this module
/// owns a creating opener.
fn open_db() -> Option<LibraryDatabase> {
    let path = crate::local_state::db_path()?;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match LibraryDatabase::open(&path) {
        Ok(db) => Some(db),
        Err(e) => {
            log::error!("[qbz-qt] library db open failed: {e}");
            None
        }
    }
}

fn basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

// ---------------------------------------------------------------------------
// Snapshot (called from the settings publish, on the blocking pool)
// ---------------------------------------------------------------------------

pub fn snapshot() -> Snapshot {
    let folders = crate::local_state::with_db(|db| db.get_folders_with_metadata())
        .unwrap_or_default()
        .into_iter()
        .map(|f| {
            let accessible = if f.is_network {
                std::fs::read_dir(&f.path).is_ok()
            } else {
                true
            };
            FolderRow {
                display_name: match f.alias.as_deref() {
                    Some(a) if !a.is_empty() => a.to_string(),
                    _ => basename(&f.path),
                },
                id: f.id,
                path: f.path,
                last_scan: f.last_scan.unwrap_or(0),
                enabled: f.enabled,
                is_network: f.is_network,
                accessible,
            }
        })
        .collect();

    let plex_cfg = crate::local_plex::settings();
    Snapshot {
        folders,
        scanning: SCANNING.load(Ordering::SeqCst),
        processed: PROCESSED.load(Ordering::SeqCst) as i32,
        total: TOTAL.load(Ordering::SeqCst) as i32,
        file: CURRENT_FILE.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        cleaning: CLEANING.load(Ordering::SeqCst),
        cleanup_status: CLEANUP_STATUS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone(),
        clearing: CLEARING.load(Ordering::SeqCst),
        status: STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        plex: PlexFields {
            has_token: !plex_cfg.token.trim().is_empty(),
            server_url: plex_cfg.base_url,
            metadata_write: plex_cfg.metadata_write_enabled,
            collapsed: super::pref_bool("plex_ui_collapsed", false),
        },
    }
}

// ---------------------------------------------------------------------------
// Folder CRUD
// ---------------------------------------------------------------------------

/// "Add folder": validate the path, detect network-ness, insert.
pub async fn add_folder(path: String) {
    let path = path.trim().to_string();
    if path.is_empty() {
        return;
    }
    let _ = tokio::task::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        if !p.is_dir() {
            set_status(qbz_i18n::t("That folder does not exist."));
            return;
        }
        let canonical = std::fs::canonicalize(p)
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.clone());
        let is_network = qbz_library::is_network_path(std::path::Path::new(&canonical));
        let fs_label = if is_network {
            qbz_library::network_fs_label(std::path::Path::new(&canonical))
        } else {
            None
        };
        let Some(db) = open_db() else {
            set_status(qbz_i18n::t("Could not open the library database."));
            return;
        };
        match db.add_folder_with_network_info(&canonical, is_network, fs_label.as_deref()) {
            Ok(_) => set_status(String::new()),
            Err(e) => {
                log::error!("[qbz-qt] add folder failed: {e}");
                set_status(qbz_i18n::t("Could not add that folder."));
            }
        }
    })
    .await;
    refresh_browse();
}

/// Remove folders (their indexed tracks go with them — `remove_folder_with_tracks`).
pub async fn remove_folders(ids: Vec<i64>) {
    if ids.is_empty() {
        return;
    }
    let _ = tokio::task::spawn_blocking(move || {
        let Some(db) = open_db() else {
            return;
        };
        let all = db.get_folders_with_metadata().unwrap_or_default();
        for id in ids {
            let Some(folder) = all.iter().find(|f| f.id == id) else {
                continue;
            };
            if let Err(e) = db.remove_folder_with_tracks(&folder.path) {
                log::error!("[qbz-qt] remove folder failed: {e}");
                set_status(qbz_i18n::t("Could not remove that folder."));
            }
        }
    })
    .await;
    refresh_browse();
}

/// Flip a folder's enabled flag (a disabled folder is skipped by every scan).
pub async fn toggle_folder_enabled(id: i64) {
    let _ = tokio::task::spawn_blocking(move || {
        let Some(db) = open_db() else {
            return;
        };
        let current = db
            .get_folder_by_id(id)
            .ok()
            .flatten()
            .map(|f| f.enabled)
            .unwrap_or(true);
        if let Err(e) = db.set_folder_enabled(id, !current) {
            log::error!("[qbz-qt] set folder enabled failed: {e}");
        }
    })
    .await;
    refresh_browse();
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

/// Scan every enabled folder (`None`) or exactly one (`Some(id)`).
///
/// Progress rides the statics; a 2 s ticker republishes the settings document
/// while the scan runs (the document is the only transport this port has, and
/// republishing per FILE would rebuild the whole snapshot thousands of times).
pub fn scan(folder_id: Option<i64>) {
    if SCANNING.swap(true, Ordering::SeqCst) {
        return;
    }
    CANCEL.store(false, Ordering::SeqCst);
    PROCESSED.store(0, Ordering::SeqCst);
    TOTAL.store(0, Ordering::SeqCst);
    *CURRENT_FILE.lock().unwrap_or_else(|e| e.into_inner()) = String::new();

    crate::spawn(async move {
        // Progress ticker.
        crate::spawn(async {
            while SCANNING.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
                super::publish_snapshot().await;
            }
        });

        let _ = tokio::task::spawn_blocking(move || {
            let Some(db) = open_db() else {
                return;
            };
            let cache = qbz_library::get_artwork_cache_dir();
            let ids = folder_id.map(|id| vec![id]);
            let on_event = move |event: ScanEvent| match event {
                ScanEvent::TotalsAdded { total } => {
                    TOTAL.store(total, Ordering::SeqCst);
                }
                ScanEvent::FileStarted { path } => {
                    let name = basename(&path);
                    *CURRENT_FILE.lock().unwrap_or_else(|e| e.into_inner()) = name;
                }
                ScanEvent::FileDone { processed, total } => {
                    PROCESSED.store(processed, Ordering::SeqCst);
                    TOTAL.store(total, Ordering::SeqCst);
                }
                ScanEvent::Finished { errors, .. } => {
                    if !errors.is_empty() {
                        log::warn!("[qbz-qt] library scan finished with {} errors", errors.len());
                    }
                }
                _ => {}
            };
            if let Err(e) = qbz_library::scan_with_progress(
                &db,
                ids.as_deref(),
                &cache,
                &CANCEL,
                &on_event,
            ) {
                log::error!("[qbz-qt] library scan failed: {e}");
                set_status(qbz_i18n::t("Scan failed."));
            }
        })
        .await;

        SCANNING.store(false, Ordering::SeqCst);
        *CURRENT_FILE.lock().unwrap_or_else(|e| e.into_inner()) = String::new();
        refresh_browse();
        super::publish_snapshot().await;
    });
}

/// "Stop" — checked at every file boundary by the shared engine.
pub fn stop_scan() {
    CANCEL.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Maintenance + danger zone
// ---------------------------------------------------------------------------

/// Remove tracks whose files no longer exist (Slint `cleanup_missing`, same
/// guard: a network folder whose mount is DOWN stats as missing for every
/// file, so its subtree is skipped instead of being wiped).
pub async fn cleanup_missing() {
    if CLEANING.swap(true, Ordering::SeqCst) {
        return;
    }
    *CLEANUP_STATUS.lock().unwrap_or_else(|e| e.into_inner()) =
        qbz_i18n::t("Scanning track paths...");
    super::publish_snapshot().await;

    let result = tokio::task::spawn_blocking(|| {
        let paths = crate::local_state::with_db(|db| db.get_all_track_paths())?;
        let skip: Vec<String> = crate::local_state::with_db(|db| db.get_folders_with_metadata())
            .unwrap_or_default()
            .into_iter()
            .filter(|f| f.is_network && std::fs::read_dir(&f.path).is_err())
            .map(|f| {
                if f.path.ends_with('/') {
                    f.path
                } else {
                    format!("{}/", f.path)
                }
            })
            .collect();
        let checked = paths.len();
        let missing: Vec<i64> = paths
            .into_iter()
            .filter(|(_, p)| {
                !skip.iter().any(|pre| p.starts_with(pre.as_str()))
                    && !std::path::Path::new(p).exists()
            })
            .map(|(id, _)| id)
            .collect();
        let mut removed = 0usize;
        for chunk in missing.chunks(500) {
            removed += crate::local_state::with_db(|db| db.delete_tracks_by_ids(chunk)).unwrap_or(0);
        }
        Some((checked, removed))
    })
    .await
    .ok()
    .flatten();

    *CLEANUP_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = match result {
        Some((checked, removed)) => qbz_i18n::t("Checked {} tracks, removed {}")
            .replacen("{}", &checked.to_string(), 1)
            .replacen("{}", &removed.to_string(), 1),
        None => qbz_i18n::t("Cleanup failed."),
    };
    CLEANING.store(false, Ordering::SeqCst);
    refresh_browse();
    super::publish_snapshot().await;
}

/// "Clear library database" — drops the indexed tracks; the audio files and
/// the registered folders stay (`clear_all_tracks`, Slint parity).
pub async fn clear_library() {
    if CLEARING.swap(true, Ordering::SeqCst) {
        return;
    }
    super::publish_snapshot().await;
    let _ = tokio::task::spawn_blocking(|| {
        if crate::local_state::with_db(|db| db.clear_all_tracks()).is_none() {
            set_status(qbz_i18n::t("Could not clear the library database."));
        }
    })
    .await;
    CLEARING.store(false, Ordering::SeqCst);
    refresh_browse();
    super::publish_snapshot().await;
}

// ---------------------------------------------------------------------------
// Plex (the rows the QbzLocal properties do not already carry)
// ---------------------------------------------------------------------------

pub fn set_metadata_write(value: bool) {
    crate::local_plex::set_metadata_write_enabled(value);
}

/// Plex danger zone > "Clear cache": drop the cached libraries and tracks but
/// KEEP the sign-in (`disconnect` is the one that clears credentials).
pub async fn plex_clear_cache() {
    let _ = tokio::task::spawn_blocking(|| {
        if let Err(e) = qbz_plex::plex_cache_clear() {
            log::error!("[qbz-qt] plex cache clear failed: {e}");
        }
    })
    .await;
    crate::local_bridge_ops::publish_plex_state();
    refresh_browse();
}

// ---------------------------------------------------------------------------
// Shared tail
// ---------------------------------------------------------------------------

/// Re-run the Local Library browse documents after any mutation — the grid,
/// the artists rail, the Tracks page and the tab badges are all queries.
fn refresh_browse() {
    crate::local_bridge_ops::reload_browse();
}
