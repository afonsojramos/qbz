//! qbz-qt — Qt/QML frontend POC entry point.
//!
//! Phase 1: boot flow (splash -> silent session restore -> shell
//! placeholder | login screen) with all the offline guardrails of the
//! Slint app. `main` is NOT async: `QGuiApplication::exec()` owns the main
//! thread, so a process-global multi-thread tokio runtime carries all
//! async work; results hop back to Qt through the bridge's `CxxQtThread`.

mod auth_qt;
// Per-domain QML bridge singletons (phase 23 — the QbzBridge God-object
// split). THE PATTERN (phase-1, replicated per domain file):
//   1. one #[cxx_qt::bridge] mod per file (crate root — cxx-qt-build
//      accepts rust_files from ONE directory only, QTBUG-93443) with a
//      single #[qml_element] #[qml_singleton] QObject + its *Rust storage;
//   2. `static QT_THREAD: OnceLock<CxxQtThread<QbzDomain>>` per file;
//   3. the singleton's `boot()` invokable registers `self.qt_thread()` —
//      Main.qml boots EVERY singleton (they instantiate lazily); only
//      QbzSession.boot also fires crate::on_boot();
//   4. `pub(crate) fn ui(f)` queues a property mutation onto that
//      object's Qt event loop (no-op pre-registration).
// All singletons live in the SAME QML module (com.blitzfc.qbz); the
// invokables stay one-line forwards into the domain controllers below.
mod session_bridge;
mod shell_bridge;
mod player_bridge;
mod queue_bridge;
mod home_bridge;
mod viz_bridge;
mod local_bridge;
mod library_bridge;
mod album_bridge;
mod artist_bridge;
mod cast_bridge;
// MyQBZ splits across THREE singletons rather than one, because the two
// modals are global overlays with their own lifetime: QbzMyQbz carries the
// two grids + the detail page + the edit/mix modals + the branding,
// QbzMyQbzAdd carries only the app-wide "Add to Mixtape/Collection" picker
// (mounted in AppShell, reachable from any view's row menu), and QbzDisco
// carries the Artist-Collection builder.
mod myqbz_bridge;
mod myqbz_add_bridge;
mod disco_bridge;
mod blacklist_bridge;
mod playlist_picker_bridge;
// Playlist Manager. THREE singletons, not one (contract D2): the manager
// document + folder list + organisation writes here, the folder modals on
// QbzFolderEdit and the shared playlist editor on QbzPlaylistEdit — so a
// folder save cannot perturb an open playlist editor.
mod playlist_manager_bridge;
mod folder_edit_bridge;
mod playlist_edit_bridge;
// Playlist Importer (public Spotify / Apple Music / Tidal / Deezer playlists).
// Its own singleton: a separate domain from both the playlist detail and the
// manager, opened from two shell surfaces that outlive each other, and its
// modal must survive the one that opened it (05 §5.8).
mod playlist_import_bridge;
// Qobuz Connect (2026-08-01 contract §8, block B4-Rust): the QbzQConnect
// singleton — the QML surface of the facade/sink controllers below
// (qconnect_qt.rs / qconnect_event_sink_qt.rs).
mod qconnect_bridge;
mod artwork_qt;
mod atmosphere_qt;
mod bridge;
mod discover_config_qt;
mod fav_cache_qt;
// The folder-modal controller. A plain module — it declares no
// #[cxx_qt::bridge], so it must NOT appear in build.rs's rust_files.
mod folder_edit_qt;
mod folders_qt;
mod foryou_qt;
mod genre_filter_qt;
mod home_qt;
mod icon_tint_qt;
mod recently_qt;
mod recommendations_qt;
mod library_db_qt;
mod library_qt;
mod library_bulk;
mod library_prefs;
mod library_sidepanel;
mod local_library_qt;
mod local_artist_match;
mod local_rows;
mod local_state;
mod local_plex;
mod local_albums;
mod local_tree;
mod local_artwork;
mod local_playback;
mod local_playlist_qt;
mod local_bridge_ops;
mod local_bulk;
mod local_ephemeral;
mod local_album_actions;
// MyQBZ domain controllers. One module per concern, all driven by the three
// bridges above: the grids + Create modal (myqbz_qt), the per-user branding
// and per-collection view prefs (myqbz_prefs_qt), the detail page
// (myqbz_detail_qt) and its playback / edit / cover / DJ-mix arms, the
// app-wide Add picker (myqbz_add_qt) and the Artist-Collection builder
// (myqbz_builder_qt + its Qobuz/local/Plex fetchers).
mod myqbz_qt;
mod myqbz_prefs_qt;
mod myqbz_detail_qt;
mod myqbz_play_qt;
mod myqbz_edit_qt;
mod myqbz_cover_qt;
mod myqbz_mix_qt;
mod myqbz_add_qt;
mod myqbz_builder_qt;
mod myqbz_builder_fetch_qt;
// Blacklist: the per-user store (artist_blacklist), the Recommendations
// dismissal store (reco_dismiss_qt) and the manager view's controller.
mod artist_blacklist;
mod reco_dismiss_qt;
mod blacklist_qt;
// Shared in-app toast publisher (the port of qbz-slint-common's toast.rs).
// A plain module, NOT a bridge — it publishes onto QbzShell.toastJson, and
// `controls/QbzToast.qml` owns the auto-hide timer.
mod toast_qt;
mod nav_qt;
mod browse_qt;
mod cast_qt;
mod label_qt;
mod now_playing;
mod output_labels;
mod quality_state;
mod offline_fwd;
mod album_qt;
mod track_info_qt;
mod external_reco_qt;
mod ambient_qt;
mod artist_qt;
mod lyrics_qt;
mod playback_qt;
mod playlist_picker_qt;
// Playlist Importer controller (the `qbz-playlist-import` crate's frontend
// half). Plain module — it declares no #[cxx_qt::bridge], so it must NOT
// appear in build.rs's rust_files.
mod playlist_import_qt;
// Playlist Manager controller, split three ways up front (the reference is
// 1053 lines BEFORE the two state machines and two serializers this port
// adds): _qt = load / cache / toolbar / publish, _rows = the pure model
// functions, _ops = the optimistic mutations. Plain modules — they declare no
// #[cxx_qt::bridge], so they must NOT appear in build.rs's rust_files.
mod playlist_manager_ops;
mod playlist_manager_qt;
mod playlist_manager_rows;
// The SHARED playlist editor's controller (rename · description ·
// offline-only · delete), driven from the manager's three delegates, the
// sidebar row menu and the playlist detail header. Plain module — it declares
// no #[cxx_qt::bridge], so it must NOT appear in build.rs's rust_files.
mod playlist_edit_qt;
mod playlist_qt;
mod queue_qt;
mod search_qt;
mod settings_qt;
mod sidebar_qt;
mod sleep_timer_qt;
mod theme_qt;
mod integrations_qt;
mod viz_qt;
// Qobuz Connect port (2026-08-01 contract), blocks B1/B2: transport config +
// credential discovery + device identity, and the renderer engine over
// `runtime.core()`. Plain modules — they declare no #[cxx_qt::bridge], so
// they must NOT appear in build.rs's rust_files. Wired by the B3 facade.
mod qconnect_engine_qt;
mod qconnect_transport_qt;
// QConnect port block B3: the facade (service singleton + controller routing)
// and the inbound event sink. Plain modules, same convention as B1/B2. The
// §11.5 wiring (init_service + spawns) lives in `on_session_entered` below.
mod qconnect_event_sink_qt;
mod qconnect_qt;

use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};

use cxx_qt::CxxQtThread;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use bridge::qbz_bridge::QbzBridge;

/// Same URL the Slint shell opens for the Terms-of-Service link.
pub(crate) const QOBUZ_TOS_URL: &str = "https://www.qobuz.com/us-en/legal/terms";

/// The async runtime for everything non-Qt (OAuth, session restore,
/// connectivity monitoring). Lives for the whole process.
static TOKIO: OnceLock<Runtime> = OnceLock::new();

/// The UI-agnostic composition root (core + runtime state + session).
static APP: OnceLock<Arc<AppRuntime<LoggingAdapter>>> = OnceLock::new();

/// Qt-thread handle of the singleton bridge, registered by its first
/// invokable (`boot`). `CxxQtThread` is Send+Sync and safe to queue on from
/// any thread; before registration, `ui()` is a no-op.
static QT_THREAD: OnceLock<CxxQtThread<QbzBridge>> = OnceLock::new();

/// In-flight browser-OAuth task, so `cancelLogin()` can `abort()` it
/// (mirrors `login_task` in crates/qbz/src/main.rs).
static LOGIN_TASK: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

/// Whether the Home view has been loaded this session (the auto-load fires
/// once per shell entry; `reloadHome()` bypasses the flag).
static HOME_LOADED: Mutex<bool> = Mutex::new(false);

/// Whether the favourite-id cache has had its network refresh this session
/// (`warm_favorites_once`). Reset on logout with the rest.
static FAV_WARMED: Mutex<bool> = Mutex::new(false);

/// Whether the sidebar tree has been built this session (`load_sidebar_once`).
///
/// MODULE-LEVEL on purpose. It used to be a function-local `static` inside
/// `load_sidebar_once`, which made it PROCESS-scoped with no way to reach it
/// from `do_logout` — so after logout -> login the latch was still set, the
/// tree was never rebuilt for the new session, and the sidebar stayed on the
/// `"[]"` the logout block publishes until the user found the Refresh row.
/// Owner-reproduced 2026-07-31 (qbz.log 20:33:32 login -> no `sidebar loaded`
/// until 20:33:47, which is the manual refresh). Reset with its siblings below.
static SIDEBAR_LOADED: Mutex<bool> = Mutex::new(false);

/// Drop every "once per session" latch. Called at each session ENTRY
/// (`on_session_entered`) and again on logout, so neither a logout -> login nor
/// an offline -> login transition can leave a new session reading the previous
/// one's answer. Each latch's own doc explains what it gates.
fn reset_session_latches() {
    *HOME_LOADED.lock().unwrap() = false;
    *LIBRARY_LOADED.lock().unwrap() = false;
    *FAV_WARMED.lock().unwrap() = false;
    *SIDEBAR_LOADED.lock().unwrap() = false;
}

pub(crate) fn register_qt_thread(thread: CxxQtThread<QbzBridge>) {
    if QT_THREAD.set(thread).is_err() {
        log::warn!("[qbz-qt] Qt thread handle already registered");
    }
}

pub(crate) fn app() -> Arc<AppRuntime<LoggingAdapter>> {
    Arc::clone(APP.get().expect("AppRuntime not initialized"))
}

/// Spawn a future on the process-global tokio runtime.
pub(crate) fn spawn(future: impl std::future::Future<Output = ()> + Send + 'static) {
    TOKIO
        .get()
        .expect("tokio runtime not initialized")
        .spawn(future);
}

/// Queue a bridge mutation onto the Qt event loop — the cxx-qt analogue of
/// Slint's `upgrade_in_event_loop`. No-op before the bridge registers its
/// thread handle (boot's first step) or after it is destroyed.
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzBridge>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

/// Boot sequence, fired by the bridge's `boot` invokable (Main.qml
/// Component.onCompleted — the Qt-thread handle is registered first, so
/// every `ui()` hop below lands).
pub(crate) fn on_boot() {
    // The source registry, FIRST of all — before anything can call
    // `qbz_source::registry()`. It is a OnceLock, so whoever touches it first
    // BUILDS it, and a registry built without the client lens leaves the Qobuz
    // source permanently detached: every catalog call then answers
    // `NotConfigured`, loudly but uselessly. Measured exactly that way on the
    // first wiring pass — the local and Plex rows resolved while all 26 Qobuz
    // albums came back "no client lens installed".
    //
    // A LENS, not a cached clone: `qbz-core` REPLACES its client (core.rs:346
    // and :384, both from paths that run before `set_session`), so anything
    // holding a clone goes stale and fails silently. Reading through on every
    // call is what makes that unrepresentable. The read guard lives inside the
    // returned future and is dropped with it, which is the same discipline
    // `myqbz_play_qt` documents for its own clone-then-drop.
    qbz_source::init_registry(std::sync::Arc::new(|| {
        Box::pin(async {
            let lock = app().core().client();
            let guard = lock.read().await;
            guard.as_ref().cloned()
        })
    }));

    // Offline guardrails FIRST (before the login screen can show): engine +
    // connectivity actor, then the live status forwarder. Both need the
    // tokio runtime context for their spawns.
    spawn(async {
        offline_fwd::start();
    });
    offline_fwd::start_ui_forwarder();

    // Derivative-cache housekeeping, off the Qt thread. Once per run: the
    // `.jpg` orphan sweep is idempotent (FIX 1 moved the scaled derivatives to
    // `.png`, so every `.jpg` left in `images/scaled/` is dead weight from a
    // pre-fix build) and the byte cap is cheap — one `read_dir`, then unlink
    // the oldest until the directory is back under the ceiling.
    spawn(async {
        let _ = tokio::task::spawn_blocking(artwork_qt::housekeeping).await;
    });

    // Splash -> silent session restore -> shell | login.
    let runtime = app();
    spawn(async move {
        // Best-effort, offline-tolerant (mirrors the Slint boot): a network
        // failure here leaves the core usable for offline playback.
        if let Err(e) = runtime.init().await {
            log::warn!("[qbz-qt] core init failed (continuing): {e}");
        }
        match auth_qt::restore_saved_session(&runtime).await {
            Ok(Some(session)) => enter_shell(session),
            Ok(None) => session_bridge::ui(|mut b| b.as_mut().set_screen(QString::from("login"))),
            Err(e) => session_bridge::ui(move |mut b| {
                                b.as_mut().set_restore_error(QString::from(e.as_str()));
                b.as_mut().set_screen(QString::from("login"));
            }),
        }
    });
}

/// Everything a SESSION ENTRY owes the shell, whatever the door was.
///
/// Extracted from [`enter_shell`] because `start_offline` — the "Start offline"
/// button, the ONLY way into the app for a user with no network — did NOT run
/// any of it. It recorded the nav entry, pushed the now-playing model and
/// switched the screen; the sidebar tree, the 1 Hz playback pump, the
/// integrations runtime, the streaming-quality seed, the settings document and
/// the persisted volume were all skipped. On a warm session (login first, then
/// offline) most of that had already run in the same process and hid the hole;
/// on a COLD offline start the shell came up with an empty sidebar (no folders,
/// no LOCAL playlists — the entire library for an account-less user), no
/// playback pump and the volume at 100 % on an exclusive-mode DAC.
///
/// Every step below is either idempotent or self-gates offline
/// (`load_home_once` / `warm_favorites_once` return early and, importantly,
/// leave their latch UNSET so a later online session still runs them), so the
/// two callers can share it verbatim.
fn on_session_entered() {
    // The once-per-session latches, reset HERE and not only in `do_logout`:
    // logging in FROM an offline session (the D2 recovery banner, or the login
    // screen after "Start offline") never passes through logout — verified in
    // the owner's 2026-07-31 log, where the OAuth exchange at 20:33:32 follows
    // the offline entry at 20:32:38 with no `logged out` line between them. A
    // latch reset only at logout would leave that transition with the OFFLINE
    // sidebar (folders + locals, no Qobuz playlists) for the rest of the run.
    // Every caller of this function is one genuine session entry, so this is
    // the one place that sees them all.
    reset_session_latches();
    // Phase 2: the shell mounts on the (only) "home" view; seed the nav
    // history and push the current now-playing model onto the bar.
    nav_qt::record("home");
    now_playing::publish_current();
    // Phase 3: fetch Discover > Home (online sessions only — the offline
    // engine gates Qobuz calls anyway, and the view shows the offline
    // placeholder instead).
    load_home_once();
    // Phase 4: the 1 Hz playback state pump (idempotent).
    playback_qt::start_poll_loop(app());
    // Integrations runtime: applies the persisted MusicBrainz / Discord /
    // ListenBrainz opt-ins and starts the scrobble-queue flush watcher.
    // Strictly opt-in — every one of them is inert until the user connects it.
    integrations_qt::start(&app());
    // Phase 7: the sidebar playlist tree.
    load_sidebar_once();
    // Refresh the favourite-id cache from the network. The disk seed already
    // ran at session activation (auth_qt -> fav_cache_qt::init_for_user);
    // this is the reference's shell-entry warm (crates/qbz/src/main.rs:418-500)
    // and it is what keeps album / artist / track / label hearts — and the
    // toggle direction behind them — correct without ever opening Library.
    warm_favorites_once();
    // Phase 10: seed the playback request tier from the persisted
    // streaming-quality pref (Settings > Audio writes it live after this).
    playback_qt::set_streaming_quality(&settings_qt::streaming_quality());
    // Seed the settings document ONCE at shell entry. It used to be published
    // only by `navigate_to("settings")` and by a language change, so until the
    // user opened Settings the shell read an EMPTY doc — and the now-playing
    // bars' "Audio settings" flyout reads `normalization` / `gapless` from it.
    // That is what made normalization impossible to turn off: the bars saw
    // `undefined`, drew the control OFF while the backend had it ON, and the
    // first interaction wrote the WRONG value back. Slint has no equivalent
    // hole because `SettingsState` is a global seeded at startup.
    publish_settings();

    // Restore the persisted player volume so audio starts at the SAVED level
    // (1:1 with qbz/src/main.rs:220-227). Without it every launch started at
    // 100% — on an exclusive-mode DAC that is not a cosmetic default. The poll
    // loop mirrors it onto the bar's slider from the engine, so nothing else
    // has to publish it.
    let restored = crate::settings_qt::read_pref_f32("volume").unwrap_or(1.0).clamp(0.0, 1.0);
    let rt = app();
    spawn(async move { playback_qt::set_volume(&rt, restored).await });

    // Qobuz Connect service wiring (2026-08-01 contract §11.5), in order.
    // This fn is MULTI-ENTRY (login, session restore AND start_offline all
    // land here), and every caller runs on the tokio runtime, so
    // `Handle::current()` is valid.
    // 1. The service singleton — without it `service()` is None and every
    //    arm no-ops silently. OnceLock-idempotent, so the multi-entry seam
    //    calls it every time.
    let qconnect_service = qconnect_qt::init_service(app());
    let tokio_handle = tokio::runtime::Handle::current();
    {
        // 2. The offline force-disconnect watcher — ONCE per process: the fn
        //    carries NO idempotency guard of its own (unlike the auto-connect
        //    below), so a naive re-spawn per shell entry would leak one
        //    permanent watcher task per entry (double force-disconnect,
        //    double badge flip).
        use std::sync::atomic::{AtomicBool, Ordering};
        static OFFLINE_WATCHER_SPAWNED: AtomicBool = AtomicBool::new(false);
        if !OFFLINE_WATCHER_SPAWNED.swap(true, Ordering::SeqCst) {
            qconnect_service.spawn_offline_force_disconnect(&tokio_handle);
        }
    }
    // 3. Startup auto-connect — gated on NOT offline: the offline shell entry
    //    never auto-connects, and calling the spawn there would burn its
    //    internal once-per-process FIRED latch before a real online entry
    //    could use it. (The task itself also re-checks offline inside its
    //    retry loop and bails silently.)
    if !offline_fwd::engine().status().is_offline() {
        qconnect_qt::spawn_startup_auto_connect(&tokio_handle);
    }
}

/// Post-login UI state: the shared session entry, then the session header,
/// has-previous-session, cleared login errors/phase and the screen switch.
fn enter_shell(session: auth_qt::SessionInfo) {
    on_session_entered();
    session_bridge::ui(move |mut b| {
        b.as_mut().set_session_user_name(QString::from(session.display_name.as_str()));
        b.as_mut().set_session_subscription(QString::from(session.subscription.as_str()));
        b.as_mut().set_has_previous_session(true);
        b.as_mut().set_login_error(QString::from(""));
        b.as_mut().set_restore_error(QString::from(""));
        b.as_mut().set_login_phase(0);
        b.as_mut().set_screen(QString::from("shell"));
    });
}

/// Login screen primary button / recovery banner: the system-browser OAuth
/// flow on a background task; phases and results hop back to Qt.
pub(crate) fn start_login() {
    {
        let guard = LOGIN_TASK.lock().unwrap();
        if guard.is_some() {
            log::warn!("[qbz-qt] login already in progress, ignoring");
            return;
        }
    }
    session_bridge::ui(|mut b| {
                b.as_mut().set_login_error(QString::from(""));
        b.as_mut().set_login_phase(1);
    });

    let runtime = app();
    let handle = TOKIO.get().unwrap().spawn(async move {
        let result = auth_qt::login_via_system_browser(&runtime, |phase| {
            let value = match phase {
                auth_qt::LoginPhase::WaitingForBrowser => 1,
                auth_qt::LoginPhase::Authenticating => 2,
            };
            session_bridge::ui(move |mut b| b.as_mut().set_login_phase(value));
        })
        .await;
        LOGIN_TASK.lock().unwrap().take();
        match result {
            Ok(session) => enter_shell(session),
            Err(e) => session_bridge::ui(move |mut b| {
                                b.as_mut().set_login_phase(0);
                b.as_mut().set_login_error(QString::from(e.as_str()));
            }),
        }
    });
    *LOGIN_TASK.lock().unwrap() = Some(handle);
}

/// Cancel link (phase 1): abort the in-flight OAuth and return to idle.
pub(crate) fn cancel_login() {
    if let Some(task) = LOGIN_TASK.lock().unwrap().take() {
        task.abort();
    }
    session_bridge::ui(|mut b| b.as_mut().set_login_phase(0));
}

/// "Start offline": unauthenticated offline session -> the shell.
///
/// It runs the SAME [`on_session_entered`] sequence a login does. It used to
/// run two lines of it (nav + now-playing), which is why an offline session
/// came up with no sidebar at all — no folders, no local playlists — and the
/// Refresh row was the only way back. See `on_session_entered`'s header.
pub(crate) fn start_offline() {
    let runtime = app();
    spawn(async move {
        match auth_qt::start_offline_session(&runtime).await {
            Ok(user_id) => {
                let name = if user_id == 0 {
                    "Guest (offline)".to_string()
                } else {
                    format!("Offline (user {user_id})")
                };
                // The full session entry, in the same order the login path
                // uses it — NOT the two-line subset this used to run.
                on_session_entered();
                session_bridge::ui(move |mut b| {
                    b.as_mut().set_session_user_name(QString::from(name.as_str()));
                    b.as_mut().set_session_subscription(QString::from(""));
                    b.as_mut().set_login_error(QString::from(""));
                    b.as_mut().set_restore_error(QString::from(""));
                    b.as_mut().set_login_phase(0);
                    b.as_mut().set_screen(QString::from("shell"));
                });
            }
            Err(e) => {
                log::error!("[qbz-qt] failed to enter offline mode: {e}");
                session_bridge::ui(move |mut b| {
                    b.as_mut().set_login_error(QString::from(e.as_str()));
                });
            }
        }
    });
}

/// Shell logout: token + session teardown, then back to the login screen.
pub(crate) fn do_logout() {
    let runtime = app();
    spawn(async move {
        // A connected renderer keeps playing after the session ends unless it
        // is torn down explicitly (the cast service owns its own socket).
        cast_qt::service().shutdown().await;
        if let Err(e) = auth_qt::logout(&runtime).await {
            log::error!("[qbz-qt] logout failed: {e}");
        }
        // Integrations: drop the Discord presence so a signed-out session
        // stops advertising what was playing (the Slint discord_rpc::clear).
        integrations_qt::discord_clear();
        // The local-library documents are per-user; a later login must not
        // inherit the previous user's cached tree.
        local_library_qt::reset();
        // A later login must re-fetch Home + Library (new user, fresh data)
        // and re-warm the favourite-id cache that `auth_qt::logout` just
        // emptied — otherwise the next account's hearts stay blank until it
        // opens Library.
        //
        // The SIDEBAR latch is in that set too, for the same reason plus a
        // sharper one: the block below publishes `"[]"` into the tree, so a
        // latched `load_sidebar_once` leaves the next session staring at an
        // empty sidebar with no way back except the Refresh row
        // (owner-reproduced 2026-07-31). It is only resettable at all because
        // it now lives at module level — see SIDEBAR_LOADED.
        //
        // `on_session_entered` resets the same set: logout is NOT the only way
        // a session ends (offline -> login never passes through here).
        reset_session_latches();
        // The tree's own CACHE, not just the published document. It holds the
        // outgoing user's playlists, folders, folder membership, hidden set and
        // local rows, and NOTHING else clears it — every `publish_sidebar()`
        // rebuilds straight from it (the Playlist Manager's optimistic
        // move-to-folder does exactly that), so a leftover cache re-renders the
        // previous account's tree for the next one. Same class as the
        // `local_library_qt::reset()` above.
        sidebar_qt::teardown();
        session_bridge::ui(|mut b| {
            b.as_mut().set_session_user_name(QString::from(""));
            b.as_mut().set_session_subscription(QString::from(""));
            b.as_mut().set_login_error(QString::from(""));
            b.as_mut().set_restore_error(QString::from(""));
            b.as_mut().set_login_phase(0);
            b.as_mut().set_screen(QString::from("login"));
        });
        home_bridge::ui(|mut b| {
            b.as_mut().set_home_sections_json(QString::from("[]"));
            b.as_mut().set_home_error(QString::from(""));
            b.as_mut().set_home_loading(false);
        });
        library_bridge::ui(|mut b| {
            b.as_mut().set_library_json(QString::from("[]"));
            b.as_mut().set_library_counts_json(QString::from("{}"));
            b.as_mut().set_library_error(QString::from(""));
            b.as_mut().set_library_loading(false);
        });
        shell_bridge::ui(|mut b| {
            b.as_mut().set_sidebar_json(QString::from("[]"));
        });
    });
}

// ============================ Discover > Home ==============================

/// Fire the Home auto-load once per shell entry. Gated on the offline
/// engine: offline sessions show the OfflinePlaceholder instead of
/// fetching (the Qobuz gate would refuse the calls anyway).
fn load_home_once() {
    if *HOME_LOADED.lock().unwrap() {
        return;
    }
    if offline_fwd::engine().status().is_offline() {
        log::info!("[qbz-qt] home load skipped (offline session)");
        return;
    }
    *HOME_LOADED.lock().unwrap() = true;
    reload_home();
}

// ============================ Favourites cache =============================

/// Network refresh of the favourite-id cache, once per shell entry.
///
/// Skipped offline on purpose: the disk seed from `init_for_user` is the truth
/// there, and the Qobuz gate would refuse the calls anyway. The flag is reset
/// on logout alongside HOME_LOADED — `fav_cache_qt::teardown()` empties the
/// sets, so a second login in the same process MUST be able to warm again.
fn warm_favorites_once() {
    if offline_fwd::engine().status().is_offline() {
        log::info!("[qbz-qt] favorites cache warm skipped (offline session)");
        return;
    }
    {
        let mut guard = FAV_WARMED.lock().unwrap();
        if *guard {
            return;
        }
        *guard = true;
    }
    let runtime = app();
    spawn(async move {
        library_qt::warm_favorites_cache(&runtime).await;
    });
}

// ============================ Playback (phase 4) ===========================

// ============================ Sidebar + pins (phase 7) =====================

/// Load + publish the sidebar tree (session entry; idempotent per session).
///
/// It goes through [`reload_sidebar_including_local`], NOT [`reload_sidebar`],
/// and it has no offline early return of its own. A session that STARTS
/// offline used to get an entirely empty sidebar — no folders, no local
/// playlists — which is the whole library for a user with no Qobuz account.
/// `sidebar_qt::load` gates only its Qobuz fetch (preserving whatever the
/// cache already holds), so offline this reads folders + locals and publishes
/// them; online it is the same call it always was.
/// The latch is [`SIDEBAR_LOADED`], a MODULE-level static reset by `do_logout`
/// — see its declaration for the logout -> login bug a function-local one
/// caused.
fn load_sidebar_once() {
    {
        let mut guard = SIDEBAR_LOADED.lock().unwrap();
        if *guard {
            return;
        }
        *guard = true;
    }
    reload_sidebar_including_local();
}

/// Fetch playlists + folders and publish the flattened entries.
pub(crate) fn reload_sidebar() {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    let runtime = app();
    spawn(async move {
        sidebar_qt::load(&runtime).await;
        publish_sidebar();
    });
}

/// Same rebuild, but it also runs OFFLINE.
///
/// `reload_sidebar` bails while offline because its Qobuz fetch cannot
/// succeed. The sidebar's LOCAL playlists can still change in that state — the
/// picker's D8 branch creates one offline — and for a user with no Qobuz
/// account they are the entire sidebar, so a no-op there means the playlist
/// they just created never appears. `sidebar_qt::load` already reads the
/// locals independently of the Qobuz fetch and survives it failing or being
/// gate-refused, so there is nothing to guard against here beyond one wasted
/// (and already-handled) request.
pub(crate) fn reload_sidebar_including_local() {
    let runtime = app();
    spawn(async move {
        sidebar_qt::load(&runtime).await;
        publish_sidebar();
    });
}

/// Republish the flattened entries from the sidebar CACHE — no fetch, no DB
/// read.
///
/// `pub(crate)` since the Playlist Manager landed: its optimistic move-to-folder
/// patches `sidebar_qt::CACHE` in place and then republishes from it, which is
/// the only refresh that is correct offline AND does not cost a round trip
/// (contract D10).
pub(crate) fn publish_sidebar() {
    let entries = sidebar_qt::rebuild();
    let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into());
    log::debug!("[qbz-qt] sidebar published: {} entries ({} bytes)", entries.len(), json.len());
    let (sort_by, sort_asc) = sidebar_qt::sort_state();
    shell_bridge::ui(move |mut b| {
        b.as_mut().set_sidebar_json(QString::from(json.as_str()));
        b.as_mut().set_sidebar_sort_by(QString::from(sort_by.as_str()));
        b.as_mut().set_sidebar_sort_asc(sort_asc);
    });
}

pub(crate) fn sidebar_set_sort(option: &str) {
    sidebar_qt::set_sort(option);
    publish_sidebar();
}

pub(crate) fn sidebar_set_search(query: &str) {
    sidebar_qt::set_search(query);
    publish_sidebar();
}

pub(crate) fn sidebar_toggle_folder(id: &str) {
    sidebar_qt::toggle_folder(id);
    publish_sidebar();
}

/// Mini-rail folder flyout: publish that folder's playlists (contract §4.7).
///
/// This exists so the flyout does NOT have to force-expand the folder to see
/// its children. `sidebar_json` only carries a folder's rows while it is
/// EXPANDED, so the old path called `sidebar_toggle_folder` on the way in — a
/// persistent side effect (the folder stayed open once the sidebar was
/// re-opened) that the reference does not have. `sidebar_qt::folder_popup_rows`
/// answers from the session CACHE instead, which is also what makes this work
/// offline and on the Qt thread.
///
/// `count` is `rows.len()`, deliberately: the reference takes it from the entry
/// row, which is computed AFTER the playlist search filter that
/// `load_folder_popup` does not apply, so its header count and its list can
/// legitimately disagree. `folderName` is a best-effort cache read — the flyout
/// takes the name it renders from the clicked entry, synchronously, because
/// this document lands a later event-loop turn.
pub(crate) fn sidebar_open_folder_popup(folder_id: &str) {
    let rows = sidebar_qt::folder_popup_rows(folder_id);
    let count = rows.len();
    let name = sidebar_qt::folder_name(folder_id).unwrap_or_default();
    let doc = serde_json::json!({
        "folderId": folder_id,
        "folderName": name,
        "count": count,
        "rows": rows,
    });
    let json = doc.to_string();
    log::debug!("[qbz-qt] sidebar folder popup: {folder_id} ({count} rows)");
    shell_bridge::ui(move |mut b| {
        b.as_mut().set_sidebar_folder_popup_json(QString::from(json.as_str()));
    });
}

/// Sidebar cover dispatch: plain url list (the tree's collage is
/// url-keyed). Disk hits emit immediately; misses download then emit.
pub(crate) fn sidebar_artwork_window(urls_json: String) {
    let urls: Vec<String> = serde_json::from_str(&urls_json).unwrap_or_default();
    log::debug!("[qbz-qt] sidebar artwork window: {} urls", urls.len());
    let mut missing: Vec<String> = Vec::new();
    for url in urls {
        let path = artwork_qt::cached_path(&url);
        if path.is_empty() {
            missing.push(url);
        } else {
            emit_library_artwork(url, path);
        }
    }
    if missing.is_empty() {
        return;
    }
    spawn(async move {
        let urls = missing;
        artwork_qt::download_missing(urls.clone()).await;
        for url in urls {
            let path = artwork_qt::cached_path(&url);
            if !path.is_empty() {
                emit_library_artwork(url, path);
            }
        }
    });
}

// ============================ Album / Artist views (phase 8) ==============

/// Open the album detail view: push the nav entry, then fetch + publish.
pub(crate) fn open_album(album_id: String) {
    // A LOCAL or Plex album id is a group key or a path, never a Qobuz catalog
    // id. Sending one to /album/get returns 404 and the view lands empty — the
    // mixed Library "All" feed hands both kinds to the same card, so the routing
    // has to happen HERE rather than in each call site.
    if library_qt::is_local_feed_id("album", &album_id) {
        nav_qt::record("localalbum");
        shell_bridge::ui(|mut b| b.as_mut().set_current_view(QString::from("localalbum")));
        local_bridge::open_album_by_id(album_id);
        return;
    }
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    nav_qt::record("album");
    *LAST_DETAIL.lock().unwrap() = ("album".to_string(), album_id.clone());
    let runtime = app();
    album_bridge::ui(move |mut b| {
        // Clear the PREVIOUS album in the same hop that raises the loading
        // flag: without it the view renders the last album until the fetch
        // lands, so opening B after A showed A.
        b.as_mut().set_album_json(QString::from("{}"));
        b.as_mut().set_album_loading(true);
    });
    spawn(async move {
        match album_qt::load_album_view(&runtime, &album_id).await {
            Ok(json) => {
                                album_bridge::ui(move |mut b| {
                    b.as_mut().set_album_json(QString::from(json.as_str()));
                    b.as_mut().set_album_loading(false);
                })
            },
            Err(e) => {
                log::warn!("[qbz-qt] album view load failed: {e}");
                album_bridge::ui(move |mut b| b.as_mut().set_album_loading(false));
            }
        }
    });
}

/// Open the artist detail view: push the nav entry, then fetch + publish.
pub(crate) fn open_artist(artist_id: String) {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    nav_qt::record("artist");
    *LAST_DETAIL.lock().unwrap() = ("artist".to_string(), artist_id.clone());
    let runtime = app();
    artist_bridge::ui(move |mut b| {
        // Same as open_album: stale artist until the fetch lands otherwise.
        b.as_mut().set_artist_json(QString::from("{}"));
        b.as_mut().set_artist_loading(true);
    });
    spawn(async move {
        match artist_qt::load_artist_view(&runtime, &artist_id).await {
            Ok(json) => artist_bridge::ui(move |mut b| {
                b.as_mut().set_artist_json(QString::from(json.as_str()));
                b.as_mut().set_artist_loading(false);
            }),
            Err(e) => {
                log::warn!("[qbz-qt] artist view load failed: {e}");
                artist_bridge::ui(move |mut b| b.as_mut().set_artist_loading(false));
            }
        }
    });
}

/// ArtistView "Load more" for one releases bucket (page 20, has_more).
pub(crate) fn load_release_section(artist_id: String, release_type: String, offset: i32) {
    let runtime = app();
    spawn(async move {
        match artist_qt::load_release_page(&runtime, &artist_id, &release_type, offset.max(0) as u32)
            .await
        {
            Ok((cards, has_more)) => {
                let json = serde_json::to_string(&cards).unwrap_or_else(|_| "[]".into());
                artist_bridge::ui(move |mut b| {
                    b.as_mut().release_section_ready(
                        QString::from(release_type.as_str()),
                        QString::from(json.as_str()),
                        has_more,
                    );
                });
            }
            Err(e) => log::warn!("[qbz-qt] release page load failed: {e}"),
        }
    });
}

/// Sidebar "+" — create an empty playlist with the default name, then
/// reload the tree (the Slint flow opens a naming modal — POC-NOTE).
///
/// OFFLINE it creates a LOCAL playlist instead, which is the reference's D8
/// rule collapsed onto this port's no-modal shortcut: `on_create_playlist`
/// (qbz/src/main.rs:21109) opens the create modal with `offline_only` set ON
/// **and LOCKED** while offline, so creation there always produces a local
/// playlist. Without this arm the "+" was a fully lit, pointer-cursor button
/// whose entire offline effect was `log::error!` — the dead-control class the
/// owner's standing rule forbids, on the ONE surface a user with no Qobuz
/// account owns. `offline_only = true` matches the picker's own offline create
/// (`playlist_picker_qt.rs:555-559`): the playlist is never offered for upload
/// and never reaches a QConnect push.
pub(crate) fn create_playlist() {
    if offline_fwd::engine().status().is_offline() {
        // The SAME default name the online arm uses — one msgid, one label,
        // whichever door the user came through.
        let name = qbz_i18n::t("New Playlist");
        spawn(async move {
            let created = tokio::task::spawn_blocking(move || {
                local_playlist_qt::create_blocking(&name, None, true)
            })
            .await
            .ok()
            .flatten();
            match created {
                Some(new_id) => {
                    log::info!("[qbz-qt] local playlist created offline: {new_id}");
                    // The offline-safe verb — `reload_sidebar` early-returns
                    // here, which is exactly this branch.
                    reload_sidebar_including_local();
                    // Land on it, like the online arm does. `open_playlist`
                    // routes a `local:` id to the local loader and does NOT
                    // offline-gate it.
                    open_playlist(new_id);
                }
                None => {
                    log::error!("[qbz-qt] offline local playlist create failed");
                    toast_qt::error(qbz_i18n::t("Couldn't create the playlist"));
                }
            }
        });
        return;
    }
    let runtime = app();
    spawn(async move {
        match runtime
            .core()
            .create_playlist(&qbz_i18n::t("New Playlist"), None, false)
            .await
        {
            Ok(p) => {
                log::info!("[qbz-qt] playlist created: {} ({})", p.name, p.id);
                sidebar_qt::load(&runtime).await;
                publish_sidebar();
                // Open the new playlist (the Slint lands on it after the
                // naming modal); the user-playlists endpoint lags the
                // write, so the tree may not show it yet — the detail view
                // fetches by id and is correct regardless.
                open_playlist(p.id.to_string());
            }
            Err(e) => log::error!("[qbz-qt] create playlist failed: {e}"),
        }
    });
}

/// AlbumCard ⋯ menu: Play next / Add to queue.
pub(crate) fn enqueue_album(album_id: String, mode: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::enqueue_album(&runtime, &album_id, &mode).await {
            log::error!("[qbz-qt] enqueue_album failed: {e}");
        }
    });
}

/// Card / detail-header pin badge: mutate the store, then fan the result out
/// — the ADR-006 per-user stores have no change-notify, so the mutation site
/// owns the fan-out (Slint's `on_toggle_pin` does the same thing: badge walk
/// over the live models, then `pinned_section::rebuild_pinned`).
///
/// FIVE consumers, and only ONE of them re-renders anything:
///  - `pin-changed` (`{kind}:{id}`) — the badge walk. Every AlbumCard /
///    ArtistCard / PlaylistCard on screen listens for its own key and flips
///    its glyph in place, and the Library All feed patches its own row. No
///    model is replaced, so no delegate is torn down.
///  - `home_qt::apply_pin_change` — patches the cached candidate rows (for
///    the NEXT republish, whatever causes it) and rebuilds the Pinned rail
///    on its own `pinnedJson` property. The three tab documents are left
///    alone.
///  - `recommendations_qt::apply_pin_change` — same shape for that tab's
///    separate cache; badge-only (it has no pinned rail).
///  - `search_qt::apply_pin_change` — same again. Search NEEDS it because its
///    cached page is re-published routinely (the artwork pass after `submit`,
///    plus `tab_changed` / `load_more` / `filter_changed`), and each of those
///    swaps the model out from under the card: without the patch the stale
///    `isPinned` in the cache silently reverted the user's flip a second after
///    the click, whenever a cover happened to land.
///  - `library_qt::apply_pin_change` — patches the cached merged feed. Only
///    re-serialized by a full `reload_library` today, so it is the cheap
///    insurance rather than a live bug.
///
/// EVERY consumer here is a cache patch or an in-place badge signal. The rule
/// this encodes: a per-click mutation must never republish a document. See
/// `home_qt::apply_pin_change`'s docs for what republishing cost.
pub(crate) fn toggle_pin(kind: String, id: String, title: String, subtitle: String, artwork_url: String) {
    if let Some(value) = sidebar_qt::toggle_pin(&kind, &id, &title, &subtitle, &artwork_url) {
        let key = format!("{kind}:{id}");
        library_bridge::ui(move |mut b| {
            b.as_mut().pin_changed(QString::from(key.as_str()), value);
        });
        home_qt::apply_pin_change(&kind, &id, value);
        recommendations_qt::apply_pin_change(&kind, &id, value);
        search_qt::apply_pin_change(&kind, &id, value);
        library_qt::apply_pin_change(&kind, &id, value);
    }
}

// ============================ Lyrics panel (phase 9) ======================

/// NPB lyrics button / panel close: toggle the column section and — like
/// LyricsState.panel-opened — re-request the current track's lyrics on
/// open (cached docs return immediately). The static is the source of
/// truth (the ui() hop is queued, not synchronous).
static LYRICS_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn toggle_lyrics() {
    let open = !LYRICS_OPEN.swap(!LYRICS_OPEN.load(std::sync::atomic::Ordering::SeqCst), std::sync::atomic::Ordering::SeqCst);
    shell_bridge::ui(move |mut b| {
        b.as_mut().set_lyrics_open(open);
    });
    if open {
        let runtime = app();
        spawn(async move {
            if let Some(track) = runtime.core().current_track().await {
                lyrics_qt::load_for_track(&runtime, &track).await;
            }
        });
    }
}

// ============================ Queue panel (phase 9) =======================

pub(crate) fn queue_set_page(page: i32) {
    queue_qt::set_page(page);
    let runtime = app();
    spawn(async move { queue_qt::publish(&runtime).await });
}

pub(crate) fn queue_set_search(query: String) {
    queue_qt::set_search(&query);
    let runtime = app();
    spawn(async move { queue_qt::publish(&runtime).await });
}

pub(crate) fn queue_play_upcoming(index: i32) {
    let runtime = app();
    spawn(async move { queue_qt::play_upcoming(&runtime, index.max(0) as usize).await });
}

pub(crate) fn queue_remove_upcoming(index: i32) {
    let runtime = app();
    spawn(async move { queue_qt::remove_upcoming(&runtime, index.max(0) as usize).await });
}

pub(crate) fn queue_remove_all_after(index: i32) {
    let runtime = app();
    spawn(async move { queue_qt::remove_all_after(&runtime, index.max(0) as usize).await });
}

pub(crate) fn queue_move_track(from: i32, to: i32) {
    let runtime = app();
    spawn(async move { queue_qt::move_track(&runtime, from.max(0) as usize, to.max(0) as usize).await });
}

pub(crate) fn queue_play_history(index: i32) {
    let runtime = app();
    spawn(async move { queue_qt::play_history(&runtime, index.max(0) as usize).await });
}

pub(crate) fn queue_clear() {
    let runtime = app();
    spawn(async move { queue_qt::clear_queue(&runtime).await });
}

pub(crate) fn queue_toggle_favorite(kind: String, id: String) {
    let runtime = app();
    spawn(async move { queue_qt::toggle_favorite(&runtime, &kind, &id).await });
}

pub(crate) fn queue_toggle_stop_after(id: String) {
    let runtime = app();
    spawn(async move { queue_qt::toggle_stop_after(&runtime, &id).await });
}

pub(crate) fn queue_toggle_infinite_play() {
    let runtime = app();
    spawn(async move { queue_qt::toggle_infinite_play(&runtime).await });
}

pub(crate) fn queue_save_as_playlist() {
    let runtime = app();
    spawn(async move { queue_qt::save_as_playlist(&runtime).await });
}

pub(crate) fn queue_add_to_playlist(index: i32) {
    let runtime = app();
    spawn(async move { queue_qt::add_to_playlist(&runtime, index.max(0) as usize).await });
}

pub(crate) fn queue_panel_opened() {
    let runtime = app();
    spawn(async move { queue_qt::panel_opened(&runtime).await });
}

/// AlbumView header Shuffle.
pub(crate) fn play_album_shuffled(album_id: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::play_album_shuffled(&runtime, &album_id).await {
            log::error!("[qbz-qt] play_album_shuffled failed: {e}");
        }
    });
}

/// AlbumView row play (album from the clicked track).
pub(crate) fn play_album_from_track(album_id: String, track_id: u64) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::play_album_from_track(&runtime, &album_id, track_id).await {
            log::error!("[qbz-qt] play_album_from_track failed: {e}");
        }
    });
}

/// AlbumView row "Play next" / "Add to queue".
pub(crate) fn enqueue_album_track(album_id: String, track_id: u64, mode: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::enqueue_album_track(&runtime, &album_id, track_id, &mode).await {
            log::error!("[qbz-qt] enqueue_album_track failed: {e}");
        }
    });
}

/// ArtistView Popular Tracks row play (the whole list as the queue).
pub(crate) fn play_artist_track(track_id: u64) {
    let runtime = app();
    spawn(async move {
        let (queue, start) = artist_qt::top_queue(Some(track_id));
        if let Err(e) = playback_qt::play_track_list(&runtime, queue, start, false).await {
            log::error!("[qbz-qt] play_artist_track failed: {e}");
        }
    });
}

/// ArtistView "Play all" / "Shuffle all".
pub(crate) fn play_artist_top(shuffle: bool) {
    let runtime = app();
    spawn(async move {
        let (queue, start) = artist_qt::top_queue(None);
        if let Err(e) = playback_qt::play_track_list(&runtime, queue, start, shuffle).await {
            log::error!("[qbz-qt] play_artist_top failed: {e}");
        }
    });
}

/// ArtistView ⋯ "Add all to queue".
pub(crate) fn enqueue_artist_top() {
    let runtime = app();
    spawn(async move {
        let (queue, _) = artist_qt::top_queue(None);
        if let Err(e) = playback_qt::enqueue_track_list(&runtime, queue).await {
            log::error!("[qbz-qt] enqueue_artist_top failed: {e}");
        }
    });
}

/// Track-row click (Library tracks): one-track queue through the core.
pub(crate) fn play_track(track_id: u64) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::play_single_track(&runtime, track_id).await {
            log::error!("[qbz-qt] play_track failed: {e}");
        }
    });
}

// ============================ Playlist view + DnD (phase 17) ==============

/// Open the playlist detail view (sidebar row / playlist card click).
pub(crate) fn open_playlist(playlist_id: String) {
    // LOCAL playlists (`local:<uuid>`) route to their own loader and open
    // REGARDLESS of connectivity. They are the whole feature for people using
    // QBZ as a player without Qobuz — refusing them while offline would gate
    // local files behind a network the user does not have.
    if local_playlist_qt::is_local_id(&playlist_id) {
        nav_qt::record("playlist");
        ui(|mut b| b.as_mut().set_playlist_json(QString::from("{}")));
        let runtime = app();
        spawn(async move {
            if !local_playlist_qt::load(&runtime, &playlist_id).await {
                log::warn!("[qbz-qt] local playlist {playlist_id} load failed");
            }
        });
        return;
    }
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    let Some(pid) = playlist_id.parse::<u64>().ok() else {
        log::warn!("[qbz-qt] open_playlist: invalid id {playlist_id}");
        return;
    };
    nav_qt::record("playlist");
    // Clear the previous playlist before the fetch — same stale-render as the
    // album and artist views had.
    ui(|mut b| b.as_mut().set_playlist_json(QString::from("{}")));
    // A Qobuz detail must not inherit the previous LOCAL detail's snapshot, or
    // its rows would resolve against the wrong playlist's queue.
    local_playlist_qt::clear_open_snapshot();
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::load(&runtime, pid).await {
            log::warn!("[qbz-qt] playlist load failed: {e}");
        }
    });
}

// ======================= MyQBZ (crate-level forwards) =====================
//
// W6: the ONLY crate-root forward the MyQBZ domain needs. Everything else the
// three bridges call, they call straight at its controller module
// (`crate::myqbz_qt::…`, `crate::myqbz_builder_qt::…`) — a thin forward per
// invokable would be 48 dead one-liners.
//
// This one exists because the CONTROLLERS need it, not the bridges: both
// `myqbz_qt::create_submit` (Create modal) and `myqbz_builder_qt::create`
// (Artist-Collection builder) navigate to the collection they just created,
// and neither should reach into a sibling controller to do it.

/// Open a mixtape/collection detail page by id — the shared "created, now go
/// there" hop. `myqbz_detail_qt::open` records the `"mixtapedetail"` route
/// itself, so this deliberately does NOT call `navigate_to` (which would
/// record a second entry and clear `LAST_DETAIL` twice).
pub(crate) fn myqbz_open_detail(id: String) {
    myqbz_detail_qt::open(id);
}

pub(crate) fn playlist_play_all() {
    let runtime = app();
    spawn(async move {
        // A LOCAL detail plays from its OWN resolved snapshot: its rows are a
        // mix of catalog, file and Plex tracks that the Qobuz play-all path
        // cannot build, and an offline-only one has to stamp the queue.
        if local_playlist_qt::open_id().is_some() {
            local_playlist_qt::play(&runtime, "").await;
            return;
        }
        if let Err(e) = playlist_qt::play_all(&runtime).await {
            log::error!("[qbz-qt] playlist play-all failed: {e}");
        }
    });
}

pub(crate) fn playlist_shuffle() {
    let runtime = app();
    spawn(async move {
        // The local guard `playlist_play_all` carries. The MODE is raised so
        // what follows this list stays shuffled, but the list itself is mixed
        // by `play_shuffled` — the flag alone would start on the playlist's #1
        // track every time (owner ruling 2026-08-01: every shuffle must be
        // genuinely random).
        if local_playlist_qt::open_id().is_some() {
            runtime.core().set_shuffle(true).await;
            now_playing::set_shuffle(true);
            local_playlist_qt::play_shuffled(&runtime).await;
            return;
        }
        if let Err(e) = playlist_qt::play_shuffled(&runtime).await {
            log::error!("[qbz-qt] playlist shuffle failed: {e}");
        }
    });
}

pub(crate) fn playlist_toggle_follow() {
    let runtime = app();
    spawn(async move { playlist_qt::toggle_follow(&runtime).await });
}

pub(crate) fn playlist_copy() {
    let runtime = app();
    spawn(async move { playlist_qt::copy_playlist(&runtime).await });
}

pub(crate) fn playlist_rename(name: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::rename(&runtime, &name).await {
            log::error!("[qbz-qt] playlist rename failed: {e}");
        }
    });
}

pub(crate) fn playlist_delete() {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::delete_playlist(&runtime).await {
            log::error!("[qbz-qt] playlist delete failed: {e}");
        }
    });
}

pub(crate) fn playlist_play_track(track_id: String) {
    let runtime = app();
    spawn(async move {
        // Same guard as `playlist_play_all` above, and for the same reason: a
        // LOCAL detail plays from its OWN resolved snapshot. Without it a row
        // click went through `playlist_qt::play_track` -> `current_queue()` ->
        // `row_to_queue`, which types EVERY row as Qobuz (`is_local: false`,
        // `source: "qobuz"`) — so a local file was queued as a catalog track
        // and `governed` came out true on the quality seed. `local_playlist_qt
        // ::play` matches on the same display id the rows carry
        // (`row_to_display` sets `id: queue.id.to_string()`), Qobuz rows
        // included, so the whole mixed detail is served by one path.
        if local_playlist_qt::open_id().is_some() {
            local_playlist_qt::play(&runtime, &track_id).await;
            return;
        }
        if let Err(e) = playlist_qt::play_track(&runtime, &track_id).await {
            log::error!("[qbz-qt] playlist row play failed: {e}");
        }
    });
}

pub(crate) fn playlist_enqueue_track(track_id: String, mode: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::enqueue_track(&runtime, &track_id, &mode).await {
            log::error!("[qbz-qt] playlist row enqueue ({mode}) failed: {e}");
        }
    });
}

/// Row ⋯ "Remove from playlist", BY DISPLAY ROW ID.
///
/// The id is a string because a LOCAL playlist's rows are not all catalog
/// tracks: a local-file row's id is a library row id, an unresolved one is
/// `"plex:<key>"` or a bare path. The Qobuz arm parses it back to the
/// membership row id, which for a Qobuz playlist IS the catalog id
/// (`playlist_qt.rs:364` sets `playlist_track_id: track.id`).
///
/// Routing by OPEN DETAIL, not by the id's shape: `local_playlist_qt::open_id`
/// is `Some` only while a `local:` detail is the open page (`open_playlist`
/// clears the snapshot before a Qobuz load), and it is the same test
/// `playlist_play_all` already routes on.
pub(crate) fn playlist_remove_track(row_id: String) {
    let runtime = app();
    spawn(async move {
        if local_playlist_qt::open_id().is_some() {
            local_playlist_qt::remove_row(&runtime, &row_id).await;
            return;
        }
        let Ok(playlist_track_id) = row_id.parse::<u64>() else {
            log::warn!("[qbz-qt] playlist remove: non-numeric row id {row_id}");
            return;
        };
        playlist_qt::remove_track(&runtime, playlist_track_id).await
    });
}

/// Drag-reorder drop: move the visible row `from` to insertion slot `slot`.
///
/// Routes like the chevrons below: a LOCAL playlist writes the repo `position`
/// order directly (no sidecar — the repo order IS the order), a Qobuz one
/// rebuilds the custom-order sidecar. Same split as the reference
/// (`main.rs:20815` `on_reorder_track`).
pub(crate) fn playlist_reorder(from: i32, slot: i32) {
    if from < 0 || slot < 0 || slot == from || slot == from + 1 {
        return;
    }
    if local_playlist_qt::open_id().is_some() {
        let runtime = app();
        spawn(async move {
            local_playlist_qt::reorder_row(&runtime, from as usize, slot as usize).await;
        });
        return;
    }
    playlist_qt::reorder_track(from as usize, slot as usize);
}

/// Per-row reorder chevrons: -1 = up, +1 = down. Same routing as the drag.
pub(crate) fn playlist_move_row(row_id: String, delta: i32) {
    if delta == 0 {
        return;
    }
    if local_playlist_qt::open_id().is_some() {
        let runtime = app();
        spawn(async move {
            local_playlist_qt::move_row(&runtime, &row_id, delta).await;
        });
        return;
    }
    playlist_qt::move_row(&row_id, delta);
}

// --- Drag & drop (DragState + DragActions, state.slint parity) -----------
//
// The dragged payload: Qobuz catalog ids only for the POC (the Slint's
// DragTrack enum also carries local-library rows and Plex keys into
// sidecar writes — out of scope, see module POC-NOTEs).
static DRAGGED: Mutex<Vec<u64>> = Mutex::new(Vec::new());
/// The claimed drop target (sidebar playlist id) — mirrored Rust-side so
/// drag_end never reads the bridge off-thread.
static DRAG_OVER: Mutex<String> = Mutex::new(String::new());

pub(crate) fn drag_start(track_id: String, title: String, subtitle: String, x: f32, y: f32) {
    let id = track_id.parse::<u64>().unwrap_or(0);
    *DRAGGED.lock().unwrap() = if id > 0 { vec![id] } else { Vec::new() };
    log::info!("[qbz-qt][drag] start {track_id} ({title})");
    shell_bridge::ui(move |mut b| {
        b.as_mut().set_drag_count(if id > 0 { 1 } else { 0 });
        b.as_mut().set_drag_title(QString::from(title.as_str()));
        b.as_mut().set_drag_subtitle(QString::from(subtitle.as_str()));
        b.as_mut().set_drag_x(x);
        b.as_mut().set_drag_y(y);
        b.as_mut().set_drag_over_playlist_id(QString::default());
        b.as_mut().set_drag_active(true);
    });
}

pub(crate) fn drag_move(x: f32, y: f32) {
    shell_bridge::ui(move |mut b| {
        b.as_mut().set_drag_x(x);
        b.as_mut().set_drag_y(y);
    });
}

pub(crate) fn drag_set_over(playlist_id: String) {
    *DRAG_OVER.lock().unwrap() = playlist_id.clone();
    shell_bridge::ui(move |mut b| {
        b.as_mut()
            .set_drag_over_playlist_id(QString::from(playlist_id.as_str()));
    });
}

pub(crate) fn drag_end() {
    let pid = std::mem::take(&mut *DRAG_OVER.lock().unwrap());
    shell_bridge::ui(|mut b| {
        b.as_mut().set_drag_active(false);
        b.as_mut().set_drag_over_playlist_id(QString::default());
    });
    let tracks = std::mem::take(&mut *DRAGGED.lock().unwrap());
    if tracks.is_empty() {
        return;
    }
    let Ok(pid) = pid.parse::<u64>() else {
        // Not a Qobuz playlist target (a local-library playlist row or
        // nothing) — the sidecar path is out of scope (POC-NOTE).
        return;
    };
    let runtime = app();
    spawn(async move { playlist_qt::add_tracks(&runtime, pid, &tracks).await });
}

/// Card overlay Play for a playlist (fetch + play from the top).
pub(crate) fn play_playlist_by_id(playlist_id: u64) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::play_playlist_by_id(&runtime, playlist_id).await {
            log::error!("[qbz-qt] play playlist {playlist_id} failed: {e}");
        }
    });
}

/// Card menu queueing for a playlist.
pub(crate) fn enqueue_playlist_by_id(playlist_id: u64, mode: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::enqueue_playlist_by_id(&runtime, playlist_id, &mode).await {
            log::error!("[qbz-qt] enqueue playlist {playlist_id} ({mode}) failed: {e}");
        }
    });
}

/// Card overlay follow/unfollow for a playlist.
pub(crate) fn playlist_set_follow_by_id(playlist_id: u64, follow: bool) {
    let runtime = app();
    spawn(async move { playlist_qt::set_follow_by_id(&runtime, playlist_id, follow).await });
}

/// Now-Playing-view flyout: switch + persist the bar mode. Large forces
/// the sidebar open (the Slint "large" arm — the dock needs it).
pub(crate) fn npb_set_mode(mode: i32) {
    let mode = settings_qt::set_npb_mode(mode);
    log::info!("[qbz-qt] npb_mode -> {mode}");
    shell_bridge::ui(move |mut b| {
        if mode == 3 {
            b.as_mut().set_sidebar_state(0);
        }
        b.as_mut().set_npb_mode(mode);
    });
}

/// Large dock, cover eye button: show/hide the FFT band. Persists the pref,
/// republishes the dock height (the Sidebar's reservation and AppShell's pin
/// both read it), and gates the capture tap — the band being hidden is what
/// stops the FFT producer, so an unused visualizer costs zero CPU.
pub(crate) fn large_toggle_visualizer() {
    let on = !settings_qt::large_visualizer_on();
    settings_qt::set_large_visualizer_on(on);
    let height = shell_bridge::large_dock_height(on);
    log::info!("[qbz-qt] large visualizer -> {on} (dock height {height})");
    viz_qt::set_enabled(on);
    shell_bridge::ui(move |mut b| {
        b.as_mut().set_large_visualizer_on(on);
        b.as_mut().set_large_dock_height(height);
    });
}

/// Large dock, band click: cycle Bars -> Waveform -> Energy.
pub(crate) fn large_cycle_spectrum() {
    let mode = settings_qt::set_large_spectrum_mode((settings_qt::large_spectrum_mode() + 1) % 3);
    log::info!("[qbz-qt] large spectrum mode -> {mode}");
    // Point the drain at the new stream BEFORE the UI switches, so the first
    // frame the new mode renders already has data.
    viz_qt::set_mode(mode);
    shell_bridge::ui(move |mut b| b.as_mut().set_large_spectrum_mode(mode));
}

/// Artist-network row click: resolve a musician NAME to a Qobuz artist and open
/// their page. Only a CONFIRMED match navigates — a guess would drop the user on
/// the wrong artist, which is worse than the row doing nothing.
pub(crate) fn resolve_musician(name: String, role: String) {
    let runtime = app();
    spawn(async move {
        match runtime.core().musicbrainz_resolve_musician(&name, &role).await {
            Ok(r) => match (r.confidence, r.qobuz_artist_id) {
                (qbz_integrations::musicbrainz::MusicianConfidence::Confirmed, Some(id)) => {
                    open_artist(id.to_string());
                }
                _ => log::info!("[qbz-qt] musician '{name}' not confidently resolved; not navigating"),
            },
            Err(e) => log::warn!("[qbz-qt] musician resolve failed for '{name}': {e}"),
        }
    });
}

/// Artist-card overlay play (ArtistGridCard): Popular tracks with the
/// studio-discography fallback.
pub(crate) fn play_artist_card(artist_id: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::play_artist(&runtime, &artist_id).await {
            log::error!("[qbz-qt] play_artist_card {artist_id} failed: {e}");
        }
    });
}

/// Library track menu: Play next / Play later / Add to queue (single feed
/// track into the existing queue).
pub(crate) fn enqueue_track(track_id: u64, mode: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::enqueue_single_track(&runtime, track_id, &mode).await {
            log::error!("[qbz-qt] enqueue_track {track_id} ({mode}) failed: {e}");
        }
    });
}

/// Album-card click on Home: resolve + enqueue + play through the core.
pub(crate) fn play_album(album_id: String) {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playback_qt::play_album(&runtime, &album_id).await {
            log::error!("[qbz-qt] play_album failed: {e}");
        }
    });
}

pub(crate) fn transport_toggle_play() {
    let runtime = app();
    spawn(async move { playback_qt::toggle_play(&runtime).await });
}

pub(crate) fn transport_next() {
    let runtime = app();
    spawn(async move { playback_qt::next(&runtime).await });
}

pub(crate) fn transport_previous() {
    let runtime = app();
    spawn(async move { playback_qt::previous(&runtime).await });
}

pub(crate) fn transport_seek(frac: f32) {
    let runtime = app();
    spawn(async move { playback_qt::seek_frac(&runtime, frac).await });
}

pub(crate) fn transport_set_volume(volume: f32) {
    // Local model first (instant UI), then the engine.
    now_playing::set_volume(volume);
    let runtime = app();
    spawn(async move { playback_qt::set_volume(&runtime, volume).await });
}

pub(crate) fn transport_toggle_mute() {
    let runtime = app();
    spawn(async move { playback_qt::toggle_mute(&runtime).await });
}

pub(crate) fn transport_toggle_shuffle() {
    let runtime = app();
    spawn(async move { playback_qt::toggle_shuffle(&runtime).await });
}

pub(crate) fn transport_cycle_repeat() {
    let runtime = app();
    spawn(async move { playback_qt::cycle_repeat(&runtime).await });
}

// ============================ Library (phase 5) ===========================

/// Whether the Library view has been loaded this session.
static LIBRARY_LOADED: Mutex<bool> = Mutex::new(false);

/// Sidebar navigation: record the view and lazy-load its data.
pub(crate) fn navigate_to(view: &str) {
    nav_qt::record(view);
    LAST_DETAIL.lock().unwrap().0.clear();
    if view == "library" {
        load_library_once();
    }
    if view == "settings" {
        publish_settings();
    }
    // MyQBZ: BOTH grids reload on EVERY visit — deliberately no once-flag
    // (contrast `library` above). A create from the Add picker, a delete from
    // the detail view or a new Artist Collection from the builder all change
    // the set while the grid is unmounted, so a once-per-session flag would
    // paint a stale grid. The read is one local SQLite query.
    if view == "mixtapes" {
        myqbz_qt::load_grid(myqbz_qt::Grid::Mixtapes);
    }
    if view == "collections" {
        myqbz_qt::load_grid(myqbz_qt::Grid::Collections);
    }
    // "mixtapedetail", "discobuilder" and "blacklist" need no arm: each is
    // only ever reached through a call that loads its own data first and
    // records the route itself (myqbz_detail_qt::open, myqbz_builder_qt::open,
    // blacklist_qt::open_manager), never through the sidebar.
}

// ============================ i18n (phase 20) ==============================

/// The active detail view ("album"/"artist" + id) — re-published on a live
/// language switch so its Rust-built section headers re-translate.
static LAST_DETAIL: Mutex<(String, String)> = Mutex::new((String::new(), String::new()));

/// Settings > Appearance > Language: the pref is already persisted by the
/// settings arm; this applies it LIVE — "auto" resolves POSIX env — then:
///  1. bumps trRev, so EVERY `QbzBridge.tr(msgid, QbzBridge.trRev)` binding
///     re-evaluates through the new catalog (the Slint reseed equivalent);
///  2. re-publishes the Rust-built translated documents (home section
///     titles, library feed labels, settings option labels, the active
///     album/artist detail headers — tf() plurals included).
pub(crate) fn apply_language(code: String) {
    let resolved = if code == "auto" {
        qbz_i18n::resolve_auto().to_string()
    } else {
        code.clone()
    };
    qbz_i18n::set_language(&resolved);
    log::info!("[qbz-qt] language -> {code} (resolved: {resolved})");
    session_bridge::ui(move |mut b| {
        let next = b.as_ref().tr_rev() + 1;
        b.as_mut().set_tr_rev(next);
    });
    reload_home();
    reload_library();
    publish_settings();
    // MyQBZ builds a handful of strings in RUST — the kind eyebrow
    // ("MIXTAPE" / "COLLECTION" / "ARTIST"), the "{} albums" plural, every
    // unresolved row's TYPE label, and the builder's footer count + default
    // collection name. None of them re-translate through trRev, because they
    // arrive as data inside a JSON document, so each owner republishes.
    //
    // Unconditional, unlike the album/artist republish below: `navigate_to`
    // clears LAST_DETAIL, so routing MyQBZ through that latch would silently
    // skip it. Each of these is a no-op on an empty document.
    myqbz_qt::republish_all();
    myqbz_detail_qt::republish();
    myqbz_builder_qt::republish();
    let (view, id) = LAST_DETAIL.lock().unwrap().clone();
    if !id.is_empty() {
        match view.as_str() {
            "album" => republish_album(id),
            "artist" => republish_artist(id),
            _ => {}
        }
    }
}

/// Re-fetch + publish the album/artist detail doc WITHOUT touching nav
/// (the open_* entry points record a nav entry).
fn republish_album(album_id: String) {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    let runtime = app();
    spawn(async move {
        if let Ok(json) = album_qt::load_album_view(&runtime, &album_id).await {
            album_bridge::ui(move |mut b| b.as_mut().set_album_json(QString::from(json.as_str())));
        }
    });
}

fn republish_artist(artist_id: String) {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    let runtime = app();
    spawn(async move {
        if let Ok(json) = artist_qt::load_artist_view(&runtime, &artist_id).await {
            artist_bridge::ui(move |mut b| b.as_mut().set_artist_json(QString::from(json.as_str())));
        }
    });
}

// ============================ Search (phase 15) ===========================

pub(crate) fn search_live(query: String) {
    if offline_fwd::engine().status().is_offline() {
        // Qobuz-only surface (the local cortinilla sections are not
        // ported — search_qt.rs POC-NOTE): nothing to show offline.
        return;
    }
    let runtime = app();
    spawn(async move { search_qt::live(&runtime, &query).await });
}

pub(crate) fn search_submit(query: String) {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    let runtime = app();
    spawn(async move { search_qt::submit(&runtime, &query, None).await });
}

pub(crate) fn cortinilla_view_more(kind: String) {
    let runtime = app();
    spawn(async move { search_qt::view_more(&runtime, &kind).await });
}

pub(crate) fn cortinilla_search_all() {
    let runtime = app();
    spawn(async move { search_qt::search_all_action(&runtime).await });
}

pub(crate) fn search_load_more(tab: i32) {
    let runtime = app();
    spawn(async move { search_qt::load_more(&runtime, tab).await });
}

pub(crate) fn search_filter_changed(index: i32) {
    let runtime = app();
    spawn(async move { search_qt::filter_changed(&runtime, index).await });
}

/// App-menu intelligent-search toggle: persist the pref + flip the live
/// kill switch + update the menu state. Live, no restart.
pub(crate) fn toggle_intelligent_search() {
    let next = search_qt::toggle_intelligent_search();
    log::info!("[qbz-qt] intelligent_search -> {next}");
    ui(move |mut b| b.as_mut().set_intelligent_search(next));
}

// ============================ Settings (phase 10) =========================

/// Publish the settings snapshot (settings_qt.rs SettingsDoc) onto
/// `settingsJson`. Called on settings-view open and after every mutation
/// (the handlers republish themselves).
pub(crate) fn publish_settings() {
    spawn(async { settings_qt::publish_snapshot().await });
}

pub(crate) fn settings_bool(key: String, value: bool) {
    let runtime = app();
    spawn(async move { settings_qt::settings_bool(&runtime, &key, value).await });
}

pub(crate) fn settings_select(key: String, index: i32) {
    let runtime = app();
    spawn(async move {
        settings_qt::settings_select(&runtime, &key, index.max(0) as usize).await
    });
}

pub(crate) fn settings_slider(key: String, value: i32) {
    let runtime = app();
    spawn(async move { settings_qt::settings_slider(&runtime, &key, value).await });
}

pub(crate) fn settings_string(key: String, value: String) {
    spawn(async move { settings_qt::settings_string(&key, value).await });
}

pub(crate) fn settings_reset() {
    let runtime = app();
    spawn(async move { settings_qt::settings_reset(&runtime).await });
}

pub(crate) fn refresh_devices() {
    let runtime = app();
    spawn(async move { settings_qt::refresh_devices(&runtime).await });
}

/// Appearance > Theme row: persist the slug + republish the token document
/// (live switch — QbzTheme.qml rebinds every consumer).
pub(crate) fn theme_set(slug: String) {
    theme_qt::set_theme(&slug);
    theme_qt::publish_theme();
}

/// Appearance > theme filter cycle (0 All / 1 Dark / 2 Light).
pub(crate) fn theme_set_filter(index: i32) {
    let index = index.clamp(0, 2);
    theme_qt::set_theme_filter(index);
    shell_bridge::ui(move |mut b| b.as_mut().set_theme_filter(index));
}

/// Integrations panel non-toggle actions (Last.fm connect flow, LB/LFM
/// disconnects).
pub(crate) fn integrations_action(action: String) {
    let runtime = app();
    spawn(async move { integrations_qt::handle_action(&runtime, &action).await });
}

/// App-menu chrome toggle: persist the flipped pref and update the menu
/// state. The APPLIED mode (`systemTitleBar`) is deliberately untouched —
/// it changes on the next launch (the window flags are read once at
/// startup, 1:1 the Slint restart semantics).
pub(crate) fn toggle_system_title_bar() {
    let next = settings_qt::toggle_system_title_bar();
    log::info!("[qbz-qt] use_system_title_bar -> {next} (applies on next launch)");
    shell_bridge::ui(move |mut b| b.as_mut().set_system_title_bar_pref(next));
}

/// App-menu ambient toggle: persist the flipped pref and apply LIVE (the
/// ambient layer is pure QML — no restart needed).
pub(crate) fn toggle_ambient_background() {
    let mode = settings_qt::toggle_ambient_background();
    log::info!("[qbz-qt] app_background -> {}", if mode == 1 { "ambient" } else { "off" });
    shell_bridge::ui(move |mut b| b.as_mut().set_ambient_mode(mode));
}

fn load_library_once() {
    if *LIBRARY_LOADED.lock().unwrap() {
        return;
    }
    if offline_fwd::engine().status().is_offline() {
        log::info!("[qbz-qt] library load skipped (offline session)");
        return;
    }
    *LIBRARY_LOADED.lock().unwrap() = true;
    reload_library();
}

/// Fetch + publish the whole library (feed + counts). Perf-instrumented
/// (phase-5 deliverable): wall timings land in the log.
pub(crate) fn reload_library() {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    log_rss("library load start");
    library_bridge::ui(|mut b| {
        b.as_mut().set_library_loading(true);
        b.as_mut().set_library_error(QString::from(""));
    });
    let runtime = app();
    spawn(async move {
        let t = std::time::Instant::now();
        match library_qt::load_library(&runtime).await {
            Ok(total) => {
                let t_ser = std::time::Instant::now();
                let (feed_json, counts_json) = library_qt::with_library(|d| {
                    (
                        serde_json::to_string(&d.feed).unwrap_or_else(|_| "[]".into()),
                        serde_json::to_string(&d.counts).unwrap_or_else(|_| "{}".into()),
                    )
                })
                .unwrap_or_else(|| ("[]".into(), "{}".into()));
                log::info!(
                    "[qbz-qt][perf] library serialize: {:?} ({} bytes)",
                    t_ser.elapsed(),
                    feed_json.len(),
                );
                log::info!(
                    "[qbz-qt][perf] library load total: {:?} ({total} items)",
                    t.elapsed(),
                );
                library_bridge::ui(move |mut b| {
                    b.as_mut().set_library_json(QString::from(feed_json.as_str()));
                    b.as_mut()
                        .set_library_counts_json(QString::from(counts_json.as_str()));
                    b.as_mut().set_library_loading(false);
                });
                log_rss("library published");
            }
            Err(e) => {
                log::warn!("[qbz-qt] library load failed: {e}");
                library_bridge::ui(move |mut b| {
                    b.as_mut().set_library_error(QString::from(e.as_str()));
                    b.as_mut().set_library_loading(false);
                });
            }
        }
    });
}

/// Re-publish the Library document from the CACHED feed — no fetch, no db read.
/// The `publish_sidebar()` of this domain.
///
/// Reserved for the mutations that REMOVE or ADD a row, which is the one thing
/// the in-place `libraryFavoriteChanged` / `pinChanged` signals cannot express:
/// they patch a badge on a row that stays, while an unfollow has to make the
/// row leave. Everything else must keep using the signals — see the long note
/// on `library_toggle_favorite` about `QQuickItemView::setModel()`.
///
/// Handing `model:` a new array is exactly what a tab switch already does, and
/// the crash that note describes was reading `.model` BACK off the view inside
/// the teardown; `LibraryView.visibleRows` no longer does that. What the user
/// does pay is the scroll offset, which is why this is not a per-click verb.
///
/// No-op before the Library has ever loaded (`with_library` -> `None`).
pub(crate) fn publish_library_document() {
    let Some((feed_json, counts_json)) = library_qt::with_library(|d| {
        (
            serde_json::to_string(&d.feed).unwrap_or_else(|_| "[]".into()),
            serde_json::to_string(&d.counts).unwrap_or_else(|_| "{}".into()),
        )
    }) else {
        return;
    };
    library_bridge::ui(move |mut b| {
        b.as_mut().set_library_json(QString::from(feed_json.as_str()));
        b.as_mut()
            .set_library_counts_json(QString::from(counts_json.as_str()));
    });
}

/// VmRSS (KiB) from /proc/self/status — phase-5 RSS-delta measurement.
fn log_rss(mark: &str) {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        if let Some(line) = status.lines().find(|l| l.starts_with("VmRSS:")) {
            log::info!("[qbz-qt][perf] RSS @ {mark}: {line}");
        }
    }
}

/// Windowed artwork dispatch for the Library feed: the view reports the
/// mounted window as artKeys; download the missing ones, emitting
/// `libraryArtworkReady` per store (id-keyed — the wrong-cover race fix
/// from the Slint round). Disk hits emit immediately.
pub(crate) fn library_artwork_window(keys_json: String) {
    let keys: Vec<String> = serde_json::from_str(&keys_json).unwrap_or_default();
    log::debug!("[qbz-qt] artwork window: {} keys", keys.len());
    if keys.is_empty() {
        return;
    }
    let Some(pairs) = library_qt::with_library(|d| {
        keys.iter()
            .filter_map(|k| {
                d.feed
                    .iter()
                    .find(|i| &i.art_key == k)
                    .map(|i| (i.art_key.clone(), i.image_url.clone()))
            })
            .filter(|(_, u)| !u.is_empty())
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    let mut missing: Vec<(String, String)> = Vec::new();
    for (key, url) in pairs {
        let path = artwork_qt::cached_path(&url);
        if path.is_empty() {
            missing.push((key, url));
        } else {
            emit_library_artwork(key, path);
        }
    }
    if missing.is_empty() {
        return;
    }
    spawn(async move {
        let urls: Vec<String> = missing.iter().map(|(_, u)| u.clone()).collect();
        artwork_qt::download_missing(urls).await;
        for (key, url) in missing {
            let path = artwork_qt::cached_path(&url);
            if !path.is_empty() {
                emit_library_artwork(key, path);
            }
        }
    });
}

fn emit_library_artwork(key: String, path: String) {
    library_bridge::ui(move |mut b| {
        b.as_mut()
            .library_artwork_ready(QString::from(key.as_str()), QString::from(path.as_str()));
    });
}

/// Card heart: toggle + signal the result (or the unchanged state on
/// failure, so the UI rolls back).
///
/// The rollback half only became real once `library_qt::toggle_favorite`
/// started returning `Some(current)` on a failed write instead of `None` —
/// while it returned None this emitted nothing and every optimistic flip
/// stuck, which is how a 404'd un-favorite still read as un-favorited until
/// the next library reload. NOTE the signal is only USEFUL where somebody
/// connects it. Listeners today: `LibraryView.qml`, `AlbumView.qml` (header
/// heart, `album:{id}`) and `ArtistView.qml` (header follow `artist:{id}` +
/// its Popular Tracks / Appears On rows, `track:{id}`), plus — since the round
/// that made the optimistic flips reconcilable — `cards/AlbumCard`,
/// `cards/ArtistCard`, `cards/PlaylistCard`, `rows/TrackRow`, `TrackCard`,
/// `QueuePanel` and `shell/PlayerBar`. So every heart in the app settles here.
///
/// Do NOT "fix" a stale heart by republishing a document from a toggle. That
/// hands `model:` a new array, and `QQuickItemView::setModel()` resets the
/// scroll offset AND tears down the QQmlDelegateModel — which is this build's
/// only crash signature (a null read at libQt6QmlModels[420c5], reproduced
/// live). The signal exists so a badge can be patched in place instead.
pub(crate) fn library_toggle_favorite(kind: String, id: String) {
    let runtime = app();
    spawn(async move {
        if let Some(value) = library_qt::toggle_favorite(&runtime, &kind, &id).await {
            emit_library_favorite(&kind, &id, value);
        }
    });
}

/// THE settle point for a heart, wherever the click came from: emit the
/// in-place signal, then fan the settled value into the document caches.
///
/// Split out of `library_toggle_favorite` so a domain that owns its own header
/// state (playlist_qt) can settle BOTH its document and the feed row from the
/// one authoritative answer instead of guessing twice — and now so that every
/// mutation site reaches the fan-out through one door.
///
/// The fan-out is the favourite twin of `toggle_pin`'s, and it exists because
/// build-time stamping alone is not enough: the three caches below are
/// SNAPSHOTS that get re-serialized later (Discover's section configurator,
/// search's post-`submit` artwork pass, a reco tab re-entry), and a snapshot
/// re-published after a toggle put the pre-toggle heart back on screen — over
/// an item whose real state had changed, so the next click did the opposite of
/// what the glyph advertised. Each patches its own rows and publishes NOTHING;
/// the cards on screen are already correct from their optimistic flip and the
/// `libraryFavoriteChanged` signal.
///
/// NOT yet covered — the PAGE-lifetime documents, whose republish window is
/// the couple of seconds between a page opening and its artwork/deferred rows
/// landing: `album_qt` (header + track rows), `artist_qt` (header + top
/// tracks), `playlist_qt` (track rows), `label_qt` (header + cards + top
/// tracks), `browse_qt` (cards). They are listed here rather than left
/// unmentioned; the fix is one `apply_favorite_change` per module, identical
/// in shape to the three below.
pub(crate) fn emit_library_favorite(kind: &str, id: &str, value: bool) {
    let key = library_qt::feed_key(kind, id);
    library_bridge::ui(move |mut b| {
        b.as_mut()
            .library_favorite_changed(QString::from(key.as_str()), value);
    });
    home_qt::apply_favorite_change(kind, id, value);
    recommendations_qt::apply_favorite_change(kind, id, value);
    search_qt::apply_favorite_change(kind, id, value);
}

/// `reloadHome()` invokable / auto-load worker: fetch + publish + artwork.
pub(crate) fn reload_home() {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    home_bridge::ui(|mut b| {
        b.as_mut().set_home_loading(true);
        b.as_mut().set_home_error(QString::from(""));
    });
    let runtime = app();
    spawn(async move {
        match home_qt::load_home(&runtime).await {
            Ok(mut sections) => {
                // Artwork: disk hits attach synchronously; misses download
                // in the background and trigger ONE republish (POC-NOTE in
                // artwork_qt.rs — per-row model updates are the follow-up).
                let mut missing = artwork_qt::attach_cached(&mut sections.home);
                missing.extend(artwork_qt::attach_cached(&mut sections.editor));
                missing.extend(artwork_qt::attach_cached(&mut sections.for_you));
                missing.dedup();
                let count: usize = sections.home.iter().map(|s| s.items.len()).sum::<usize>()
                    + sections.editor.iter().map(|s| s.items.len()).sum::<usize>()
                    + sections.for_you.iter().map(|s| s.items.len()).sum::<usize>();

                publish_home_sections(&sections);
                log::info!(
                    "[qbz-qt] home published: {}+{}+{} sections, {} cards, {} artwork misses",
                    sections.home.len(),
                    sections.editor.len(),
                    sections.for_you.len(),
                    count,
                    missing.len(),
                );
                if !missing.is_empty() {
                    spawn(async move {
                        artwork_qt::download_missing(missing).await;
                        let mut sections = sections;
                        let _ = artwork_qt::attach_cached(&mut sections.home);
                        let _ = artwork_qt::attach_cached(&mut sections.editor);
                        let _ = artwork_qt::attach_cached(&mut sections.for_you);
                        publish_home_sections(&sections);
                        log::info!("[qbz-qt] home republished after artwork downloads");
                    });
                }

                home_bridge::ui(|mut b| b.as_mut().set_home_loading(false));
            }
            Err(e) => {
                log::warn!("[qbz-qt] home load failed: {e}");
                home_bridge::ui(move |mut b| {
                    b.as_mut().set_home_error(QString::from(e.as_str()));
                    b.as_mut().set_home_loading(false);
                });
            }
        }
    });
}

fn publish_home_sections(sections: &home_qt::DiscoverSections) {
    let home_json = serde_json::to_string(&sections.home).unwrap_or_else(|_| "[]".to_string());
    let editor_json = serde_json::to_string(&sections.editor).unwrap_or_else(|_| "[]".to_string());
    let for_you_json = serde_json::to_string(&sections.for_you).unwrap_or_else(|_| "[]".to_string());
    home_bridge::ui(move |mut b| {
        b.as_mut().set_home_sections_json(QString::from(home_json.as_str()));
        b.as_mut()
            .set_editor_sections_json(QString::from(editor_json.as_str()));
        b.as_mut()
            .set_for_you_sections_json(QString::from(for_you_json.as_str()));
    });
}


/// The Settings > Appearance RENDERER row, consumed at startup (PARITY-DEBT
/// #104 — the row rendered and persisted into the SAME ui_prefs the Slint
/// reads, but nothing consumed it, so it lied).
///
/// Precedence (Slint `requested_renderer_tier`, main.rs:7825-7848): explicit
/// Qt envs (`QSG_RHI_BACKEND` / `QT_QUICK_BACKEND`) > `QBZ_RENDERER` > the
/// persisted pref > Qt's default backend (Metal on macOS, OpenGL on Linux —
/// both measured healthy, 2026-08-01/02). Cross-frontend mapping: the GPU
/// tiers (`auto`/`wgpu`/`gpu`/`hardware`/`hw`) ARE Qt's default on both
/// platforms; `gl` maps to `QSG_RHI_BACKEND=opengl` on Linux and to the
/// Metal default on macOS (the Slint remaps GL to skia(Metal) there,
/// main.rs:7651-7653); `software` maps to `QT_QUICK_BACKEND=software`.
///
/// OWED and kept in PARITY-DEBT #104: the crash auto-revert sentinel and
/// the frame-liveness watchdog — forcing a renderer without them can leave
/// a broken startup, exactly what the Slint sentinel was built for.
fn apply_renderer_preference() {
    if std::env::var_os("QSG_RHI_BACKEND").is_some()
        || std::env::var_os("QT_QUICK_BACKEND").is_some()
    {
        log::info!("[renderer] explicit Qt backend env present; leaving the choice to it");
        return;
    }
    let choice = match std::env::var("QBZ_RENDERER")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty() && s != "auto")
    {
        Some(v) => v,
        None => settings_qt::pref_str("renderer", "auto"),
    };
    match choice.as_str() {
        "software" | "cpu" | "soft" => {
            std::env::set_var("QT_QUICK_BACKEND", "software");
            log::info!("[renderer] '{choice}' -> QT_QUICK_BACKEND=software");
        }
        "gl" | "gles" | "femtovg" => {
            if cfg!(target_os = "macos") {
                log::info!("[renderer] '{choice}' on macOS -> Metal default (the Slint GL remap)");
            } else {
                std::env::set_var("QSG_RHI_BACKEND", "opengl");
                log::info!("[renderer] '{choice}' -> QSG_RHI_BACKEND=opengl");
            }
        }
        "auto" | "wgpu" | "gpu" | "hardware" | "hw" => {
            log::info!("[renderer] '{choice}' -> Qt default backend (GPU path)");
        }
        other => log::warn!("[renderer] unrecognized choice '{other}' -> Qt default backend"),
    }
}

fn main() {
    qbz_log::install("info");
    // i18n (phase 20): honor the persisted ui_prefs language; "auto"/missing
    // resolves POSIX env (qbz_i18n::resolve_auto).
    let boot_lang = settings_qt::pref_str("language", "auto");
    qbz_i18n::set_language(if boot_lang == "auto" {
        qbz_i18n::resolve_auto()
    } else {
        boot_lang.as_str()
    });
    // rustls process-level CryptoProvider (aws-lc-rs) — required before any
    // reqwest call, same as the Slint and daemon binaries.
    qbz_app::ensure_crypto_provider();

    let tokio_runtime = Runtime::new().expect("failed to build the tokio runtime");
    let _ = TOKIO.set(tokio_runtime);

    // `with_visualizer` == `new` plus a VisualizerTap wired into the player.
    // The tap starts DISABLED (it captures nothing and the FFT producer idles),
    // so this costs nothing until the Large dock's band is shown. It is a
    // read-only copy downstream of the bit-perfect stream — no device/stream
    // init is touched.
    let runtime = Arc::new(AppRuntime::with_visualizer(LoggingAdapter::new(
        "[qbz-qt]",
    )));
    if let Some(tap) = runtime.visualizer_tap().cloned() {
        viz_qt::install(tap);
        viz_qt::set_mode(settings_qt::large_spectrum_mode());
    } else {
        log::warn!("[qbz-qt] runtime has no visualizer tap; the Large dock band stays empty");
    }
    let _ = APP.set(runtime);

    apply_renderer_preference();

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/com/blitzfc/qbz/qml/Main.qml",
        ));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
        // Same reason as logout: leaving the app must stop the renderer.
        if let Some(rt) = TOKIO.get() {
            rt.block_on(async { cast_qt::service().shutdown().await });
        }
    }
}
