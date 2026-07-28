//! QbzQueue — queue-panel domain bridge (phase 23 split of the QbzBridge
//! God-object; the pattern is documented in main.rs). Props: the legacy
//! queueModel list + the queueJson document. Invokables: the panel's
//! tabs/pagination/search and row actions — one-line forwards into the
//! crate handlers.

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_queue {
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
        // --- Queue panel -------------------------------------------------
        // POC: empty until phase 4 (QML shows the empty states).
        #[qproperty(QList_QVariant, queue_model)]

        // --- Queue panel (phase 9) -----------------------------------------
        // One JSON document (queue_qt.rs QueueDoc: current/upcoming/history
        // + pagination + #442 section markers). Supersedes `queueModel`.
        #[qproperty(QString, queue_json)]

        type QbzQueue = super::QbzQueueRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzQueue>);

        /// Queue panel: tabs/pagination/search.
        #[qinvokable]
        fn queue_set_page(self: Pin<&mut QbzQueue>, page: i32);
        #[qinvokable]
        fn queue_set_search(self: Pin<&mut QbzQueue>, query: QString);
        /// Row actions.
        #[qinvokable]
        fn queue_play_upcoming(self: Pin<&mut QbzQueue>, index: i32);
        #[qinvokable]
        fn queue_remove_upcoming(self: Pin<&mut QbzQueue>, index: i32);
        #[qinvokable]
        fn queue_remove_all_after(self: Pin<&mut QbzQueue>, index: i32);
        #[qinvokable]
        fn queue_move_track(self: Pin<&mut QbzQueue>, from: i32, to: i32);
        #[qinvokable]
        fn queue_play_history(self: Pin<&mut QbzQueue>, index: i32);
        #[qinvokable]
        fn queue_toggle_favorite(self: Pin<&mut QbzQueue>, kind: QString, id: QString);
        /// Footer: Clear queue.
        #[qinvokable]
        fn queue_clear(self: Pin<&mut QbzQueue>);
    }

    impl cxx_qt::Threading for QbzQueue {}
}

use cxx_qt_lib::{QList, QVariant};
use qbz_queue::QbzQueue;

type QListQVariant = QList<QVariant>;

/// Rust side of the queue bridge (plain storage, phase-1 pattern).
pub struct QbzQueueRust {
    queue_model: QListQVariant,
    queue_json: QString,
}

impl Default for QbzQueueRust {
    fn default() -> Self {
        Self {
            queue_model: QListQVariant::default(),
            queue_json: QString::from("{}"),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzQueue>> = OnceLock::new();

/// Queue a queue-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzQueue>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

impl qbz_queue::QbzQueue {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] queue Qt thread already registered");
        }
    }

    pub fn queue_set_page(self: Pin<&mut Self>, page: i32) {
        crate::queue_set_page(page);
    }

    pub fn queue_set_search(self: Pin<&mut Self>, query: QString) {
        crate::queue_set_search(query.to_string());
    }

    pub fn queue_play_upcoming(self: Pin<&mut Self>, index: i32) {
        crate::queue_play_upcoming(index);
    }

    pub fn queue_remove_upcoming(self: Pin<&mut Self>, index: i32) {
        crate::queue_remove_upcoming(index);
    }

    pub fn queue_remove_all_after(self: Pin<&mut Self>, index: i32) {
        crate::queue_remove_all_after(index);
    }

    pub fn queue_move_track(self: Pin<&mut Self>, from: i32, to: i32) {
        crate::queue_move_track(from, to);
    }

    pub fn queue_play_history(self: Pin<&mut Self>, index: i32) {
        crate::queue_play_history(index);
    }

    pub fn queue_toggle_favorite(self: Pin<&mut Self>, kind: QString, id: QString) {
        crate::queue_toggle_favorite(kind.to_string(), id.to_string());
    }

    pub fn queue_clear(self: Pin<&mut Self>) {
        crate::queue_clear();
    }
}
