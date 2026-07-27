//! The single Qt-side bridge object for the POC.
//!
//! One `QbzBridge` QObject is registered as a QML SINGLETON (`QbzBridge.*`
//! in QML). All session/login/offline state the QML needs lives in its
//! properties; user actions come in as invokables. Invokable bodies NEVER
//! block the Qt thread — they enqueue work onto the process-global tokio
//! runtime (see `main.rs`) and the async results hop back here through
//! `CxxQtThread::queue` (the cxx-qt analogue of Slint's
//! `upgrade_in_event_loop`).
//!
//! `#[auto_cxx_name]` on the extern blocks keeps Rust names snake_case while
//! QML/C++ see camelCase (`login_phase` -> `loginPhase`), matching the
//! property names of the Slint `LoginState`/`OfflineState` globals.

#[cxx_qt::bridge]
pub mod qbz_bridge {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
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

        /// Recovery banner "Sign in again": same browser flow.
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
    }

    impl cxx_qt::Threading for QbzBridge {}
}

use core::pin::Pin;

use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

/// Rust side of the bridge. All fields are driven exclusively through the
/// generated `set_*` methods on the Qt thread; the struct itself is plain
/// storage (as required by cxx-qt's Default-constructed qobjects).
#[derive(Default)]
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
}
