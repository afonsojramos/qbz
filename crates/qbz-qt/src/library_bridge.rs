//! QbzLibrary — Library view domain bridge (phase 23 split of the QbzBridge
//! God-object; the pattern is documented in main.rs). Props: the merged-feed
//! document + counts + loading/error. Invokables: reload, windowed artwork
//! dispatch, favorite/pin toggles. Signals: artwork-ready / favorite-changed
//! / pin-changed (emitted through the ui() hop like before).

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_library {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Library view --------------------------------------------------
        #[qproperty(bool, library_loading)]
        #[qproperty(QString, library_error)]
        // One JSON document: the FULL merged feed (tabs/search/sort/source
        // filters derive QML-side from the parsed array — see library_qt.rs
        // for the rationale + the measured parse cost).
        #[qproperty(QString, library_json)]
        // {tracks, albums, artists, playlists, labels, all} — tab badges.
        #[qproperty(QString, library_counts_json)]

        type QbzLibrary = super::QbzLibraryRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzLibrary>);

        /// Library retry button / manual refresh.
        #[qinvokable]
        fn reload_library(self: Pin<&mut QbzLibrary>);

        /// Windowed artwork: the grid/list reports the mounted window as a
        /// JSON array of artKeys (tab views filter the feed, so raw indices
        /// don't map); Rust dispatches covers for those keys (id-keyed via
        /// `libraryArtworkReady`).
        #[qinvokable]
        fn library_artwork_window(self: Pin<&mut QbzLibrary>, keys_json: QString);

        /// Card heart: toggle favorite (Qobuz API or the local store,
        /// routed by id shape); the result arrives via
        /// `libraryFavoriteChanged`.
        #[qinvokable]
        fn library_toggle_favorite(self: Pin<&mut QbzLibrary>, kind: QString, id: QString);

        /// Emitted when a dispatched cover lands on disk (id-keyed —
        /// `{kind}:{id}`); QML updates its artwork map.
        #[qsignal]
        fn library_artwork_ready(self: Pin<&mut QbzLibrary>, key: QString, path: QString);

        /// Emitted after a heart toggle (optimistic flip + rollback).
        #[qsignal]
        fn library_favorite_changed(self: Pin<&mut QbzLibrary>, key: QString, value: bool);

        /// AlbumCard pin badge: toggle pin (album/artist/playlist).
        #[qinvokable]
        fn toggle_pin(self: Pin<&mut QbzLibrary>, kind: QString, id: QString, title: QString, subtitle: QString, artwork_url: QString);
        /// Emitted after a pin toggle (`{kind}:{id}` key like artKey).
        #[qsignal]
        fn pin_changed(self: Pin<&mut QbzLibrary>, key: QString, value: bool);
    }

    impl cxx_qt::Threading for QbzLibrary {}
}

use qbz_library::QbzLibrary;

/// Rust side of the library bridge (plain storage, phase-1 pattern).
pub struct QbzLibraryRust {
    library_loading: bool,
    library_error: QString,
    library_json: QString,
    library_counts_json: QString,
}

impl Default for QbzLibraryRust {
    fn default() -> Self {
        Self {
            library_loading: false,
            library_error: QString::default(),
            library_json: QString::from("[]"),
            library_counts_json: QString::from("{}"),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzLibrary>> = OnceLock::new();

/// Queue a library-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzLibrary>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

impl qbz_library::QbzLibrary {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] library Qt thread already registered");
        }
    }

    pub fn reload_library(self: Pin<&mut Self>) {
        crate::reload_library();
    }

    pub fn library_artwork_window(self: Pin<&mut Self>, keys_json: QString) {
        crate::library_artwork_window(keys_json.to_string());
    }

    pub fn library_toggle_favorite(self: Pin<&mut Self>, kind: QString, id: QString) {
        crate::library_toggle_favorite(kind.to_string(), id.to_string());
    }

    pub fn toggle_pin(self: Pin<&mut Self>, kind: QString, id: QString, title: QString, subtitle: QString, artwork_url: QString) {
        crate::toggle_pin(
            kind.to_string(),
            id.to_string(),
            title.to_string(),
            subtitle.to_string(),
            artwork_url.to_string(),
        );
    }
}
