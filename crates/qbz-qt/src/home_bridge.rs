//! QbzHome — Discover > Home domain bridge (phase 23 split of the QbzBridge
//! God-object; the pattern is documented in main.rs). Props: the three tab
//! section documents + loading/error. Invokable: reloadHome (the retry
//! button / manual refresh).

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_home {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Discover > Home ---------------------------------------------
        #[qproperty(bool, home_loading)]
        #[qproperty(QString, home_error)]
        // POC-NOTE: the brief's QVariantList-of-QVariantMap shape is not
        // expressible in cxx-qt-lib 0.7.3 (no QVariantValue impls for
        // QMap/QList — a QVariant can't hold a nested map/list), so the
        // sections travel as ONE JSON document; QML JSON.parse()s it into
        // the exact same {id, title, kind, items:[{...}]} object graph.
        #[qproperty(QString, home_sections_json)]
        // Editor's Picks / For You tab section lists (phase 13; same
        // HomeSection shape, ordered by each tab's discover prefs).
        #[qproperty(QString, editor_sections_json)]
        #[qproperty(QString, for_you_sections_json)]
        // Recommendations tab (the 4th) — same HomeSection shape, published
        // by src/recommendations_qt.rs. LAZY: nothing is fetched until the
        // view calls `loadRecommendations()`, and a row whose service is not
        // connected is simply absent from the document.
        #[qproperty(QString, reco_sections_json)]
        #[qproperty(bool, reco_loading)]

        type QbzHome = super::QbzHomeRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzHome>);

        /// Home view retry button / manual refresh: refetch the discover
        /// index + rails and republish `homeSectionsJson`.
        #[qinvokable]
        fn reload_home(self: Pin<&mut QbzHome>);

        /// Discover > Recommendations became visible: build the tab once per
        /// session (idempotent; a re-entry only repaints from memory).
        #[qinvokable]
        fn load_recommendations(self: Pin<&mut QbzHome>);
    }

    impl cxx_qt::Threading for QbzHome {}
}

use qbz_home::QbzHome;

/// Rust side of the home bridge (plain storage, phase-1 pattern).
pub struct QbzHomeRust {
    home_loading: bool,
    home_error: QString,
    home_sections_json: QString,
    editor_sections_json: QString,
    for_you_sections_json: QString,
    reco_sections_json: QString,
    reco_loading: bool,
}

impl Default for QbzHomeRust {
    fn default() -> Self {
        Self {
            home_loading: false,
            home_error: QString::default(),
            home_sections_json: QString::from("[]"),
            editor_sections_json: QString::from("[]"),
            for_you_sections_json: QString::from("[]"),
            reco_sections_json: QString::from("[]"),
            reco_loading: false,
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzHome>> = OnceLock::new();

/// Queue a home-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzHome>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

impl qbz_home::QbzHome {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] home Qt thread already registered");
        }
    }

    pub fn reload_home(self: Pin<&mut Self>) {
        crate::reload_home();
    }

    pub fn load_recommendations(self: Pin<&mut Self>) {
        crate::recommendations_qt::ensure_loaded();
    }
}
