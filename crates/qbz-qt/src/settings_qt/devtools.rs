//! Settings > Developer, Settings > Blacklist and the Flatpak/Snap section.
//!
//! ADR-006 again: the log file is `qbz_log::install::log_file_path()` (the
//! sink `main.rs` installs), the export bundle is the shared
//! `qbz_app::settings::bundle` engine (the same one `qbzd` uses), and the
//! blacklist counters come from the per-user `artist_blacklist` singleton —
//! the same store the Blacklist Manager view mutates, so the row cannot go
//! stale behind it.
//!
//! Deltas vs the Slint (reported, not hidden):
//! - Developer > "Connect diagnostics" is not shipped: the port has no live
//!   Qobuz Connect service, so `QconnectDevState` has nothing to show.
//! - Developer > logs opens the log FILE (the port has no log-viewer overlay
//!   with copy/upload); same for the sub-nav's "Share logs" entry.
//! - Developer > export writes the bundle straight to disk with auth
//!   EXCLUDED. The Slint's `SettingsExportModal` (the include-auth gate,
//!   default OFF) is not ported, so the safe default is the only behaviour.

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

/// Blacklist counters, read-only, straight off the per-user singleton bound at
/// session activation (`auth_qt::bind_per_user_stores`).
///
/// This used to open a throwaway `BlacklistService` on every settings publish.
/// It no longer can: the manager view mutates the SAME store, so a second
/// handle would serve stale counts the moment the user removed an artist.
///
/// Fail-open is unchanged. `artist_blacklist`'s accessors return
/// `(true, 0, 0)` for exactly the three cases the deleted early-returns
/// covered — no session bound, no store file, store open failed — so the
/// no-session and fresh-account behaviours do not move. The counts
/// deliberately ignore the enabled flag: the row reports what is stored, and
/// the flag is rendered separately.
fn blacklist_counts() -> (bool, i32, i32) {
    crate::blacklist_qt::counts()
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

/// Developer > Export settings… — writes `qbz-settings-YYYYMMDD.qbzb` (0600),
/// then reports the path inline.
///
/// `include_auth` is the reference's single `--include-auth` gate
/// (`SettingsExportModal.slint`, one checkbox, default OFF, read in
/// `crates/qbz/src/settings.rs:1412-1425`). It used to be hard-coded `false`
/// here, so the Qt build could never produce the bundle the CLI's
/// `--include-auth` describes — the row said "portable bundle of your
/// settings" and quietly shipped one that could not sign you in.
pub async fn export_settings(include_auth: bool) {
    let text = tokio::task::spawn_blocking(move || {
        let bundle = match bundle::export(ExportSource::Desktop, &ExportOptions { include_auth }) {
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
