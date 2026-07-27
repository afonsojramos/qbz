//! The single Qt-side bridge object for the POC.
//!
//! One `QbzBridge` QObject is registered as a QML SINGLETON (`QbzBridge.*`
//! in QML). All session/login/offline/shell state the QML needs lives in
//! its properties; user actions come in as invokables. Invokable bodies
//! NEVER block the Qt thread — they enqueue work onto the process-global
//! tokio runtime (see `main.rs`) and the async results hop back here
//! through `CxxQtThread::queue` (the cxx-qt analogue of Slint's
//! `upgrade_in_event_loop`).
//!
//! `#[auto_cxx_name]` on the extern blocks keeps Rust names snake_case
//! while QML/C++ see camelCase (`login_phase` -> `loginPhase`), matching
//! the property names of the Slint `LoginState`/`OfflineState` globals.

#[cxx_qt::bridge]
pub mod qbz_bridge {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qlist.h");
        type QList_QVariant = cxx_qt_lib::QList<cxx_qt_lib::QVariant>;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // "splash" | "login" | "shell"
        #[qproperty(QString, screen)]
        // Login narration: 0 idle / 1 waiting-for-browser / 2 authenticating.
        #[qproperty(i32, login_phase)]
        // Last SIGN-IN failure message ("" = none). Mirrors Slint
        // `LoginState.error`.
        #[qproperty(QString, login_error)]
        // Session-RESTORE failure message ("" = none). Mirrors Slint
        // `OfflineState.login-error`. (The Slint app has two separate fields;
        // keeping both here so the two login-screen boxes stay distinct.)
        #[qproperty(QString, restore_error)]
        // Offline-MODE engine snapshot (Slint `OfflineState`):
        #[qproperty(bool, offline)]
        // 0 online / 1 real offline / 2 induced offline.
        #[qproperty(i32, offline_mode)]
        // 0 unknown / 1 up / 2 down.
        #[qproperty(i32, connectivity)]
        #[qproperty(bool, captive_portal)]
        #[qproperty(bool, has_previous_session)]
        #[qproperty(bool, show_recovery_banner)]
        #[qproperty(bool, offline_session)]
        // Shell session header:
        #[qproperty(QString, session_user_name)]
        #[qproperty(QString, session_subscription)]

        // --- Shell chrome (Slint ShellState) ----------------------------
        // Three-state sidebar: 0 = open (240px), 1 = mini (64px), 2 = closed.
        #[qproperty(i32, sidebar_state)]
        #[qproperty(bool, queue_open)]
        // Content view id — only "home" exists (phase 3 adds the rest).
        #[qproperty(QString, current_view)]
        // Nav history (src/nav_qt.rs):
        #[qproperty(bool, can_back)]
        #[qproperty(bool, can_forward)]

        // --- Now playing (Slint NowPlayingState; np_ prefix) ------------
        // POC: fed by a static NowPlayingModel (src/now_playing.rs) with
        // empty-state defaults; phase 4 swaps the data source for the real
        // player poll.
        #[qproperty(bool, np_has_track)]
        #[qproperty(QString, np_title)]
        #[qproperty(QString, np_artist)]
        #[qproperty(QString, np_artwork_path)]
        #[qproperty(i32, np_elapsed_secs)]
        #[qproperty(i32, np_duration_secs)]
        #[qproperty(f32, np_progress)]
        #[qproperty(f32, np_cache_progress)]
        #[qproperty(bool, np_playing)]
        #[qproperty(bool, np_loading)]
        #[qproperty(f32, np_volume)]
        #[qproperty(bool, np_muted)]
        #[qproperty(bool, np_shuffle)]
        // 0 off / 1 all / 2 one.
        #[qproperty(i32, np_repeat_mode)]
        // "hires" | "mp3" | "lossless" | "cd" (AudioStamp tier mapping).
        #[qproperty(QString, np_quality_tier)]
        // e.g. "24-bit / 96 kHz" (AudioStamp detail line).
        #[qproperty(QString, np_quality_label)]

        // --- Queue panel -------------------------------------------------
        // POC: empty until phase 4 (QML shows the empty states).
        #[qproperty(QList_QVariant, queue_model)]

        // --- Discover > Home ---------------------------------------------
        #[qproperty(bool, home_loading)]
        #[qproperty(QString, home_error)]
        // POC-NOTE: the brief's QVariantList-of-QVariantMap shape is not
        // expressible in cxx-qt-lib 0.7.3 (no QVariantValue impls for
        // QMap/QList — a QVariant can't hold a nested map/list), so the
        // sections travel as ONE JSON document; QML JSON.parse()s it into
        // the exact same {id, title, kind, items:[{...}]} object graph.
        #[qproperty(QString, home_sections_json)]

        type QbzBridge = super::QbzBridgeRust;

        /// Called once from Main.qml's Component.onCompleted: registers the
        /// Qt thread handle and kicks off the boot sequence (offline engine
        /// start + silent session restore).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzBridge>);

        /// Login screen primary button: system-browser OAuth (no webview).
        #[qinvokable]
        fn sign_in_via_browser(self: Pin<&mut QbzBridge>);

        /// Cancel link (phase 1): aborts the in-flight OAuth task.
        #[qinvokable]
        fn cancel_login(self: Pin<&mut QbzBridge>);

        /// "Start offline" link: unauthenticated offline session.
        #[qinvokable]
        fn start_offline(self: Pin<&mut QbzBridge>);

        /// Recovery badge "Sign in": same browser flow.
        #[qinvokable]
        fn recovery_login(self: Pin<&mut QbzBridge>);

        /// Terms-of-Service link: opens the system browser.
        #[qinvokable]
        fn open_tos(self: Pin<&mut QbzBridge>);

        /// Shell logout: back to the login screen.
        #[qinvokable]
        fn logout(self: Pin<&mut QbzBridge>);

        /// i18n lookup against the shared gettext catalog (qbz-i18n), so the
        /// QML texts reuse the EXISTING .po translations via the same msgids
        /// the Slint `@tr()` calls use.
        #[qinvokable]
        fn tr(self: &QbzBridge, msgid: QString) -> QString;

        // --- Shell chrome -------------------------------------------------
        /// Header panel-left button: cycle the sidebar open -> mini ->
        /// closed -> open (Slint `ShellState.cycle-sidebar()`).
        #[qinvokable]
        fn cycle_sidebar(self: Pin<&mut QbzBridge>);
        /// NPB queue button / queue panel close.
        #[qinvokable]
        fn toggle_queue(self: Pin<&mut QbzBridge>);
        /// Header history buttons.
        #[qinvokable]
        fn navigate_back(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn navigate_forward(self: Pin<&mut QbzBridge>);

        // --- Transport (phase 4 wires the player; POC log-and-noop, except
        // the pure-UI toggles which mutate the NowPlayingModel) -----------
        #[qinvokable]
        fn toggle_play(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn next(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn previous(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn seek(self: Pin<&mut QbzBridge>, frac: f32);
        #[qinvokable]
        fn set_volume(self: Pin<&mut QbzBridge>, volume: f32);
        #[qinvokable]
        fn toggle_mute(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn toggle_shuffle(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn cycle_repeat(self: Pin<&mut QbzBridge>);

        /// Home view retry button / manual refresh: refetch the discover
        /// index + rails and republish `homeSectionsJson`.
        #[qinvokable]
        fn reload_home(self: Pin<&mut QbzBridge>);
    }

    impl cxx_qt::Threading for QbzBridge {}
}

use core::pin::Pin;

use cxx_qt::Threading as _;
use cxx_qt_lib::{QList, QString, QVariant};

type QListQVariant = QList<QVariant>;

/// Rust side of the bridge. All fields are driven exclusively through the
/// generated `set_*` methods on the Qt thread; the struct itself is plain
/// storage (as required by cxx-qt's Default-constructed qobjects).
pub struct QbzBridgeRust {
    screen: QString,
    login_phase: i32,
    login_error: QString,
    restore_error: QString,
    offline: bool,
    offline_mode: i32,
    connectivity: i32,
    captive_portal: bool,
    has_previous_session: bool,
    show_recovery_banner: bool,
    offline_session: bool,
    session_user_name: QString,
    session_subscription: QString,
    sidebar_state: i32,
    queue_open: bool,
    current_view: QString,
    can_back: bool,
    can_forward: bool,
    np_has_track: bool,
    np_title: QString,
    np_artist: QString,
    np_artwork_path: QString,
    np_elapsed_secs: i32,
    np_duration_secs: i32,
    np_progress: f32,
    np_cache_progress: f32,
    np_playing: bool,
    np_loading: bool,
    np_volume: f32,
    np_muted: bool,
    np_shuffle: bool,
    np_repeat_mode: i32,
    np_quality_tier: QString,
    np_quality_label: QString,
    queue_model: QListQVariant,
    home_loading: bool,
    home_error: QString,
    home_sections_json: QString,
}

impl Default for QbzBridgeRust {
    fn default() -> Self {
        Self {
            screen: QString::from("splash"),
            current_view: QString::from("home"),
            // A derive would zero these — the model's sane defaults instead.
            np_volume: 1.0,
            np_quality_tier: QString::from("cd"),
            login_phase: 0,
            login_error: QString::default(),
            restore_error: QString::default(),
            offline: false,
            offline_mode: 0,
            connectivity: 0,
            captive_portal: false,
            has_previous_session: false,
            show_recovery_banner: false,
            offline_session: false,
            session_user_name: QString::default(),
            session_subscription: QString::default(),
            sidebar_state: 0,
            queue_open: false,
            can_back: false,
            can_forward: false,
            np_has_track: false,
            np_title: QString::default(),
            np_artist: QString::default(),
            np_artwork_path: QString::default(),
            np_elapsed_secs: 0,
            np_duration_secs: 0,
            np_progress: 0.0,
            np_cache_progress: 0.0,
            np_playing: false,
            np_loading: false,
            np_muted: false,
            np_shuffle: false,
            np_repeat_mode: 0,
            np_quality_label: QString::default(),
            queue_model: QListQVariant::default(),
            home_loading: false,
            home_error: QString::default(),
            home_sections_json: QString::from("[]"),
        }
    }
}

impl qbz_bridge::QbzBridge {
    pub fn boot(self: Pin<&mut Self>) {
        // The very first invokable: hand the Qt-thread handle to the process
        // global so tokio tasks can queue property updates back here.
        crate::register_qt_thread(self.qt_thread());
        crate::on_boot();
    }

    pub fn sign_in_via_browser(self: Pin<&mut Self>) {
        crate::start_login();
    }

    pub fn cancel_login(self: Pin<&mut Self>) {
        crate::cancel_login();
    }

    pub fn start_offline(self: Pin<&mut Self>) {
        crate::start_offline();
    }

    pub fn recovery_login(self: Pin<&mut Self>) {
        crate::start_login();
    }

    pub fn open_tos(self: Pin<&mut Self>) {
        // Same URL the Slint shell opens (crates/qbz/src/main.rs
        // QOBUZ_TOS_URL). `open` spawns xdg-open detached, so this is safe to
        // run on the Qt thread.
        if let Err(e) = open::that(crate::QOBUZ_TOS_URL) {
            log::error!("[qbz-qt] failed to open Terms of Service: {e}");
        }
    }

    pub fn logout(self: Pin<&mut Self>) {
        crate::do_logout();
    }

    pub fn tr(&self, msgid: QString) -> QString {
        QString::from(&qbz_i18n::t(&msgid.to_string()))
    }

    pub fn cycle_sidebar(mut self: Pin<&mut Self>) {
        let next = (self.sidebar_state() + 1) % 3;
        self.as_mut().set_sidebar_state(next);
    }

    pub fn toggle_queue(mut self: Pin<&mut Self>) {
        let next = !self.queue_open();
        self.as_mut().set_queue_open(next);
    }

    pub fn navigate_back(self: Pin<&mut Self>) {
        crate::nav_qt::back();
    }

    pub fn navigate_forward(self: Pin<&mut Self>) {
        crate::nav_qt::forward();
    }

    pub fn toggle_play(self: Pin<&mut Self>) {
        crate::now_playing::toggle_play();
    }

    pub fn next(self: Pin<&mut Self>) {
        crate::now_playing::next();
    }

    pub fn previous(self: Pin<&mut Self>) {
        crate::now_playing::previous();
    }

    pub fn seek(self: Pin<&mut Self>, frac: f32) {
        crate::now_playing::seek(frac);
    }

    pub fn set_volume(self: Pin<&mut Self>, volume: f32) {
        crate::now_playing::set_volume(volume);
    }

    pub fn toggle_mute(self: Pin<&mut Self>) {
        crate::now_playing::toggle_mute();
    }

    pub fn toggle_shuffle(self: Pin<&mut Self>) {
        crate::now_playing::toggle_shuffle();
    }

    pub fn cycle_repeat(self: Pin<&mut Self>) {
        crate::now_playing::cycle_repeat();
    }

    pub fn reload_home(self: Pin<&mut Self>) {
        crate::reload_home();
    }
}
