//! Settings > Import / Export — everything that moves data in or out of
//! this QBZ install, in one place:
//!
//! - the app settings bundle (`.qbzb`), formerly under Developer;
//! - the portable blacklist (a JSON one user can hand to another);
//! - account migration (snapshot of a Qobuz account's favorites and
//!   playlists, and the additive migration into another account).
//!
//! Each block writes its own status line; the panel renders them from the
//! settings document (`importExport`). Files are written to the Downloads
//! folder (home as the fallback), like the settings bundle always was.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use qbz_account_migration as migration;
use qbz_account_migration::{MigrationEvent, MigrationPhase};
use qbz_app::settings::blacklist_portable;
use qbz_app::settings::bundle::{self, ExportOptions, ExportSource};
use qbz_app::user_data::UserDataPaths;
use serde::Serialize;

static SETTINGS_STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
static BLACKLIST_STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
static SNAPSHOT_STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
static MIGRATION_STATUS: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::new()));
/// One snapshot or migration at a time; the buttons disable on it.
static MIGRATION_BUSY: AtomicBool = AtomicBool::new(false);
/// Progress events since the last republish (throttle).
static PROGRESS_TICKS: AtomicUsize = AtomicUsize::new(0);

fn set_settings_status(text: String) {
    *SETTINGS_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

fn set_blacklist_status(text: String) {
    *BLACKLIST_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

fn set_snapshot_status(text: String) {
    *SNAPSHOT_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

fn set_migration_status(text: String) {
    *MIGRATION_STATUS.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

fn read(status: &Mutex<String>) -> String {
    status.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// One snapshot file found on this machine, as the panel lists it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRow {
    pub user_id: u64,
    pub path: String,
    /// "Account 10385965 · 2026-09-02 01:20"
    pub label: String,
    pub favorites: usize,
    pub playlists: usize,
    pub subscriptions: usize,
    /// Taken from the account that is signed in right now: can be kept as
    /// a backup, cannot be migrated into itself.
    pub is_current_account: bool,
}

/// The `importExport` block of the settings document.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings_status: String,
    pub blacklist_status: String,
    pub blacklist_artists: i32,
    pub blacklist_albums: i32,
    pub snapshot_status: String,
    pub migration_status: String,
    pub migration_busy: bool,
    pub snapshots: Vec<SnapshotRow>,
}

pub fn snapshot() -> Snapshot {
    let (_, blacklist_artists, blacklist_albums) = crate::blacklist_qt::counts();
    Snapshot {
        settings_status: read(&SETTINGS_STATUS),
        blacklist_status: read(&BLACKLIST_STATUS),
        blacklist_artists,
        blacklist_albums,
        snapshot_status: read(&SNAPSHOT_STATUS),
        migration_status: read(&MIGRATION_STATUS),
        migration_busy: MIGRATION_BUSY.load(Ordering::Relaxed),
        snapshots: snapshot_rows(),
    }
}

fn snapshot_rows() -> Vec<SnapshotRow> {
    let Ok(root) = UserDataPaths::global_data_dir() else {
        return Vec::new();
    };
    let current = crate::app().active_user_id().unwrap_or(0);
    migration::snapshot::list_snapshots(&root)
        .into_iter()
        .filter_map(|(user_id, path)| {
            let snap = migration::AccountSnapshot::read(&path).ok()?;
            let when = snap.created_at_local();
            let who = if snap.source.display_name.is_empty() {
                user_id.to_string()
            } else {
                format!("{} ({})", snap.source.display_name, user_id)
            };
            Some(SnapshotRow {
                user_id,
                path: path.to_string_lossy().into_owned(),
                label: qbz_i18n::t_args("Account {} · {}", &[&who, &when]),
                favorites: snap.favorites.total(),
                playlists: snap.playlists.len(),
                subscriptions: snap.subscriptions.len(),
                is_current_account: user_id == current,
            })
        })
        .collect()
}

/// Where exports land: Downloads, else home.
fn export_dir() -> Option<std::path::PathBuf> {
    dirs::download_dir().or_else(dirs::home_dir)
}

/// "Export settings…" — writes `qbz-settings-YYYYMMDD.qbzb` (0600), then
/// reports the path inline.
///
/// `include_auth` is the reference's single `--include-auth` gate
/// (`SettingsExportModal.slint`, one checkbox, default OFF, read in
/// `crates/qbz/src/settings.rs:1412-1425`). It used to be hard-coded `false`
/// here, so the Qt build could never produce the bundle the CLI's
/// `--include-auth` describes.
pub async fn export_settings(include_auth: bool) {
    let text = tokio::task::spawn_blocking(move || {
        let bundle = match bundle::export(ExportSource::Desktop, &ExportOptions { include_auth }) {
            Ok(b) => b,
            Err(e) => {
                log::error!("[qbz-qt] settings export failed: {e:?}");
                return qbz_i18n::t("Export failed.");
            }
        };
        let Some(dir) = export_dir() else {
            return qbz_i18n::t("Export failed.");
        };
        let path = dir.join(bundle::default_filename());
        match bundle::write_bundle_file(&path, &bundle) {
            Ok(()) => qbz_i18n::t("Saved to {}").replacen("{}", &path.to_string_lossy(), 1),
            Err(e) => {
                log::error!("[qbz-qt] settings bundle write failed: {e:?}");
                qbz_i18n::t("Export failed.")
            }
        }
    })
    .await
    .unwrap_or_else(|_| qbz_i18n::t("Export failed."));
    set_settings_status(text);
}

/// "Export blacklist…" — the blocked artists and albums as
/// `qbz-blacklist-YYYYMMDD.json` in Downloads.
pub async fn export_blacklist() {
    let text = tokio::task::spawn_blocking(|| {
        let bundle = match crate::artist_blacklist::export_portable() {
            Ok(b) => b,
            Err(e) => {
                log::error!("[qbz-qt] blacklist export failed: {e}");
                return qbz_i18n::t("Export failed.");
            }
        };
        let Some(dir) = export_dir() else {
            return qbz_i18n::t("Export failed.");
        };
        let path = dir.join(blacklist_portable::default_filename());
        let json = match serde_json::to_string_pretty(&bundle) {
            Ok(j) => j,
            Err(e) => {
                log::error!("[qbz-qt] blacklist serialize failed: {e}");
                return qbz_i18n::t("Export failed.");
            }
        };
        match std::fs::write(&path, json) {
            Ok(()) => qbz_i18n::t("Saved to {}").replacen("{}", &path.to_string_lossy(), 1),
            Err(e) => {
                log::error!("[qbz-qt] blacklist write failed: {e}");
                qbz_i18n::t("Export failed.")
            }
        }
    })
    .await
    .unwrap_or_else(|_| qbz_i18n::t("Export failed."));
    set_blacklist_status(text);
}

/// "Import blacklist…" — pick a file, merge it additively, report what
/// changed, and republish the manager so an open Blacklist view refreshes.
/// Cancelling the dialog is a no-op.
pub async fn import_blacklist() {
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(&qbz_i18n::t("Choose a blacklist file"))
        .add_filter("JSON", &["json"])
        .pick_file()
        .await
    else {
        return;
    };
    let path = file.path().to_path_buf();
    let text = tokio::task::spawn_blocking(move || {
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) => {
                log::error!("[qbz-qt] blacklist read failed: {e}");
                return qbz_i18n::t("Import failed.");
            }
        };
        let bundle = match blacklist_portable::parse(&text) {
            Ok(b) => b,
            Err(e) => {
                log::warn!("[qbz-qt] blacklist import refused: {e}");
                return qbz_i18n::t("Not a QBZ blacklist file.");
            }
        };
        match crate::artist_blacklist::import_portable(&bundle) {
            Ok(report) => qbz_i18n::t_args(
                "Imported {} artists and {} albums ({} already blocked).",
                &[
                    &report.artists_added.to_string(),
                    &report.albums_added.to_string(),
                    &report.existing().to_string(),
                ],
            ),
            Err(e) => {
                log::error!("[qbz-qt] blacklist import failed: {e}");
                qbz_i18n::t("Import failed.")
            }
        }
    })
    .await
    .unwrap_or_else(|_| qbz_i18n::t("Import failed."));
    set_blacklist_status(text);
    crate::blacklist_qt::publish();
}

// ======================= ACCOUNT MIGRATION ===========================

/// Progress sink: phase changes and every 10th item update the status
/// line and republish the document, so a 2 000-favorite run shows life
/// without republishing 2 000 times.
fn progress_sink(status: &'static Mutex<String>) -> impl Fn(MigrationEvent) + Send + Sync {
    move |event| {
        let (text, publish) = match event {
            MigrationEvent::Phase(phase) => (
                match phase {
                    MigrationPhase::ReadingSource => {
                        qbz_i18n::t("Reading favorites and playlists…")
                    }
                    MigrationPhase::ReadingTarget => qbz_i18n::t("Reading this account…"),
                    MigrationPhase::Favorites => qbz_i18n::t("Adding favorites…"),
                    MigrationPhase::Playlists => qbz_i18n::t("Adding playlists…"),
                    MigrationPhase::Subscriptions => qbz_i18n::t("Following playlists…"),
                    MigrationPhase::Done => String::new(),
                },
                true,
            ),
            MigrationEvent::Progress { done, total, label } => {
                let n = PROGRESS_TICKS.fetch_add(1, Ordering::Relaxed);
                (
                    qbz_i18n::t_args(
                        "{} / {} — {}",
                        &[&(done + 1).to_string(), &total.to_string(), &label],
                    ),
                    n % 10 == 0,
                )
            }
        };
        if !text.is_empty() {
            *status.lock().unwrap_or_else(|e| e.into_inner()) = text;
        }
        if publish {
            crate::spawn(async { super::publish_snapshot().await });
        }
    }
}

/// The signed-in client and session, or the status text to show instead.
async fn signed_in_client() -> Result<(qbz_qobuz::QobuzClient, qbz_models::UserSession), String> {
    let runtime = crate::app();
    let client = {
        let lock = runtime.core().client();
        let guard = lock.read().await;
        guard.as_ref().cloned()
    };
    let Some(client) = client else {
        return Err(qbz_i18n::t("Not logged in to Qobuz"));
    };
    let Some(session) = client.session().await else {
        return Err(qbz_i18n::t("Not logged in to Qobuz"));
    };
    Ok((client, session))
}

/// "Create migration snapshot": capture the signed-in account into its
/// own profile directory (`users/<uid>/account_migration/`).
pub async fn create_snapshot() {
    if MIGRATION_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    set_snapshot_status(qbz_i18n::t("Reading favorites and playlists…"));
    let result: Result<String, String> = async {
        let (client, session) = signed_in_client().await?;
        let profile_dir = UserDataPaths::data_dir_for(session.user_id)?;
        let sink = progress_sink(&SNAPSHOT_STATUS);
        let snap = migration::capture(&client, session.user_id, &session.display_name, &sink)
            .await
            .map_err(|e| {
                log::error!("[qbz-qt] account snapshot failed: {e}");
                qbz_i18n::t("Snapshot failed.")
            })?;
        let path = migration::snapshot::snapshot_path(&profile_dir);
        snap.write(&path).map_err(|e| {
            log::error!("[qbz-qt] account snapshot write failed: {e}");
            qbz_i18n::t("Snapshot failed.")
        })?;
        Ok(qbz_i18n::t_args(
            "Saved {} favorites, {} playlists and {} followed playlists to {}",
            &[
                &snap.favorites.total().to_string(),
                &snap.playlists.len().to_string(),
                &snap.subscriptions.len().to_string(),
                &path.to_string_lossy(),
            ],
        ))
    }
    .await;
    set_snapshot_status(match result {
        Ok(text) | Err(text) => text,
    });
    MIGRATION_BUSY.store(false, Ordering::SeqCst);
}

/// The panel's "Migrate…" payload: the snapshot path plus the local-copy
/// options. A bare path (older panel) means everything on.
#[derive(Debug, serde::Deserialize)]
struct MigrateRequest {
    path: String,
    #[serde(default = "yes")]
    media_servers: bool,
    #[serde(default = "yes")]
    scrobblers: bool,
    #[serde(default = "yes")]
    listening_history: bool,
    #[serde(default = "yes")]
    local_profile: bool,
}

fn yes() -> bool {
    true
}

impl MigrateRequest {
    fn parse(value: &str) -> Self {
        serde_json::from_str(value).unwrap_or_else(|_| Self {
            path: value.to_string(),
            media_servers: true,
            scrobblers: true,
            listening_history: true,
            local_profile: true,
        })
    }
}

/// "Migrate…" on a snapshot row: plan against this account's live state
/// and apply, additively; then copy the old profile's local data through
/// the ledger's playlist map. Re-warms the favorites cache and reloads the
/// sidebar; the local half needs a restart to be picked up by every store.
pub async fn migrate(value: String) {
    if MIGRATION_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let request = MigrateRequest::parse(&value);
    let path = request.path.clone();
    set_migration_status(qbz_i18n::t("Reading this account…"));
    let runtime = crate::app();
    let result: Result<String, String> = async {
        let source = migration::AccountSnapshot::read(std::path::Path::new(&path)).map_err(|e| {
            log::error!("[qbz-qt] snapshot unreadable: {e}");
            qbz_i18n::t("Migration failed.")
        })?;
        let (client, session) = signed_in_client().await?;
        if source.source.user_id == session.user_id {
            return Err(qbz_i18n::t(
                "That snapshot was taken from this same account.",
            ));
        }
        let profile_dir = UserDataPaths::data_dir_for(session.user_id)?;
        let sink = progress_sink(&MIGRATION_STATUS);
        sink(MigrationEvent::Phase(MigrationPhase::ReadingTarget));
        let target = migration::capture(&client, session.user_id, &session.display_name, &sink)
            .await
            .map_err(|e| {
                log::error!("[qbz-qt] target capture failed: {e}");
                qbz_i18n::t("Migration failed.")
            })?;
        let ledger = migration::Ledger::load(&profile_dir).map_err(|e| {
            log::error!("[qbz-qt] ledger unreadable: {e}");
            qbz_i18n::t("Migration failed.")
        })?;
        let plan = migration::plan(&source, &target, &ledger);
        if plan.is_empty() {
            return Ok(qbz_i18n::t(
                "Nothing to migrate: everything is already in this account.",
            ));
        }
        let report = migration::apply(&plan, &client, &profile_dir, source.source.user_id, &sink)
            .await
            .map_err(|e| {
                log::error!("[qbz-qt] migration apply failed: {e}");
                qbz_i18n::t("Migration failed.")
            })?;
        let mut text = qbz_i18n::t_args(
            "Added {} favorites, {} playlists ({} tracks) and {} followed playlists; {} were already there.",
            &[
                &report.favorites.added.to_string(),
                &report.playlists.added.to_string(),
                &report.tracks_added.to_string(),
                &report.subscriptions.added.to_string(),
                &(report.favorites.already + report.playlists.already + report.subscriptions.already)
                    .to_string(),
            ],
        );
        if report.failed() > 0 {
            for line in report
                .favorites
                .failed
                .iter()
                .chain(&report.playlists.failed)
                .chain(&report.subscriptions.failed)
            {
                log::warn!("[qbz-qt] migration item failed: {line}");
            }
            text.push(' ');
            text.push_str(&qbz_i18n::t_args(
                "{} items failed; see the log.",
                &[&report.failed().to_string()],
            ));
        }
        // --- Local profile (old users/<uid>/ → this one) -----------------
        if request.local_profile {
            let src_dir = UserDataPaths::data_dir_for(source.source.user_id)?;
            if src_dir.is_dir() {
                let ledger = migration::Ledger::load(&profile_dir).map_err(|e| {
                    log::error!("[qbz-qt] ledger unreadable after apply: {e}");
                    qbz_i18n::t("Migration failed.")
                })?;
                let options = migration::LocalOptions {
                    media_servers: request.media_servers,
                    scrobblers: request.scrobblers,
                    listening_history: request.listening_history,
                };
                let dst_dir = profile_dir.clone();
                let local = tokio::task::spawn_blocking(move || {
                    migration::copy_profile(&src_dir, &dst_dir, &ledger, options)
                })
                .await
                .map_err(|e| e.to_string())?;
                match local {
                    Ok(local) => {
                        for note in &local.notes {
                            log::info!("[qbz-qt] local profile copy: {note}");
                        }
                        text.push(' ');
                        text.push_str(&qbz_i18n::t_args(
                            "Local profile: {} rows copied; {} playlist rows had no match.",
                            &[
                                &local.total_rows().to_string(),
                                &local.unmapped_playlist_rows.to_string(),
                            ],
                        ));
                        if local.needs_rescan {
                            text.push(' ');
                            text.push_str(&qbz_i18n::t(
                                "The library already had folders: run a rescan to index the added ones.",
                            ));
                        }
                        text.push(' ');
                        text.push_str(&qbz_i18n::t(
                            "Restart QBZ to see the migrated local profile.",
                        ));
                    }
                    Err(e) => {
                        log::error!("[qbz-qt] local profile copy failed: {e}");
                        text.push(' ');
                        text.push_str(&qbz_i18n::t("The local profile could not be copied; see the log."));
                    }
                }
            }
        }
        Ok(text)
    }
    .await;
    let ok = result.is_ok();
    set_migration_status(match result {
        Ok(text) | Err(text) => text,
    });
    MIGRATION_BUSY.store(false, Ordering::SeqCst);
    if ok {
        crate::library_qt::warm_favorites_cache(&runtime).await;
        crate::sidebar_qt::load(&runtime).await;
    }
}
