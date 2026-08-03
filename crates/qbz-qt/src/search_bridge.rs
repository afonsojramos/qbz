//! QbzSearch — the search domain bridge (cortinilla + results page).
//!
//! Extracted from the `QbzBridge` god-object per TRACK-RULES §2 (*"un
//! `#[cxx_qt::bridge]` por dominio, nunca un God-object"*) and the
//! 2026-08-03 cortinilla-parity contract §1.2, commit C0. `QbzBridge`
//! carried FIVE domains; this is the Search one, moved verbatim.
//!
//! Surface:
//! - **Cortinilla** (the live as-you-type dropdown): open/loading flags, the
//!   live query, ONE JSON payload (`search_qt.rs` `CortinillaData`:
//!   query/top/sections with controller-assigned flat indices), the keyboard
//!   selection and its content-space scroll y.
//! - **Results page**: ONE JSON document (`search_qt.rs` `SearchPageDoc`),
//!   the five-tab strip, per-type "Load more" and the searchType radios.
//! - The **intelligent-search kill switch** (the 2.0.0 opt-out module).
//!
//! Boot order matters: `ui()` below is a SILENT NO-OP until `boot()` has
//! registered this bridge's `CxxQtThread`, so a missing `QbzSearch.boot()`
//! in `qml/Main.qml` does not error — the dropdown simply never repaints
//! (TRACK-RULES §"Orden de boot de los singletons").
//!
//! Two renames landed with the move, both deliberate:
//! - `selected_index` -> `cortinilla_selected_index` (QML
//!   `cortinillaSelectedIndex`). It is cortinilla-only state and `selectedIndex`
//!   on an app-wide singleton is a collision hazard.
//! - `cortinilla_scroll_y` `f32` -> `f64`, unifying with the immersive twin
//!   (`immersive_bridge.rs`), which was already `f64`.

#[cxx_qt::bridge]
pub mod qbz_search {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Cortinilla (the live dropdown) --------------------------------
        #[qproperty(bool, cortinilla_open)]
        #[qproperty(bool, cortinilla_loading)]
        #[qproperty(QString, cortinilla_json)]
        /// The LIVE header query, published SYNCHRONOUSLY on every keystroke
        /// that passes the >= 2-char gate — ahead of the debounce and ahead
        /// of any fetch. Slint's twin is `SearchState.cortinilla-query`
        /// (`state.slint`), set at `main.rs:9647` before the debounce starts.
        ///
        /// Declared here by the C0 move so the domain's surface lands in one
        /// commit; it is WIRED in C2, which is what makes Enter carry the
        /// text the user actually typed instead of the last successfully
        /// loaded payload's query (contract §4.1 D-BUG-2).
        #[qproperty(QString, cortinilla_query)]
        /// Keyboard selection over the controller-assigned flat indices.
        /// -1 = nothing selected. Written ONLY by the controller: hover is
        /// purely visual and must never write it, or the mouse would hijack
        /// the row that Enter activates (contract row E8).
        #[qproperty(i32, cortinilla_selected_index)]
        /// Content-space top-y of the selected row, for scroll-into-view.
        #[qproperty(f64, cortinilla_scroll_y)]

        // --- Results page --------------------------------------------------
        #[qproperty(QString, search_json)]

        // --- Intelligent Search kill switch --------------------------------
        /// `ui_prefs.intelligent_search` mirror. Live-flippable, no restart.
        /// The LIVE path is the Settings > Appearance row
        /// (`settings_qt.rs` -> `search_qt::set_enabled`); there is no
        /// app-menu surface in either frontend.
        #[qproperty(bool, intelligent_search)]

        type QbzSearch = super::QbzSearchRust;

        /// Registers this bridge's Qt-thread hop. MUST be called from
        /// `qml/Main.qml`; before it, `ui()` silently drops every update.
        #[qinvokable]
        fn boot(self: Pin<&mut QbzSearch>);

        /// Header field keystrokes: drive the cortinilla live query.
        #[qinvokable]
        fn search_live(self: Pin<&mut QbzSearch>, query: QString);
        /// Enter with the cortinilla closed: full results page (All tab).
        #[qinvokable]
        fn search_submit(self: Pin<&mut QbzSearch>, query: QString);
        /// Cortinilla: Esc / click-outside / idle-close / page change.
        #[qinvokable]
        fn cortinilla_dismiss(self: Pin<&mut QbzSearch>);
        /// Arrow keys: delta -1 (up) / +1 (down) through the flat list.
        #[qinvokable]
        fn cortinilla_move_selection(self: Pin<&mut QbzSearch>, delta: i32);
        /// Row click or Enter on the keyboard-selected row.
        #[qinvokable]
        fn cortinilla_row_clicked(self: Pin<&mut QbzSearch>, index: i32);
        /// Section "View more" (kind: album | track | artist | playlist).
        #[qinvokable]
        fn cortinilla_view_more(self: Pin<&mut QbzSearch>, kind: QString);
        /// The Enter affordance with no keyboard selection: Search > All.
        #[qinvokable]
        fn cortinilla_search_all(self: Pin<&mut QbzSearch>);
        /// Results page: the five-tab strip.
        #[qinvokable]
        fn search_tab_changed(self: Pin<&mut QbzSearch>, tab: i32);
        /// Per-type tab "Load more".
        #[qinvokable]
        fn search_load_more(self: Pin<&mut QbzSearch>, tab: i32);
        /// searchType filter radios (0 = none, 1..5 = the chips).
        #[qinvokable]
        fn search_filter_changed(self: Pin<&mut QbzSearch>, index: i32);
        /// Intelligent-search toggle. NOTE: this has zero QML callers — the
        /// live path is the Settings row. Deleted in C1 (contract DEAD-1);
        /// moved here first so every intermediate commit compiles.
        #[qinvokable]
        fn toggle_intelligent_search(self: Pin<&mut QbzSearch>);
    }

    impl cxx_qt::Threading for QbzSearch {}
}

use core::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

use qbz_search::QbzSearch;

/// Rust side of the bridge. Plain storage; every field is driven through the
/// generated `set_*` methods on the Qt thread.
pub struct QbzSearchRust {
    cortinilla_open: bool,
    cortinilla_loading: bool,
    cortinilla_json: QString,
    cortinilla_query: QString,
    cortinilla_selected_index: i32,
    cortinilla_scroll_y: f64,
    search_json: QString,
    intelligent_search: bool,
}

impl Default for QbzSearchRust {
    fn default() -> Self {
        Self {
            cortinilla_open: false,
            cortinilla_loading: false,
            cortinilla_json: QString::from("{}"),
            cortinilla_query: QString::from(""),
            cortinilla_selected_index: -1,
            cortinilla_scroll_y: 0.0,
            search_json: QString::from("{}"),
            intelligent_search: crate::search_qt::intelligent_search_pref(),
        }
    }
}

static QT_THREAD: OnceLock<CxxQtThread<QbzSearch>> = OnceLock::new();

/// Queue a search-bridge mutation onto the Qt event loop. NO-OP before
/// `boot()` registers the thread — by design, and the reason a missing
/// `QbzSearch.boot()` shows up as "the dropdown never updates" rather than
/// as an error.
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzSearch>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

impl qbz_search::QbzSearch {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] search Qt thread already registered");
        }
    }

    pub fn search_live(self: Pin<&mut Self>, query: QString) {
        crate::search_live(query.to_string());
    }

    pub fn search_submit(self: Pin<&mut Self>, query: QString) {
        crate::search_submit(query.to_string());
    }

    pub fn cortinilla_dismiss(self: Pin<&mut Self>) {
        crate::search_qt::dismiss();
    }

    pub fn cortinilla_move_selection(self: Pin<&mut Self>, delta: i32) {
        crate::search_qt::move_selection(delta);
    }

    pub fn cortinilla_row_clicked(self: Pin<&mut Self>, index: i32) {
        crate::search_qt::row_clicked(index);
    }

    pub fn cortinilla_view_more(self: Pin<&mut Self>, kind: QString) {
        crate::cortinilla_view_more(kind.to_string());
    }

    pub fn cortinilla_search_all(self: Pin<&mut Self>) {
        crate::cortinilla_search_all();
    }

    pub fn search_tab_changed(self: Pin<&mut Self>, tab: i32) {
        crate::search_qt::tab_changed(tab);
    }

    pub fn search_load_more(self: Pin<&mut Self>, tab: i32) {
        crate::search_load_more(tab);
    }

    pub fn search_filter_changed(self: Pin<&mut Self>, index: i32) {
        crate::search_filter_changed(index);
    }

    pub fn toggle_intelligent_search(self: Pin<&mut Self>) {
        crate::toggle_intelligent_search();
    }
}
