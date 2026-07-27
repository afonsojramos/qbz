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
mod library_db_qt;
mod library_qt;
mod nav_qt;
mod now_playing;
mod offline_fwd;
mod playback_qt;
mod sidebar_qt;

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
    // Phase 4: the 1 Hz playback state pump (idempotent).
    playback_qt::start_poll_loop(app());
    // Phase 7: the sidebar playlist tree.
    load_sidebar_once();

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
        // A later login must re-fetch Home + Library (new user, fresh data).
        *HOME_LOADED.lock().unwrap() = false;
        *LIBRARY_LOADED.lock().unwrap() = false;
        ui(|mut b| {
            b.as_mut().set_session_user_name(QString::from(""));
            b.as_mut().set_session_subscription(QString::from(""));
            b.as_mut().set_login_error(QString::from(""));
            b.as_mut().set_restore_error(QString::from(""));
            b.as_mut().set_login_phase(0);
            b.as_mut().set_home_sections_json(QString::from("[]"));
            b.as_mut().set_home_error(QString::from(""));
            b.as_mut().set_home_loading(false);
            b.as_mut().set_library_json(QString::from("[]"));
            b.as_mut().set_library_counts_json(QString::from("{}"));
            b.as_mut().set_library_error(QString::from(""));
            b.as_mut().set_library_loading(false);
            b.as_mut().set_sidebar_json(QString::from("[]"));
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

// ============================ Playback (phase 4) ===========================

// ============================ Sidebar + pins (phase 7) =====================

/// Load + publish the sidebar tree (session entry; idempotent per session).
fn load_sidebar_once() {
    static LOADED: Mutex<bool> = Mutex::new(false);
    if *LOADED.lock().unwrap() {
        return;
    }
    if offline_fwd::engine().status().is_offline() {
        log::info!("[qbz-qt] sidebar load skipped (offline session)");
        return;
    }
    *LOADED.lock().unwrap() = true;
    reload_sidebar();
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

fn publish_sidebar() {
    let entries = sidebar_qt::rebuild();
    let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into());
    log::debug!("[qbz-qt] sidebar published: {} entries ({} bytes)", entries.len(), json.len());
    let (sort_by, sort_asc) = sidebar_qt::sort_state();
    ui(move |mut b| {
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

/// Sidebar cover dispatch: plain url list (the tree's collage is
/// url-keyed). Disk hits emit immediately; misses download then emit.
pub(crate) fn sidebar_artwork_window(urls_json: String) {
    let urls: Vec<String> = serde_json::from_str(&urls_json).unwrap_or_default();
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

/// Sidebar "+" — create an empty playlist with the default name, then
/// reload the tree (the Slint flow opens a naming modal — POC-NOTE).
pub(crate) fn create_playlist() {
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

/// AlbumCard pin badge: toggle + signal the result.
pub(crate) fn toggle_pin(kind: String, id: String, title: String, subtitle: String, artwork_url: String) {
    if let Some(value) = sidebar_qt::toggle_pin(&kind, &id, &title, &subtitle, &artwork_url) {
        let key = format!("{kind}:{id}");
        ui(move |mut b| {
            b.as_mut().pin_changed(QString::from(key.as_str()), value);
        });
    }
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
    if view == "library" {
        load_library_once();
    }
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
    ui(|mut b| {
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
                ui(move |mut b| {
                    b.as_mut().set_library_json(QString::from(feed_json.as_str()));
                    b.as_mut()
                        .set_library_counts_json(QString::from(counts_json.as_str()));
                    b.as_mut().set_library_loading(false);
                });
                log_rss("library published");
            }
            Err(e) => {
                log::warn!("[qbz-qt] library load failed: {e}");
                ui(move |mut b| {
                    b.as_mut().set_library_error(QString::from(e.as_str()));
                    b.as_mut().set_library_loading(false);
                });
            }
        }
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
    log::debug!("[qbz-qt] library_artwork_window: {} keys", keys.len());
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
    ui(move |mut b| {
        b.as_mut()
            .library_artwork_ready(QString::from(key.as_str()), QString::from(path.as_str()));
    });
}

/// Card heart: toggle + signal the result (or the unchanged state on
/// failure, so the UI rolls back).
pub(crate) fn library_toggle_favorite(kind: String, id: String) {
    let key = library_qt::feed_key(&kind, &id);
    let runtime = app();
    spawn(async move {
        if let Some(value) = library_qt::toggle_favorite(&runtime, &kind, &id).await {
            ui(move |mut b| {
                b.as_mut()
                    .library_favorite_changed(QString::from(key.as_str()), value);
            });
        }
    });
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

/// Build the bridge's `queueModel` QVariantList from per-row JSON strings
/// (each QVariant carries one QString — see the nesting POC-NOTE in
/// playback_qt.rs `publish_queue`).
pub(crate) fn json_rows_to_qvariant_list(
    rows: Vec<String>,
) -> cxx_qt_lib::QList<cxx_qt_lib::QVariant> {
    let mut list = cxx_qt_lib::QList::<cxx_qt_lib::QVariant>::default();
    for row in rows {
        list.append(<QString as cxx_qt_lib::QVariantValue>::construct(&QString::from(
            row.as_str(),
        )));
    }
    list
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
