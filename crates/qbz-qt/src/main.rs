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
mod artwork_qt;
mod bridge;
mod discover_config_qt;
mod genre_filter_qt;
mod home_qt;
mod library_db_qt;
mod library_qt;
mod local_library_qt;
mod local_rows;
mod local_state;
mod local_plex;
mod local_albums;
mod local_tree;
mod local_artwork;
mod local_playback;
mod local_bridge_ops;
mod local_bulk;
mod local_ephemeral;
mod local_album_actions;
mod nav_qt;
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
mod playlist_qt;
mod queue_qt;
mod search_qt;
mod settings_qt;
mod sidebar_qt;
mod theme_qt;
mod integrations_qt;
mod viz_qt;

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
            Ok(None) => session_bridge::ui(|mut b| b.as_mut().set_screen(QString::from("login"))),
            Err(e) => session_bridge::ui(move |mut b| {
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
    // Integrations runtime: applies the persisted MusicBrainz / Discord /
    // ListenBrainz opt-ins and starts the scrobble-queue flush watcher.
    // Strictly opt-in — every one of them is inert until the user connects it.
    integrations_qt::start(&app());
    // Phase 7: the sidebar playlist tree.
    load_sidebar_once();
    // Phase 10: seed the playback request tier from the persisted
    // streaming-quality pref (Settings > Audio writes it live after this).
    playback_qt::set_streaming_quality(&settings_qt::streaming_quality());
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
                session_bridge::ui(move |mut b| {
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
        if let Err(e) = auth_qt::logout(&runtime).await {
            log::error!("[qbz-qt] logout failed: {e}");
        }
        // Integrations: drop the Discord presence so a signed-out session
        // stops advertising what was playing (the Slint discord_rpc::clear).
        integrations_qt::discord_clear();
        // The local-library documents are per-user; a later login must not
        // inherit the previous user's cached tree.
        local_library_qt::reset();
        // A later login must re-fetch Home + Library (new user, fresh data).
        *HOME_LOADED.lock().unwrap() = false;
        *LIBRARY_LOADED.lock().unwrap() = false;
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

/// AlbumCard pin badge: toggle + signal the result.
pub(crate) fn toggle_pin(kind: String, id: String, title: String, subtitle: String, artwork_url: String) {
    if let Some(value) = sidebar_qt::toggle_pin(&kind, &id, &title, &subtitle, &artwork_url) {
        let key = format!("{kind}:{id}");
        library_bridge::ui(move |mut b| {
            b.as_mut().pin_changed(QString::from(key.as_str()), value);
        });
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
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::load(&runtime, pid).await {
            log::warn!("[qbz-qt] playlist load failed: {e}");
        }
    });
}

pub(crate) fn playlist_play_all() {
    let runtime = app();
    spawn(async move {
        if let Err(e) = playlist_qt::play_all(&runtime).await {
            log::error!("[qbz-qt] playlist play-all failed: {e}");
        }
    });
}

pub(crate) fn playlist_shuffle() {
    let runtime = app();
    spawn(async move {
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

pub(crate) fn playlist_remove_track(playlist_track_id: u64) {
    let runtime = app();
    spawn(async move { playlist_qt::remove_track(&runtime, playlist_track_id).await });
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
pub(crate) fn library_toggle_favorite(kind: String, id: String) {
    let key = library_qt::feed_key(&kind, &id);
    let runtime = app();
    spawn(async move {
        if let Some(value) = library_qt::toggle_favorite(&runtime, &kind, &id).await {
            library_bridge::ui(move |mut b| {
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
