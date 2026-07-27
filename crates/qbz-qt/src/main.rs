//! qbz-qt — Qt/QML frontend POC entry point.
//!
//! Phase 1: boot flow (splash -> silent session restore -> shell
//! placeholder | login screen) with all the offline guardrails of the
//! Slint app. `main` is NOT async: `QGuiApplication::exec()` owns the main
//! thread, so a process-global multi-thread tokio runtime carries all
//! async work; results hop back to Qt through the bridge's `CxxQtThread`.

mod auth_qt;
mod artwork_qt;
mod bridge;
mod home_qt;
mod nav_qt;
mod now_playing;
mod offline_fwd;

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
    // Offline guardrails FIRST (before the login screen can show): engine +
    // connectivity actor, then the live status forwarder. Both need the
    // tokio runtime context for their spawns.
    spawn(async {
        offline_fwd::start();
    });
    offline_fwd::start_ui_forwarder();

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
            Ok(None) => ui(|mut b| b.as_mut().set_screen(QString::from("login"))),
            Err(e) => ui(move |mut b| {
                                b.as_mut().set_restore_error(QString::from(e.as_str()));
                b.as_mut().set_screen(QString::from("login"));
            }),
        }
    });
}

/// Post-login UI state: session header, refresh has-previous-session,
/// clear login errors/phase, switch to the shell.
fn enter_shell(session: auth_qt::SessionInfo) {
    // Phase 2: the shell mounts on the (only) "home" view; seed the nav
    // history and push the current now-playing model onto the bar.
    nav_qt::record("home");
    now_playing::publish_current();
    // Phase 3: fetch Discover > Home (online sessions only — the offline
    // engine gates Qobuz calls anyway, and the view shows the offline
    // placeholder instead).
    load_home_once();
    ui(move |mut b| {
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
    ui(|mut b| {
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
            ui(move |mut b| b.as_mut().set_login_phase(value));
        })
        .await;
        LOGIN_TASK.lock().unwrap().take();
        match result {
            Ok(session) => enter_shell(session),
            Err(e) => ui(move |mut b| {
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
    ui(|mut b| b.as_mut().set_login_phase(0));
}

/// "Start offline": unauthenticated offline session -> shell placeholder.
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
                ui(move |mut b| {
                    b.as_mut().set_session_user_name(QString::from(name.as_str()));
                    b.as_mut().set_session_subscription(QString::from(""));
                    b.as_mut().set_login_error(QString::from(""));
                    b.as_mut().set_restore_error(QString::from(""));
                    b.as_mut().set_login_phase(0);
                    b.as_mut().set_screen(QString::from("shell"));
                });
                nav_qt::record("home");
                now_playing::publish_current();
            }
            Err(e) => {
                log::error!("[qbz-qt] failed to enter offline mode: {e}");
                ui(move |mut b| {
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
        if let Err(e) = auth_qt::logout(&runtime).await {
            log::error!("[qbz-qt] logout failed: {e}");
        }
        // A later login must re-fetch Home (new user, fresh rails).
        *HOME_LOADED.lock().unwrap() = false;
        ui(|mut b| {
            b.as_mut().set_session_user_name(QString::from(""));
            b.as_mut().set_session_subscription(QString::from(""));
            b.as_mut().set_login_error(QString::from(""));
            b.as_mut().set_restore_error(QString::from(""));
            b.as_mut().set_login_phase(0);
            b.as_mut().set_home_sections_json(QString::from("[]"));
            b.as_mut().set_home_error(QString::from(""));
            b.as_mut().set_home_loading(false);
            b.as_mut().set_screen(QString::from("login"));
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

/// `reloadHome()` invokable / auto-load worker: fetch + publish + artwork.
pub(crate) fn reload_home() {
    if offline_fwd::engine().status().is_offline() {
        return;
    }
    ui(|mut b| {
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
                let missing = artwork_qt::attach_cached(&mut sections);
                let count: usize = sections.iter().map(|s| s.items.len()).sum();
                publish_home_sections(&sections);
                log::info!(
                    "[qbz-qt] home published: {} sections, {} cards, {} artwork misses",
                    sections.len(),
                    count,
                    missing.len(),
                );
                if !missing.is_empty() {
                    spawn(async move {
                        artwork_qt::download_missing(missing).await;
                        let mut sections = sections;
                        let _ = artwork_qt::attach_cached(&mut sections);
                        publish_home_sections(&sections);
                        log::info!("[qbz-qt] home republished after artwork downloads");
                    });
                }
                ui(|mut b| b.as_mut().set_home_loading(false));
            }
            Err(e) => {
                log::warn!("[qbz-qt] home load failed: {e}");
                ui(move |mut b| {
                    b.as_mut().set_home_error(QString::from(e.as_str()));
                    b.as_mut().set_home_loading(false);
                });
            }
        }
    });
}

fn publish_home_sections(sections: &[home_qt::HomeSection]) {
    let json = serde_json::to_string(sections).unwrap_or_else(|_| "[]".to_string());
    ui(move |mut b| {
        b.as_mut().set_home_sections_json(QString::from(json.as_str()));
    });
}

fn main() {
    qbz_log::install("info");
    qbz_i18n::set_language(qbz_i18n::resolve_auto());
    // rustls process-level CryptoProvider (aws-lc-rs) — required before any
    // reqwest call, same as the Slint and daemon binaries.
    qbz_app::ensure_crypto_provider();

    let tokio_runtime = Runtime::new().expect("failed to build the tokio runtime");
    let _ = TOKIO.set(tokio_runtime);

    let runtime = Arc::new(AppRuntime::new(LoggingAdapter::new("[qbz-qt]")));
    let _ = APP.set(runtime);

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/com/blitzfc/qbz/qml/Main.qml",
        ));
    }

    if let Some(app) = app.as_mut() {
        app.exec();
    }
}
