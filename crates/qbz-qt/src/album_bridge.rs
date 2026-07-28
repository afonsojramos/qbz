//! QbzAlbum — Album detail view domain bridge (phase 23 split of the
//! QbzBridge God-object; the pattern is documented in main.rs).
//!
//! The module is `qbz_album_bridge`, not `qbz_album`: bridge modules are
//! never named after a workspace crate (the `qbz_library` collision is the
//! documented precedent). The QML type name comes from the QObject
//! (`QbzAlbum`), not from the module, so QML is unaffected.
//!
//! Props: the one album-view JSON document + its loading flag.
//! Invokable: openAlbum (nav push + fetch + publish).
//!
//! The album/artist domains are TWO singletons on purpose (owner decision):
//! one per view, so a view's state can never be perturbed by the other's
//! fetch. `view_param_id` did NOT come along — see artist_bridge.rs for the
//! deletion rationale; the id QML needs is already inside `albumJson`
//! (`AlbumHeader.id`).

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_album_bridge {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Album detail view ---------------------------------------------
        #[qproperty(bool, album_loading)]
        // ONE JSON document (album_qt.rs AlbumViewData: header + track rows
        // + works/goodies/related buckets); QML JSON.parse()s it once.
        #[qproperty(QString, album_json)]
        // --- Track Info modal (qml/shell/TrackInfoModal.qml) --------------
        #[qproperty(bool, track_info_loading)]
        #[qproperty(QString, track_info_json)]

        type QbzAlbum = super::QbzAlbumRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzAlbum>);

        /// Open the album detail view (pushes "album" on the nav stack).
        #[qinvokable]
        fn open_album(self: Pin<&mut QbzAlbum>, album_id: QString);
        /// Info button on either now-playing bar: fetch + publish the Track
        /// Info document (track_info_qt.rs).
        #[qinvokable]
        fn open_track_info(self: Pin<&mut QbzAlbum>, track_id: QString);
    }

    impl cxx_qt::Threading for QbzAlbum {}
}

use qbz_album_bridge::QbzAlbum;

/// Rust side of the album bridge (plain storage, phase-1 pattern).
pub struct QbzAlbumRust {
    album_loading: bool,
    album_json: QString,
    track_info_loading: bool,
    track_info_json: QString,
}

impl Default for QbzAlbumRust {
    fn default() -> Self {
        Self {
            album_loading: false,
            album_json: QString::from("{}"),
            track_info_loading: false,
            track_info_json: QString::from("{}"),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzAlbum>> = OnceLock::new();

/// Queue an album-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzAlbum>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

impl qbz_album_bridge::QbzAlbum {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] album Qt thread already registered");
        }
    }

    pub fn open_album(self: Pin<&mut Self>, album_id: QString) {
        crate::open_album(album_id.to_string());
    }
    pub fn open_track_info(self: Pin<&mut Self>, track_id: QString) {
        crate::track_info_qt::open(track_id.to_string());
    }

}
