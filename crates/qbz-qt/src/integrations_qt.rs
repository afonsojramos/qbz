//! Integrations settings controller (phase 19) — the Slint
//! `crates/qbz/src/scrobble.rs` + `discord_rpc.rs` + the settings.rs
//! integration rows, ported onto the SAME stores:
//! `scrobbler_settings.db` (`qbz_app::settings::scrobblers`, per-user),
//! `discover_prefs.db` (`qbz_app::settings::discover_prefs`, per-user) and
//! the shared `ui_prefs.json` (`musicbrainz_enabled`, `discord_rpc_enabled`).
//!
//! POC-NOTEs:
//! - No live scrobbling: `scrobble::start` / `on_track_changed` (the
//!   now-playing/scrobble firing + offline queues) is NOT ported — the
//!   toggles + credentials persist to the SAME DB the Slint scrobbler
//!   reads, so they take effect in the Slint app / a future port.
//! - Discord: the enable pref persists AND flips the live `DiscordRpc`
//!   flag, but presence updates (`DiscordRpc::update` on track change)
//!   are not wired (no rich-presence feed in the POC).
//! - ListenBrainz disconnect does NOT clear the shared
//!   `cache/listenbrainz_v2.db` credentials row (scrobble.rs:334-342 —
//!   that cache feeds the scrobbler runtime, which the POC does not run).

use std::sync::{Arc, Mutex, OnceLock};

use qbz_app::settings::discover_prefs::DiscoverPrefsStore;
use qbz_app::settings::scrobblers::{ScrobblerSettings, ScrobblerSettingsState};
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_integrations::discord::DiscordRpc;
use qbz_integrations::lastfm::LastFmClient;
use qbz_integrations::listenbrainz::ListenBrainzClient;

use crate::settings_qt::{pref_bool, save_pref};

// ---------------------------------------------------------------------------
// Stores (SAME files as the Slint app)
// ---------------------------------------------------------------------------

static SCROBBLE: OnceLock<ScrobblerSettingsState> = OnceLock::new();

fn scrobble() -> &'static ScrobblerSettingsState {
    SCROBBLE.get_or_init(|| {
        let state = ScrobblerSettingsState::new_empty();
        if let Some(dir) = crate::sidebar_qt::user_dir() {
            if let Err(e) = state.init_at(&dir) {
                log::warn!("[qbz-qt] scrobbler settings store unavailable: {e}");
            }
        }
        state
    })
}

static DISCOVER: OnceLock<Mutex<Option<DiscoverPrefsStore>>> = OnceLock::new();

fn with_discover<T>(f: impl FnOnce(&DiscoverPrefsStore) -> T) -> Option<T> {
    let cell = DISCOVER.get_or_init(|| {
        let store = crate::sidebar_qt::user_dir()
            .and_then(|dir| DiscoverPrefsStore::new_at(&dir).ok());
        if store.is_none() {
            log::warn!("[qbz-qt] discover prefs store unavailable");
        }
        Mutex::new(store)
    });
    let guard = cell.lock().ok()?;
    guard.as_ref().map(f)
}

static DISCORD: OnceLock<DiscordRpc> = OnceLock::new();

fn discord() -> &'static DiscordRpc {
    DISCORD.get_or_init(DiscordRpc::new)
}

// ---------------------------------------------------------------------------
// Transient UI state (ScrobbleState busy/auth-url/status + the Last.fm
// pending request token — process memory only, like LASTFM_PENDING_TOKEN).
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct IntegrationUi {
    pub lastfm_busy: bool,
    pub listenbrainz_busy: bool,
    pub lastfm_auth_url: String,
    pub status_text: String,
    /// 0 none / 1 info / 2 ok / 3 error (ScrobbleState.status-kind).
    pub status_kind: i32,
    pending_lastfm_token: String,
}

static UI: OnceLock<Mutex<IntegrationUi>> = OnceLock::new();

fn ui_state() -> &'static Mutex<IntegrationUi> {
    UI.get_or_init(|| Mutex::new(IntegrationUi::default()))
}

pub fn ui_snapshot() -> IntegrationUi {
    let g = ui_state().lock().unwrap();
    IntegrationUi {
        lastfm_busy: g.lastfm_busy,
        listenbrainz_busy: g.listenbrainz_busy,
        lastfm_auth_url: g.lastfm_auth_url.clone(),
        status_text: g.status_text.clone(),
        status_kind: g.status_kind,
        pending_lastfm_token: String::new(),
    }
}

fn set_status(text: &str, kind: i32) {
    let mut g = ui_state().lock().unwrap();
    g.status_text = text.to_string();
    g.status_kind = kind;
}

// ---------------------------------------------------------------------------
// Snapshot fields (folded into SettingsDoc by settings_qt::publish_snapshot)
// ---------------------------------------------------------------------------

pub fn scrobble_settings() -> ScrobblerSettings {
    scrobble().get_settings().unwrap_or_default()
}

pub fn show_recommendations() -> bool {
    with_discover(|s| s.load().show_recommendations).unwrap_or(true)
}

pub fn discord_enabled() -> bool {
    pref_bool("discord_rpc_enabled", false)
}

// ---------------------------------------------------------------------------
// Toggle handlers (settings_qt.rs bool arms delegate here)
// ---------------------------------------------------------------------------

pub fn set_show_recommendations(value: bool) -> Result<(), String> {
    with_discover(|s| {
        let mut prefs = s.load();
        prefs.show_recommendations = value;
        s.save(&prefs)
    })
    .unwrap_or_else(|| Err("discover prefs store not open".to_string()))
}

pub fn set_scrobble_enabled(value: bool) -> Result<(), String> {
    scrobble().set_enabled(value)
}

pub fn set_scrobble_collapsed(value: bool) -> Result<(), String> {
    scrobble().set_ui_collapsed(value)
}

pub fn set_lastfm_enabled(value: bool) -> Result<(), String> {
    scrobble().set_lastfm_enabled(value)
}

pub fn set_listenbrainz_enabled(value: bool) -> Result<(), String> {
    scrobble().set_listenbrainz_enabled(value)
}

pub fn set_discord_enabled(value: bool) -> Result<(), String> {
    save_pref("discord_rpc_enabled", serde_json::json!(value));
    discord().set_enabled(value);
    log::info!("[qbz-qt] discord_rpc_enabled -> {value}");
    Ok(())
}

pub fn set_musicbrainz_enabled(value: bool) -> Result<(), String> {
    save_pref("musicbrainz_enabled", serde_json::json!(value));
    Ok(())
}

// ---------------------------------------------------------------------------
// Connection flows (ScrobbleActions)
// ---------------------------------------------------------------------------

/// ListenBrainz paste-token flow (scrobble.rs:315): validate against
/// /validate-token, persist token + username, force-enable on success.
pub async fn listenbrainz_set_token(token: &str) {
    let token = token.trim().to_string();
    if token.is_empty() {
        return;
    }
    ui_state().lock().unwrap().listenbrainz_busy = true;
    crate::settings_qt::publish_snapshot().await;

    let client = ListenBrainzClient::new();
    match client.set_token(&token).await {
        Ok(info) => {
            if let Err(e) = scrobble().set_listenbrainz_token(&token, &info.user_name) {
                log::error!("[qbz-qt] persist listenbrainz token failed: {e}");
            }
            // First-connect force-enable (scrobble.rs).
            let _ = scrobble().set_listenbrainz_enabled(true);
            set_status(&format!("Signed in as {}.", info.user_name), 2);
        }
        Err(e) => set_status(&format!("ListenBrainz: {e}"), 3),
    }
    ui_state().lock().unwrap().listenbrainz_busy = false;
    crate::settings_qt::publish_snapshot().await;
}

/// integrations_action dispatch (non-toggle rows).
pub async fn handle_action(_runtime: &Arc<AppRuntime<LoggingAdapter>>, action: &str) {
    match action {
        // Last.fm two-step browser auth (scrobble.rs:190): request token ->
        // authorize URL in the browser -> Finish exchanges for a session.
        "lastfm-connect" => {
            ui_state().lock().unwrap().lastfm_busy = true;
            crate::settings_qt::publish_snapshot().await;
            let client = LastFmClient::new();
            match client.get_token().await {
                Ok((token, url)) => {
                    {
                        let mut g = ui_state().lock().unwrap();
                        g.pending_lastfm_token = token;
                        g.lastfm_auth_url = url.clone();
                    }
                    set_status("Authorize QBZ in your browser, then click Finish.", 1);
                    if let Err(e) = open::that(&url) {
                        log::warn!("[qbz-qt] open Last.fm authorize page failed: {e}");
                    }
                }
                Err(e) => set_status(&format!("Last.fm: {e}"), 3),
            }
            ui_state().lock().unwrap().lastfm_busy = false;
            crate::settings_qt::publish_snapshot().await;
        }
        "lastfm-open-auth-url" => {
            let url = ui_state().lock().unwrap().lastfm_auth_url.clone();
            if !url.is_empty() {
                if let Err(e) = open::that(&url) {
                    log::warn!("[qbz-qt] open Last.fm authorize page failed: {e}");
                }
            }
        }
        "lastfm-finish" => {
            let token = ui_state().lock().unwrap().pending_lastfm_token.clone();
            if token.is_empty() {
                return;
            }
            ui_state().lock().unwrap().lastfm_busy = true;
            crate::settings_qt::publish_snapshot().await;
            let mut client = LastFmClient::new();
            match client.get_session(&token).await {
                Ok(session) => {
                    if let Err(e) = scrobble().set_lastfm_session(&session.key, &session.name) {
                        log::error!("[qbz-qt] persist lastfm session failed: {e}");
                    }
                    // First-connect force-enable (scrobble.rs).
                    let _ = scrobble().set_lastfm_enabled(true);
                    {
                        let mut g = ui_state().lock().unwrap();
                        g.pending_lastfm_token.clear();
                        g.lastfm_auth_url.clear();
                    }
                    set_status(&format!("Signed in as {}.", session.name), 2);
                }
                Err(e) => set_status(&format!("Last.fm: {e}"), 3),
            }
            ui_state().lock().unwrap().lastfm_busy = false;
            crate::settings_qt::publish_snapshot().await;
        }
        "lastfm-disconnect" => {
            if let Err(e) = scrobble().disconnect_lastfm() {
                log::error!("[qbz-qt] lastfm disconnect failed: {e}");
            }
            set_status("", 0);
            crate::settings_qt::publish_snapshot().await;
        }
        "listenbrainz-disconnect" => {
            if let Err(e) = scrobble().disconnect_listenbrainz() {
                log::error!("[qbz-qt] listenbrainz disconnect failed: {e}");
            }
            set_status("", 0);
            crate::settings_qt::publish_snapshot().await;
        }
        other => log::warn!("[qbz-qt] unknown integrations action: {other}"),
    }
}
