//! Integrations settings + runtime controller — the Slint
//! `crates/qbz/src/scrobble.rs` + `discord_rpc.rs` + the settings.rs
//! integration rows, ported onto the SAME stores:
//! `scrobbler_settings.db` (`qbz_app::settings::scrobblers`, per-user),
//! `discover_prefs.db` (`qbz_app::settings::discover_prefs`, per-user), the
//! shared per-user `offline_settings.db` `scrobble_queue`, the shared
//! `cache/listenbrainz_v2.db` (credentials + `listen_queue`) and the shared
//! `ui_prefs.json` (`musicbrainz_enabled`, `discord_rpc_enabled`).
//!
//! Everything here is STRICTLY OPT-IN (the project rule): no client is
//! constructed and no request leaves the process until the user connected the
//! service AND its enable flags are on. `ScrobblerSettings::lastfm_active()` /
//! `listenbrainz_active()` (master + per-service + authed) gate every fire,
//! and `DiscordRpc` only opens its IPC socket once `set_enabled(true)` came
//! from the persisted opt-in or the toggle.
//!
//! Inventory (1:1 with `settings/IntegrationsSettings.slint`):
//!   - Recommendations (discover_prefs `show_recommendations`)
//!   - MusicBrainz (`ui_prefs.musicbrainz_enabled` -> `core.musicbrainz_set_enabled`)
//!   - Scrobblers master (+ collapse) -> Last.fm, ListenBrainz
//!   - Discord Rich Presence (`ui_prefs.discord_rpc_enabled`)
//! (Discogs is NOT a settings integration in either reference — it is a tag
//! editor remote-metadata provider + an album external link.)
//!
//! Runtime wiring (the fire path) needs two call sites OUTSIDE this file:
//! [`start`] at shell entry and [`on_track_changed`] / [`discord_push`] on the
//! playback track-change + play/pause edges. See the "GLUE" notes on those
//! functions.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use qbz_app::offline_mode::OfflineModeStore;
use qbz_app::scrobble_timing::scrobble_delay_secs;
use qbz_app::settings::discover_prefs::DiscoverPrefsStore;
use qbz_app::settings::scrobblers::{ScrobblerSettings, ScrobblerSettingsState};
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_integrations::discord::DiscordRpc;
use qbz_integrations::lastfm::LastFmClient;
use qbz_integrations::listenbrainz::cache::ListenBrainzCache;
use qbz_integrations::listenbrainz::{AdditionalInfo, ListenBrainzClient};
use qbz_integrations::NowListening;
use qbz_models::QueueTrack;

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
        let store =
            crate::sidebar_qt::user_dir().and_then(|dir| DiscoverPrefsStore::new_at(&dir).ok());
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

/// `<user_dir>/cache/listenbrainz_v2.db` — the SAME per-user file the Slint
/// and Tauri builds open, so credentials AND the offline listen queue are
/// shared across frontends (scrobble.rs `listenbrainz_cache_path`).
fn listenbrainz_cache_path() -> Option<PathBuf> {
    let dir = crate::sidebar_qt::user_dir()?.join("cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("listenbrainz_v2.db"))
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

fn set_status(text: String, kind: i32) {
    let mut g = ui_state().lock().unwrap();
    g.status_text = text;
    g.status_kind = kind;
}

// ---------------------------------------------------------------------------
// Snapshot fields (folded into SettingsDoc by settings_qt::publish_snapshot)
// ---------------------------------------------------------------------------

pub fn scrobble_settings() -> ScrobblerSettings {
    scrobble().get_settings().unwrap_or_default()
}

/// The playlist importer's PREFILL + optional credential (2.0.3 expansion).
///
/// Returns `(lastfm_username, listenbrainz_username, listenbrainz_token)`.
///
/// STRICTLY A CONVENIENCE. Both service sources read PUBLIC data with a bare
/// username, so nothing here is required — a user who has connected nothing
/// types a handle and it works. The token is the one credential that changes
/// behaviour when present (higher ListenBrainz rate limits, and the user's own
/// private playlists resolve), and it is sent only because it is already
/// stored; the importer never asks for it.
pub fn scrobbler_handles() -> (String, String, Option<String>) {
    let cfg = scrobble_settings();
    let token = Some(cfg.listenbrainz_token.clone()).filter(|t| !t.trim().is_empty());
    (cfg.lastfm_username, cfg.listenbrainz_username, token)
}

pub fn show_recommendations() -> bool {
    with_discover(|s| s.load().show_recommendations).unwrap_or(true)
}

pub fn discord_enabled() -> bool {
    pref_bool("discord_rpc_enabled", false)
}

// ---------------------------------------------------------------------------
// Shell entry (scrobble::start + discord_rpc::init + the MusicBrainz seed)
// ---------------------------------------------------------------------------

/// One-shot guard for the offline-engine flush watcher (lives for the process).
static FLUSH_WATCHER: OnceLock<()> = OnceLock::new();

/// Per-user runtime start. GLUE: call from `enter_shell` (main.rs), AFTER the
/// per-user stores bind (`sidebar_qt::init_pinned` sets the user dir).
///
/// Applies the three persisted opt-ins that would otherwise only take effect
/// after the user re-toggled them:
///   - MusicBrainz -> `core.musicbrainz_set_enabled` (main.rs:9023 in Slint),
///   - Discord     -> `DiscordRpc::set_enabled` + an initial presence push
///                    (discord_rpc::init — the PR #477 "enabled on restart"
///                    fix: applied AFTER the session, never at early boot),
///   - ListenBrainz-> adopt the shared-cache credentials when this build has
///                    none (a Tauri/Slint sign-in carries over; enable flags
///                    are NOT touched, scrobbling stays opt-in).
/// Then drains the offline scrobble queues and watches for offline->online
/// edges to drain them again.
pub fn start(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    // Bind the per-user scrobbler store now (lazy OnceLock) so the fire path
    // never races the first Settings visit.
    let _ = scrobble().get_settings();

    let rt = runtime.clone();
    crate::spawn(async move {
        // MusicBrainz opt-out is a plain pref; the core caches its own flag.
        let mb_on = pref_bool("musicbrainz_enabled", true);
        rt.core().musicbrainz_set_enabled(mb_on).await;

        seed_listenbrainz_from_shared_cache().await;

        let discord_on = discord_enabled();
        discord().set_enabled(discord_on);
        if discord_on {
            discord_push(&rt);
        }

        if !crate::offline_fwd::engine().is_offline() {
            flush_offline_queues().await;
        }
    });

    FLUSH_WATCHER.get_or_init(|| {
        crate::spawn(async move {
            let mut rx = crate::offline_fwd::engine().subscribe();
            let mut was_offline = rx.borrow_and_update().is_offline();
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                let offline = rx.borrow_and_update().is_offline();
                if was_offline && !offline {
                    log::info!("[qbz-qt] scrobblers: back online, flushing queues");
                    flush_offline_queues().await;
                }
                was_offline = offline;
            }
        });
    });
}

/// Adopt the shared `ListenBrainzCache` credentials when this build's store
/// has no LB token yet (scrobble.rs `seed_listenbrainz_from_shared_cache`).
/// Enable flags are NOT touched — scrobbling stays opt-in per build.
async fn seed_listenbrainz_from_shared_cache() {
    if scrobble_settings().listenbrainz_is_authed() {
        return;
    }
    let Some(path) = listenbrainz_cache_path() else {
        return;
    };
    let creds = tokio::task::spawn_blocking(move || {
        ListenBrainzCache::new(&path).and_then(|c| c.get_credentials())
    })
    .await;
    if let Ok(Ok((Some(token), Some(user_name)))) = creds {
        if !token.is_empty() {
            log::info!("[qbz-qt] adopting ListenBrainz credentials from shared cache");
            if let Err(e) = scrobble().set_listenbrainz_token(&token, &user_name) {
                log::warn!("[qbz-qt] persist adopted ListenBrainz credentials failed: {e}");
            }
        }
    }
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

/// Discord toggle (discord_rpc::set_enabled): persist the opt-in, apply it to
/// the live client, and push the current track (enable) — `set_enabled(false)`
/// tears the IPC connection down, so the presence disappears immediately.
pub fn set_discord_enabled(value: bool) -> Result<(), String> {
    save_pref("discord_rpc_enabled", serde_json::json!(value));
    discord().set_enabled(value);
    if value {
        discord_push(&crate::app());
    }
    log::info!("[qbz-qt] discord_rpc_enabled -> {value}");
    Ok(())
}

pub fn set_musicbrainz_enabled(value: bool) -> Result<(), String> {
    save_pref("musicbrainz_enabled", serde_json::json!(value));
    Ok(())
}

// ---------------------------------------------------------------------------
// Discord Rich Presence (discord_rpc.rs)
// ---------------------------------------------------------------------------

/// Build the "now listening" snapshot from the live queue + playback state and
/// push it. No-op when the user has not opted in (cheap early return — nothing
/// is fetched, nothing connects).
///
/// GLUE: call on the playback track-change edge and on play/pause, mirroring
/// the Tauri service's (track_id, is_playing) transition pushes.
pub fn discord_push(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    if !discord().is_enabled() {
        return;
    }
    let runtime = runtime.clone();
    crate::spawn(async move {
        let state = runtime.core().get_queue_state().await;
        let Some(track) = state.current_track else {
            // Nothing playing — drop the activity.
            let _ = tokio::task::spawn_blocking(|| discord().clear()).await;
            return;
        };
        let pb = runtime.core().get_playback_state();
        let title = match track.version.as_deref().filter(|v| !v.is_empty()) {
            Some(version) => format!("{} ({version})", track.title),
            None => track.title.clone(),
        };
        // Discord's large_image needs an http(s) URL or an asset key; local /
        // Plex covers are filesystem paths Discord can't fetch, so drop them
        // (the core falls back to the "cover" asset key). A sized Qobuz cover
        // is rewritten to the 600 tier first — the queue can carry the 50px
        // `small` from a restored session, and Discord renders whatever the
        // suffix says (owner smoke 2026-08-15: pixelated presence art).
        let cover_url = track
            .artwork_url
            .clone()
            .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
            .map(|u| qbz_models::qobuz_cover_at_px(&u, 600).unwrap_or(u));
        let meta = NowListening {
            title,
            artist: track.artist.clone(),
            album: track.album.clone(),
            is_playing: pb.is_playing,
            current_time: pb.position as f64,
            duration: track.duration_secs as f64,
            cover_url,
        };
        let _ = tokio::task::spawn_blocking(move || discord().update(&meta)).await;
    });
}

/// Tear down the live activity + IPC connection (logout / app exit).
/// GLUE (optional): call from the logout path and the quit handler.
pub fn discord_clear() {
    crate::spawn(async {
        let _ = tokio::task::spawn_blocking(|| discord().clear()).await;
    });
}

// ---------------------------------------------------------------------------
// Connection flows (ScrobbleActions)
// ---------------------------------------------------------------------------

/// ListenBrainz paste-token flow (scrobble.rs `listenbrainz_set_token`):
/// validate against `/validate-token`, persist the token + username to this
/// build's store AND the shared cache (so the other frontends see the same
/// sign-in), and force-enable on the first connect.
pub async fn listenbrainz_set_token(token: &str) {
    let token = token.trim().to_string();
    if token.is_empty() {
        set_status(qbz_i18n::t("Paste your ListenBrainz user token first"), 3);
        crate::settings_qt::publish_snapshot().await;
        return;
    }
    // Re-entrancy guard: the field commits on blur AND the button submits, so
    // both can land back-to-back; one validation at a time.
    {
        let mut g = ui_state().lock().unwrap();
        if g.listenbrainz_busy {
            return;
        }
        g.listenbrainz_busy = true;
    }
    crate::settings_qt::publish_snapshot().await;

    let client = ListenBrainzClient::new();
    match client.set_token(&token).await {
        Ok(info) => {
            if let Err(e) = scrobble().set_listenbrainz_token(&token, &info.user_name) {
                log::error!("[qbz-qt] persist listenbrainz token failed: {e}");
            }
            // First-connect force-enable (scrobble.rs).
            if !scrobble_settings().listenbrainz_enabled {
                let _ = scrobble().set_listenbrainz_enabled(true);
            }
            // Write-through to the shared cache (the Slint/Tauri builds read
            // it at session start). Best-effort.
            if let Some(path) = listenbrainz_cache_path() {
                let tok = token.clone();
                let name = info.user_name.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    ListenBrainzCache::new(&path).and_then(|c| c.save_credentials(&tok, &name))
                })
                .await;
            }
            set_status(
                qbz_i18n::t_args("Connected as {}", &[info.user_name.as_str()]),
                2,
            );
        }
        Err(e) => set_status(qbz_i18n::t_args("Error: {}", &[&e.to_string()]), 3),
    }
    ui_state().lock().unwrap().listenbrainz_busy = false;
    crate::settings_qt::publish_snapshot().await;
}

/// integrations_action dispatch (non-toggle rows).
pub async fn handle_action(_runtime: &Arc<AppRuntime<LoggingAdapter>>, action: &str) {
    match action {
        // Last.fm two-step browser auth (scrobble.rs `lastfm_connect`):
        // request token -> authorize URL in the browser -> Finish exchanges
        // it for a session.
        "lastfm-connect" => {
            {
                let mut g = ui_state().lock().unwrap();
                if g.lastfm_busy {
                    return;
                }
                g.lastfm_busy = true;
            }
            crate::settings_qt::publish_snapshot().await;
            let client = LastFmClient::new();
            match client.get_token().await {
                Ok((token, url)) => {
                    {
                        let mut g = ui_state().lock().unwrap();
                        g.pending_lastfm_token = token;
                        g.lastfm_auth_url = url.clone();
                    }
                    set_status(
                        qbz_i18n::t("Authorize QBZ in your browser, then click \"Finish\""),
                        1,
                    );
                    if let Err(e) = open::that(&url) {
                        log::warn!("[qbz-qt] open Last.fm authorize page failed: {e}");
                    }
                }
                Err(e) => set_status(qbz_i18n::t_args("Error: {}", &[&e.to_string()]), 3),
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
                set_status(qbz_i18n::t("Start the sign-in first"), 3);
                crate::settings_qt::publish_snapshot().await;
                return;
            }
            {
                let mut g = ui_state().lock().unwrap();
                if g.lastfm_busy {
                    return;
                }
                g.lastfm_busy = true;
            }
            crate::settings_qt::publish_snapshot().await;
            let mut client = LastFmClient::new();
            match client.get_session(&token).await {
                Ok(session) => {
                    if let Err(e) = scrobble().set_lastfm_session(&session.key, &session.name) {
                        log::error!("[qbz-qt] persist lastfm session failed: {e}");
                    }
                    // First-connect force-enable (scrobble.rs).
                    if !scrobble_settings().lastfm_enabled {
                        let _ = scrobble().set_lastfm_enabled(true);
                    }
                    {
                        let mut g = ui_state().lock().unwrap();
                        g.pending_lastfm_token.clear();
                        g.lastfm_auth_url.clear();
                    }
                    set_status(
                        qbz_i18n::t_args("Connected as {}", &[session.name.as_str()]),
                        2,
                    );
                }
                Err(e) => set_status(
                    qbz_i18n::t_args(
                        "Error: {} (did you authorize in the browser?)",
                        &[&e.to_string()],
                    ),
                    3,
                ),
            }
            ui_state().lock().unwrap().lastfm_busy = false;
            crate::settings_qt::publish_snapshot().await;
        }
        "lastfm-disconnect" => {
            if let Err(e) = scrobble().disconnect_lastfm() {
                log::error!("[qbz-qt] lastfm disconnect failed: {e}");
            }
            {
                let mut g = ui_state().lock().unwrap();
                g.pending_lastfm_token.clear();
                g.lastfm_auth_url.clear();
                g.lastfm_busy = false;
            }
            set_status(qbz_i18n::t("Last.fm disconnected"), 1);
            crate::settings_qt::publish_snapshot().await;
        }
        "listenbrainz-disconnect" => {
            if let Err(e) = scrobble().disconnect_listenbrainz() {
                log::error!("[qbz-qt] listenbrainz disconnect failed: {e}");
            }
            // Clear the SHARED cache credentials too (mirrors the Slint +
            // Tauri disconnect) — otherwise the next start re-adopts them.
            if let Some(path) = listenbrainz_cache_path() {
                let _ = tokio::task::spawn_blocking(move || {
                    ListenBrainzCache::new(&path).and_then(|c| c.clear_credentials())
                })
                .await;
            }
            ui_state().lock().unwrap().listenbrainz_busy = false;
            set_status(qbz_i18n::t("ListenBrainz disconnected"), 1);
            crate::settings_qt::publish_snapshot().await;
        }
        other => log::warn!("[qbz-qt] unknown integrations action: {other}"),
    }
}

// ===========================================================================
// Fire + schedule (source-agnostic: Qobuz, local AND Plex all funnel through
// the normalized QueueTrack). scrobble.rs `on_track_changed` and below.
// ===========================================================================

/// Normalized track facts the fire path needs. The title is the
/// version-enriched display title so remixes/editions scrobble correctly
/// (issue #360 parity).
#[derive(Clone)]
pub struct ScrobbleMeta {
    pub artist: String,
    pub track: String,
    /// `None` when empty — the clients take `Option<&str>` for album.
    pub album: Option<String>,
    pub duration_secs: u64,
}

/// Build the fire meta from the current queue track (keeps the glue one line).
pub fn meta_from_queue_track(track: &QueueTrack) -> ScrobbleMeta {
    let title = match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    };
    ScrobbleMeta {
        artist: track.artist.clone(),
        track: title,
        // The clean album name — `album_version` is deliberately NOT appended
        // (qbz-models: Last.fm wants the clean name).
        album: Some(track.album.clone()).filter(|a| !a.is_empty()),
        duration_secs: track.duration_secs,
    }
}

/// Monotonic generation, bumped on every track change so a delayed scrobble
/// timer that fires after the user skipped is dropped (the Svelte
/// `clearTimeout` equivalent). Like Tauri, pause/stop do NOT cancel it.
static SCROBBLE_GEN: AtomicU64 = AtomicU64::new(0);

/// Track-change entry point. Fires now-playing immediately for each enabled +
/// authed service, then arms a delayed scrobble at `min(50% of duration,
/// 240s)`. No-op when no service is active (opt-in gate).
///
/// GLUE: call from the playback poll's de-duped track-change edge only — NOT
/// on resume/seek, or the scrobble re-arms.
pub fn on_track_changed(meta: ScrobbleMeta) {
    // Always bump the generation so any in-flight stale timer self-cancels.
    let my_gen = SCROBBLE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let cfg = scrobble_settings();
    if !cfg.lastfm_active() && !cfg.listenbrainz_active() {
        return;
    }
    crate::spawn(async move {
        // Now-playing immediately (skipped while offline — it needs network
        // and is not worth queueing; matches the Svelte path).
        if !crate::offline_fwd::engine().is_offline() {
            send_now_playing(&meta, &cfg).await;
        }

        // Delayed scrobble. Unknown / too-short duration: skip (Last.fm's
        // "longer than 30 seconds" rule lives in qbz_app::scrobble_timing).
        let Some(wait) = scrobble_delay_secs(meta.duration_secs) else {
            log::debug!(
                "[qbz-qt] scrobble: skip delayed scrobble, unusable duration for '{}'",
                meta.track
            );
            return;
        };
        if wait > 0 {
            tokio::time::sleep(Duration::from_secs(wait)).await;
        }
        // Self-cancel if a newer track change superseded us.
        if SCROBBLE_GEN.load(Ordering::SeqCst) != my_gen {
            return;
        }
        send_scrobble(&meta).await;
    });
}

/// The whole integrations reaction to a LOCAL track-change edge, in one call:
/// scrobble now-playing + arm the delayed scrobble, and refresh the Discord
/// presence. Both halves early-return when the user has not opted in, so this
/// is free for everyone else (the queue state is only read when at least one
/// integration is live).
///
/// §11.2 SPLIT (Slint playback.rs:2266-2278): the peer-active guard covers
/// the SCROBBLE half only — never scrobble a peer's audio. The Discord half
/// is deliberately UNGUARDED: in Slint controller mode Discord keeps
/// following the peer's tracks while scrobbles are suppressed (the push
/// inside `refresh_now_playing_meta`, playback.rs:2236-2239, which the peer
/// track edge also reaches, :5137-5141). The poll loop's PEER edge therefore
/// calls [`discord_push`] directly and never this entry. The ephemeral-mode
/// product law (scrobble MAY happen) is unaffected — this guard is about
/// REMOTE ownership, not ephemeral.
///
/// GLUE: call from the playback poll's DE-DUPED track-change edge
/// (`track_id != last_track_id`), never from `refresh_now_playing` — that runs
/// on every play/queue republish and would re-arm the scrobble timer.
pub fn on_track_change_edge(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let cfg = scrobble_settings();
    let scrobblers_live = cfg.lastfm_active() || cfg.listenbrainz_active();
    if !scrobblers_live && !discord().is_enabled() {
        return;
    }
    let rt = runtime.clone();
    crate::spawn(async move {
        if scrobblers_live {
            // The guard (playback.rs:2267-2271): a remote QConnect renderer
            // owns playback — skip the scrobble half ONLY. The spawn's other
            // half (Discord) must still run, so this is a skip, not the
            // reference's early `return` (its spawn holds no Discord half).
            // Fires in the topology-vs-snapshot window, where `is_peer_active`
            // is already true but the poll loop still runs the LOCAL path.
            let peer_active = match crate::qconnect_qt::service() {
                Some(svc) => svc.is_peer_active().await,
                None => false,
            };
            if !peer_active {
                let state = rt.core().get_queue_state().await;
                if let Some(track) = state.current_track {
                    on_track_changed(meta_from_queue_track(&track));
                }
            }
        }
        // UNGUARDED, 1:1 with the reference — Discord follows the peer.
        discord_push(&rt);
    });
}

/// Optional ListenBrainz extras — duration is the only one the QueueTrack
/// carries (no ISRC / MB IDs on the queue model yet).
fn lb_info(duration_secs: u64) -> Option<AdditionalInfo> {
    Some(AdditionalInfo {
        duration_ms: (duration_secs > 0).then_some(duration_secs * 1000),
        ..Default::default()
    })
}

/// Fire "now playing" for each enabled service. Failures only log — the
/// scrobble path is what queues.
async fn send_now_playing(meta: &ScrobbleMeta, cfg: &ScrobblerSettings) {
    let album = meta.album.as_deref();
    if cfg.lastfm_active() {
        let client = LastFmClient::with_session_key(cfg.lastfm_session_key.clone());
        if let Err(e) = client
            .update_now_playing(&meta.artist, &meta.track, album)
            .await
        {
            log::debug!("[qbz-qt] Last.fm now-playing failed: {e}");
        }
    }
    if cfg.listenbrainz_active() {
        let client = ListenBrainzClient::new();
        client
            .restore_token(
                cfg.listenbrainz_token.clone(),
                cfg.listenbrainz_username.clone(),
            )
            .await;
        if let Err(e) = client
            .submit_playing_now(
                &meta.artist,
                &meta.track,
                album,
                lb_info(meta.duration_secs),
            )
            .await
        {
            log::debug!("[qbz-qt] ListenBrainz now-playing failed: {e}");
        }
    }
}

/// Fire the actual scrobble for each enabled service. Engine offline OR call
/// failure queues it — Last.fm to the shared `scrobble_queue`, ListenBrainz to
/// the shared `listen_queue`. Settings are re-read in case the user
/// disconnected while the timer waited.
async fn send_scrobble(meta: &ScrobbleMeta) {
    let cfg = scrobble_settings();
    let album = meta.album.as_deref();
    let timestamp = unix_now();
    let offline = crate::offline_fwd::engine().is_offline();

    if cfg.lastfm_active() {
        let sent = if offline {
            false
        } else {
            let client = LastFmClient::with_session_key(cfg.lastfm_session_key.clone());
            match client
                .scrobble(&meta.artist, &meta.track, album, timestamp as u64)
                .await
            {
                Ok(()) => {
                    log::info!(
                        "[qbz-qt] Last.fm scrobbled: {} - {}",
                        meta.artist,
                        meta.track
                    );
                    true
                }
                Err(e) => {
                    log::warn!("[qbz-qt] Last.fm scrobble failed ({e}); queueing for later");
                    false
                }
            }
        };
        if !sent {
            queue_lastfm(meta, timestamp).await;
        }
    }

    if cfg.listenbrainz_active() {
        let sent = if offline {
            false
        } else {
            let client = ListenBrainzClient::new();
            client
                .restore_token(
                    cfg.listenbrainz_token.clone(),
                    cfg.listenbrainz_username.clone(),
                )
                .await;
            match client
                .submit_listen(
                    &meta.artist,
                    &meta.track,
                    album,
                    timestamp,
                    lb_info(meta.duration_secs),
                )
                .await
            {
                Ok(()) => {
                    log::info!(
                        "[qbz-qt] ListenBrainz scrobbled: {} - {}",
                        meta.artist,
                        meta.track
                    );
                    true
                }
                Err(e) => {
                    log::warn!("[qbz-qt] ListenBrainz scrobble failed ({e}); queueing for later");
                    false
                }
            }
        };
        if !sent {
            queue_listenbrainz(meta, timestamp).await;
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Queue a Last.fm scrobble into the SHARED per-user `offline_settings.db`
/// `scrobble_queue` (the table the other frontends queue into and flush from).
async fn queue_lastfm(meta: &ScrobbleMeta, timestamp: i64) {
    let Some(dir) = crate::sidebar_qt::user_dir() else {
        return;
    };
    let artist = meta.artist.clone();
    let track = meta.track.clone();
    let album = meta.album.clone();
    let _ = tokio::task::spawn_blocking(move || match OfflineModeStore::new_at(&dir) {
        Ok(store) => {
            if let Err(e) = store.queue_scrobble(&artist, &track, album.as_deref(), timestamp) {
                log::warn!("[qbz-qt] queue Last.fm scrobble failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-qt] open offline settings store failed: {e}"),
    })
    .await;
}

/// Queue a ListenBrainz listen into the SHARED per-user
/// `ListenBrainzCache.listen_queue` (the canonical LB offline store).
async fn queue_listenbrainz(meta: &ScrobbleMeta, timestamp: i64) {
    let Some(path) = listenbrainz_cache_path() else {
        return;
    };
    let artist = meta.artist.clone();
    let track = meta.track.clone();
    let album = meta.album.clone();
    let duration_ms = (meta.duration_secs > 0).then_some(meta.duration_secs * 1000);
    let _ = tokio::task::spawn_blocking(move || match ListenBrainzCache::new(&path) {
        Ok(cache) => {
            if let Err(e) = cache.queue_listen(
                timestamp,
                &artist,
                &track,
                album.as_deref(),
                None,
                None,
                None,
                None,
                duration_ms,
            ) {
                log::warn!("[qbz-qt] queue ListenBrainz listen failed: {e}");
            }
        }
        Err(e) => log::warn!("[qbz-qt] open ListenBrainz cache failed: {e}"),
    })
    .await;
}

// ---------------------------------------------------------------------------
// Offline flush — drain both queues (shell entry + every offline->online edge)
// ---------------------------------------------------------------------------

async fn flush_offline_queues() {
    flush_lastfm_queue().await;
    flush_listenbrainz_queue().await;
}

/// Flush the Last.fm queue: up to 50 per pass (the Last.fm batch limit),
/// oldest first; entries older than 14 days are dropped (marked sent) since
/// Last.fm rejects them. Stops at the first network failure and retries on the
/// next edge. Cleans up sent rows older than 7 days afterwards.
async fn flush_lastfm_queue() {
    let cfg = scrobble_settings();
    if !cfg.lastfm_is_authed() {
        return;
    }
    let Some(dir) = crate::sidebar_qt::user_dir() else {
        return;
    };
    let pending = match tokio::task::spawn_blocking({
        let dir = dir.clone();
        move || OfflineModeStore::new_at(&dir).and_then(|s| s.get_queued_scrobbles(50))
    })
    .await
    {
        Ok(Ok(p)) => p,
        _ => return,
    };
    if pending.is_empty() {
        return;
    }

    let client = LastFmClient::with_session_key(cfg.lastfm_session_key.clone());
    let cutoff = unix_now() - 14 * 86400;
    let mut sent_ids: Vec<i64> = Vec::new();
    for item in pending {
        if item.timestamp < cutoff {
            // Too old for Last.fm — drop it (mark sent so it stops re-trying).
            sent_ids.push(item.id);
            continue;
        }
        match client
            .scrobble(
                &item.artist,
                &item.track,
                item.album.as_deref(),
                item.timestamp as u64,
            )
            .await
        {
            Ok(()) => sent_ids.push(item.id),
            Err(e) => {
                log::warn!(
                    "[qbz-qt] Last.fm flush stopped at {} - {}: {e}",
                    item.artist,
                    item.track
                );
                break; // still offline / failing — retry on the next edge
            }
        }
    }
    if !sent_ids.is_empty() {
        let count = sent_ids.len();
        let _ = tokio::task::spawn_blocking(move || {
            OfflineModeStore::new_at(&dir).and_then(|s| {
                s.mark_scrobbles_sent(&sent_ids)?;
                s.cleanup_sent_scrobbles(7)
            })
        })
        .await;
        log::info!("[qbz-qt] Last.fm flush: {count} scrobble(s) sent/cleared");
    }
}

/// Flush the ListenBrainz queue from the shared cache. Stops at the first
/// failure and retries on the next edge.
async fn flush_listenbrainz_queue() {
    let cfg = scrobble_settings();
    if !cfg.listenbrainz_is_authed() {
        return;
    }
    let Some(path) = listenbrainz_cache_path() else {
        return;
    };
    let pending = match tokio::task::spawn_blocking({
        let path = path.clone();
        move || ListenBrainzCache::new(&path).and_then(|c| c.get_pending_listens(500))
    })
    .await
    {
        Ok(Ok(p)) => p,
        _ => return,
    };
    if pending.is_empty() {
        return;
    }

    let client = ListenBrainzClient::new();
    client
        .restore_token(
            cfg.listenbrainz_token.clone(),
            cfg.listenbrainz_username.clone(),
        )
        .await;
    let mut sent_ids: Vec<i64> = Vec::new();
    for item in pending {
        let info = AdditionalInfo {
            recording_mbid: item.recording_mbid.clone(),
            release_mbid: item.release_mbid.clone(),
            artist_mbids: item.artist_mbids.clone(),
            isrc: item.isrc.clone(),
            duration_ms: item.duration_ms,
            ..Default::default()
        };
        if client
            .submit_listen(
                &item.artist_name,
                &item.track_name,
                item.release_name.as_deref(),
                item.listened_at,
                Some(info),
            )
            .await
            .is_ok()
        {
            sent_ids.push(item.id);
        } else {
            break; // still failing — retry on the next edge
        }
    }
    if !sent_ids.is_empty() {
        let count = sent_ids.len();
        let _ = tokio::task::spawn_blocking(move || {
            ListenBrainzCache::new(&path).and_then(|c| c.mark_listens_sent(&sent_ids))
        })
        .await;
        log::info!("[qbz-qt] ListenBrainz flush: {count} listen(s) sent");
    }
}
