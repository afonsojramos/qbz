//! QbzArtist — Artist detail view domain bridge (phase 23 split of the
//! QbzBridge God-object; the pattern is documented in main.rs).
//!
//! The module is `qbz_artist_bridge`, not `qbz_artist`: bridge modules are
//! never named after a workspace crate (the `qbz_library` collision is the
//! documented precedent). The QML type name comes from the QObject
//! (`QbzArtist`), not from the module, so QML is unaffected.
//!
//! Props: the one artist-view JSON document + its loading flag.
//! Invokables: openArtist, loadReleaseSection (per-bucket "Load more").
//! Signal: releaseSectionReady (the next page of one releases bucket).
//!
//! `view_param_id` is NOT here and NOT in album_bridge.rs: it was a
//! write-only property (three setters in main.rs — album, artist, playlist —
//! and zero readers in QML or Rust, verified by grep). Splitting it would
//! have duplicated dead state into two singletons; the id each view actually
//! needs already travels inside its own document (`ArtistViewData.id` /
//! `AlbumHeader.id`), so it was deleted outright instead.

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_artist_bridge {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Artist detail view --------------------------------------------
        #[qproperty(bool, artist_loading)]
        // ONE JSON document (artist_qt.rs ArtistViewData: header + top
        // tracks + releases buckets + labels/similar/playlists).
        #[qproperty(QString, artist_json)]

        type QbzArtist = super::QbzArtistRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzArtist>);

        /// Open the artist detail view (pushes "artist" on the nav stack).
        #[qinvokable]
        fn open_artist(self: Pin<&mut QbzArtist>, artist_id: QString);

        /// ArtistView per-section "Load more" — the next releases page.
        #[qinvokable]
        fn load_release_section(self: Pin<&mut QbzArtist>, artist_id: QString, release_type: QString, offset: i32);

        /// Emitted with the next page of a releases bucket.
        #[qsignal]
        fn release_section_ready(self: Pin<&mut QbzArtist>, release_type: QString, cards_json: QString, has_more: bool);
    }

    impl cxx_qt::Threading for QbzArtist {}
}

use qbz_artist_bridge::QbzArtist;

/// Rust side of the artist bridge (plain storage, phase-1 pattern).
pub struct QbzArtistRust {
    artist_loading: bool,
    artist_json: QString,
}

impl Default for QbzArtistRust {
    fn default() -> Self {
        Self {
            artist_loading: false,
            artist_json: QString::from("{}"),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzArtist>> = OnceLock::new();

/// Queue an artist-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzArtist>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

impl qbz_artist_bridge::QbzArtist {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] artist Qt thread already registered");
        }
    }

    pub fn open_artist(self: Pin<&mut Self>, artist_id: QString) {
        crate::open_artist(artist_id.to_string());
    }

    pub fn load_release_section(self: Pin<&mut Self>, artist_id: QString, release_type: QString, offset: i32) {
        crate::load_release_section(artist_id.to_string(), release_type.to_string(), offset);
    }
}
