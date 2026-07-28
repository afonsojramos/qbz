//! Settings > Developer, Settings > Blacklist and the Flatpak/Snap section.
//!
//! ADR-006 again: the log file is `qbz_log::install::log_file_path()` (the
//! sink `main.rs` installs), the export bundle is the shared
//! `qbz_app::settings::bundle` engine (the same one `qbzd` uses), and the
//! blacklist counters come from the shared `BlacklistService` store.
//!
//! Deltas vs the Slint (reported, not hidden):
//! - Developer > "Connect diagnostics" is not shipped: the port has no live
//!   Qobuz Connect service, so `QconnectDevState` has nothing to show.
//! - Developer > logs opens the log FILE (the port has no log-viewer overlay
//!   with copy/upload); same for the sub-nav's "Share logs" entry.
//! - Developer > export writes the bundle straight to disk with auth
//!   EXCLUDED. The Slint's `SettingsExportModal` (the include-auth gate,
//!   default OFF) is not ported, so the safe default is the only behaviour.
//! - Blacklist > "Manage" is not shipped: there is no Blacklist Manager view
//!   in this port, so the chevron would open nothing. The status line is the
//!   real store's counters.

use qbz_app::settings::bundle::{self, ExportOptions, ExportSource};
use serde::Serialize;
use std::sync::{LazyLock, Mutex};

static STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));

#[derive(Clone, Default, Serialize)]
pub struct Snapshot {
    /// Absolute path of the current run's log file ("" when unavailable).
    #[serde(rename = "logPath")]
    pub log_path: String,
    /// Result line of the last developer action (export…).
    pub status: String,
    #[serde(rename = "blacklistEnabled")]
    pub blacklist_enabled: bool,
    #[serde(rename = "blacklistArtists")]
    pub blacklist_artists: i32,
    #[serde(rename = "blacklistAlbums")]
    pub blacklist_albums: i32,
    /// "flatpak" | "snap" | "" — gates the sandbox sub-nav entry.
    #[serde(rename = "installMethod")]
    pub install_method: String,
}

/// Sandboxed-install detection (the Slint's `SandboxState.install-method`):
/// Flatpak exposes `/.flatpak-info`, Snap exports `$SNAP`.
fn install_method() -> String {
    if std::path::Path::new("/.flatpak-info").exists() {
        return "flatpak".to_string();
    }
    if std::env::var_os("SNAP").is_some() {
        return "snap".to_string();
    }
    String::new()
}

/// Blacklist counters, read-only. The store file is NOT created when it does
/// not exist yet — an empty blacklist reads as zeros.
fn blacklist_counts() -> (bool, i32, i32) {
    let Some(path) = crate::sidebar_qt::user_dir()
        .map(|d| d.join(qbz_app::settings::artist_blacklist::DB_FILE_NAME))
    else {
        return (true, 0, 0);
    };
    if !path.exists() {
        return (true, 0, 0);
    }
    match qbz_app::settings::artist_blacklist::BlacklistService::new(&path) {
        Ok(service) => (
            service.is_enabled(),
            service.count() as i32,
            service.album_count() as i32,
        ),
        Err(e) => {
            log::warn!("[qbz-qt] blacklist store open failed: {e}");
            (true, 0, 0)
        }
    }
}

pub fn snapshot() -> Snapshot {
    let (blacklist_enabled, blacklist_artists, blacklist_albums) = blacklist_counts();
    Snapshot {
        log_path: qbz_log::install::log_file_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        status: STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        blacklist_enabled,
        blacklist_artists,
        blacklist_albums,
        install_method: install_method(),
    }
}

/// Developer > Application logs (and the sub-nav "Share logs"): reveal the
/// on-disk log so it can be attached to an issue.
pub fn open_log_file() {
    let Some(path) = qbz_log::install::log_file_path() else {
        return;
    };
    if let Err(e) = open::that(&path) {
        log::warn!("[qbz-qt] could not open the log file: {e}");
        *STATUS.lock().unwrap_or_else(|e| e.into_inner()) =
            qbz_i18n::t("Could not open the log file.");
    }
}

/// Developer > Export settings… — writes `qbz-settings-YYYYMMDD.qbzb` (0600)
/// with auth EXCLUDED, then reports the path inline.
pub async fn export_settings() {
    let text = tokio::task::spawn_blocking(|| {
        let bundle = match bundle::export(
            ExportSource::Desktop,
            &ExportOptions {
                include_auth: false,
            },
        ) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[qbz-qt] settings export failed: {e:?}");
                return qbz_i18n::t("Export failed.");
            }
        };
        let Some(dir) = dirs::download_dir().or_else(dirs::home_dir) else {
            return qbz_i18n::t("Export failed.");
        };
        let path = dir.join(bundle::default_filename());
        match bundle::write_bundle_file(&path, &bundle) {
            Ok(()) => {
                qbz_i18n::t("Saved to {}").replacen("{}", &path.to_string_lossy(), 1)
            }
            Err(e) => {
                log::error!("[qbz-qt] settings bundle write failed: {e:?}");
                qbz_i18n::t("Export failed.")
            }
        }
    })
    .await
    .unwrap_or_else(|_| qbz_i18n::t("Export failed."));
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text;
}
