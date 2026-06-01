//! Persistence layer for QConnect startup mode + last-known state.
//!
//! Mirrors the device_name persistence pattern in `transport.rs`.
//! Reuses the existing `qconnect_settings.db` (key/value table).
//!
//! All operations are fail-open: any I/O or SQLite error returns the
//! default (Off / None) rather than propagating, so a corrupt DB never
//! prevents the app from starting.

use qconnect_app::QconnectStartupMode;

/// Path to the QConnect settings database (global, not per-user).
/// Same path used by `transport.rs::qconnect_settings_db_path`.
fn db_path() -> Option<std::path::PathBuf> {
    let data_dir = dirs::data_dir()?.join("qbz");
    std::fs::create_dir_all(&data_dir).ok()?;
    Some(data_dir.join("qconnect_settings.db"))
}

fn open_settings_conn() -> Option<rusqlite::Connection> {
    let conn = rusqlite::Connection::open(db_path()?).ok()?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .ok()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    )
    .ok()?;
    Some(conn)
}

/// Load the persisted startup mode. Returns `Off` (default) when missing or invalid.
pub fn load_startup_mode() -> QconnectStartupMode {
    let Some(conn) = open_settings_conn() else {
        return QconnectStartupMode::default();
    };
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'startup_mode'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();
    value
        .as_deref()
        .and_then(QconnectStartupMode::from_str)
        .unwrap_or_default()
}

/// Persist the startup mode.
pub fn save_startup_mode(mode: QconnectStartupMode) {
    let Some(conn) = open_settings_conn() else {
        return;
    };
    let _ = conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('startup_mode', ?1)",
        rusqlite::params![mode.as_str()],
    );
}

/// Load the last-known QConnect on/off state, if recorded.
pub fn load_last_known_state() -> Option<bool> {
    let conn = open_settings_conn()?;
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'last_known_state'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();
    match value.as_deref() {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    }
}

/// Persist the last-known on/off state. Called from the V2 connect/disconnect
/// commands when `startup_mode == RememberLast`.
pub fn save_last_known_state(state: bool) {
    let Some(conn) = open_settings_conn() else {
        return;
    };
    let value = if state { "on" } else { "off" };
    let _ = conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('last_known_state', ?1)",
        rusqlite::params![value],
    );
}

/// Tauri-managed wrapper for the volatile CLI override
/// (`--enable-qconnect` / `--disable-qconnect`).
///
/// Set once in `pub fn run` and read from inside the bootstrap command,
/// because the auto-connect dispatch must wait until OAuth restore + session
/// activation are complete.
pub struct QconnectCliOverride(pub Option<bool>);

/// Bounded, monotonically-increasing backoff schedule for the startup
/// auto-connect retry loop (gap #8). A failed initial connect previously left
/// the lifecycle Off forever; now we retry on this fixed schedule and give up
/// for the session after the last step. Pure so it can be unit-tested headless.
pub fn startup_retry_schedule() -> [u64; 4] {
    [2_000, 5_000, 15_000, 30_000]
}

/// Trigger QConnect auto-connect AFTER the runtime is fully bootstrapped
/// (client init + OAuth restore + CoreBridge auth + session activation).
///
/// Called from inside `v2_runtime_bootstrap` after the OAuth success path
/// completes. If startup_mode + last_known_state + cli_override resolve to
/// "should not connect", returns early. Otherwise spawns a fire-and-forget
/// task that mirrors the `v2_qconnect_connect` connect path.
///
/// On failure the lifecycle stays Off; the existing reconnect loop only
/// fires for established sessions, not for failed initial connects.
pub async fn maybe_auto_connect_after_bootstrap(
    app: &tauri::AppHandle,
    cli_override: Option<bool>,
) {
    use qconnect_app::compute_effective_startup;
    use tauri::Manager;

    let mode = load_startup_mode();
    let last = load_last_known_state();
    let should_connect = compute_effective_startup(mode, cli_override, last);

    log::info!(
        "[QConnect] post-bootstrap startup decision: mode={} cli_override={:?} last_known={:?} -> {}",
        mode.as_str(),
        cli_override,
        last,
        should_connect
    );

    if !should_connect {
        return;
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let service = app_handle.state::<crate::qconnect::QconnectServiceState>();
        let app_state = app_handle.state::<crate::AppState>();
        let core_bridge = app_handle.state::<crate::core_bridge::CoreBridgeState>();

        // gap #8: retry the initial connect on a bounded backoff schedule
        // instead of giving up after a single failed attempt. Each iteration
        // re-resolves the transport config (endpoint+JWT via qws/createToken)
        // because the failure may be a transient credential/network issue that
        // clears on a later attempt.
        let schedule = startup_retry_schedule();
        // One immediate attempt, then one per scheduled delay.
        for attempt in 0..=schedule.len() {
            // Use default options; auto-discovery via qws/createToken resolves
            // endpoint+JWT (see transport.rs::resolve_transport_config).
            let resolved = crate::qconnect::transport::resolve_transport_config(
                Default::default(),
                &app_state,
            )
            .await;

            match resolved {
                Ok(config) => {
                    match service
                        .connect(app_handle.clone(), core_bridge.0.clone(), config)
                        .await
                    {
                        Ok(_) => {
                            log::info!("[QConnect] startup auto-connect succeeded");
                            return;
                        }
                        Err(e) => {
                            log::warn!(
                                "[QConnect] startup auto-connect attempt {} failed: {e}",
                                attempt + 1
                            );
                        }
                    }
                }
                Err(e) => {
                    log::warn!(
                        "[QConnect] startup auto-connect attempt {} transport resolve failed: {e}",
                        attempt + 1
                    );
                }
            }

            // If there is a remaining scheduled delay, wait it out before the
            // next attempt; otherwise we have exhausted the schedule.
            match schedule.get(attempt) {
                Some(delay_ms) => {
                    tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)).await;
                }
                None => {
                    log::warn!(
                        "[QConnect] startup auto-connect gave up for this session after {} attempts",
                        attempt + 1
                    );
                    return;
                }
            }
        }
    });
}
