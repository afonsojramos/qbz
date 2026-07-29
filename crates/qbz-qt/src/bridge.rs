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
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Album / Artist detail views: MOVED (phase 23) ----------------
        // album_* -> QbzAlbum (album_bridge.rs), artist_* + the releases
        // "Load more" signal -> QbzArtist (artist_bridge.rs). `view_param_id`
        // was deleted, not moved: write-only, zero readers — the rationale
        // lives in artist_bridge.rs's header.

        // --- Settings (phase 10) -----------------------------------------
        // One JSON document (settings_qt.rs SettingsDoc: audio + playback
        // row states, select option lists, output-device groups).
        #[qproperty(QString, settings_json)]

        // --- Search (phase 15) ---------------------------------------------
        // Cortinilla (live dropdown): open/loading flags, ONE JSON payload
        // (search_qt.rs CortinillaData: query/top/sections with controller
        // flat indices), keyboard selection + its content-space scroll y.
        #[qproperty(bool, cortinilla_open)]
        #[qproperty(bool, cortinilla_loading)]
        #[qproperty(QString, cortinilla_json)]
        #[qproperty(i32, selected_index)]
        #[qproperty(f32, cortinilla_scroll_y)]
        // Results page: ONE JSON document (search_qt.rs SearchPageDoc).
        #[qproperty(QString, search_json)]
        // The intelligent-search kill switch (ui_prefs pref state — the
        // app-menu check; live-flippable, no restart).
        #[qproperty(bool, intelligent_search)]

        // --- Playlist view (phase 17) --------------------------------------
        // ONE JSON document (playlist_qt.rs PlaylistDoc: header + track
        // rows + ownership/follow/pin/sort/search state).
        #[qproperty(QString, playlist_json)]

        // --- Filter by genre (shared, per context) -------------------------
        // ONE JSON document (genre_filter_qt.rs FilterDoc): the popup model
        // (chips / tree / remember / advanced / loading) PLUS the per-context
        // selection counts and selected genre NAMES. Every surface that draws
        // a genre button reads this one property: Discover (context
        // "discover", server-side via get_discover_index) and Library > All
        // (context "library-all", client-side over the feed).
        #[qproperty(QString, genre_filter_json)]

        // --- Discover section configurator (the toolbar gear) --------------
        // ONE JSON document (discover_config_qt.rs ConfigDoc: the active
        // tab's ordered rows + enabled/total counts).
        #[qproperty(QString, discover_config_json)]

        type QbzBridge = super::QbzBridgeRust;

        /// TEMP (phase 23 split): registers the QbzBridge Qt-thread
        /// hop while the remaining domains still live here. Removed
        /// when the last domain moves out.
        #[qinvokable]
        fn boot(self: Pin<&mut QbzBridge>);

        // --- Settings (phase 10) ------------------------------------------
        /// Toggle rows (settings_qt.rs key dispatch; persists + applies +
        /// republishes `settingsJson`).
        #[qinvokable]
        fn settings_bool(self: Pin<&mut QbzBridge>, key: QString, value: bool);
        /// Select rows: the picked OPTION INDEX within the row's list.
        #[qinvokable]
        fn settings_select(self: Pin<&mut QbzBridge>, key: QString, index: i32);
        /// Slider rows (initial buffer size).
        #[qinvokable]
        fn settings_slider(self: Pin<&mut QbzBridge>, key: QString, value: i32);
        /// Free-text rows (Qobuz Connect device name).
        #[qinvokable]
        fn settings_string(self: Pin<&mut QbzBridge>, key: QString, value: QString);
        /// "Reset to defaults" (audio + playback stores, quality pref).
        #[qinvokable]
        fn settings_reset(self: Pin<&mut QbzBridge>);
        /// Output-device refresh button: release a held device, re-enumerate.
        #[qinvokable]
        fn refresh_devices(self: Pin<&mut QbzBridge>);

        // --- Integrations (phase 19) ---------------------------------------
        /// Non-toggle integration actions (integrations_qt.rs): Last.fm
        /// connect/open-auth-url/finish/disconnect, ListenBrainz disconnect.
        #[qinvokable]
        fn integrations_action(self: Pin<&mut QbzBridge>, action: QString);

        // --- Search (phase 15) ---------------------------------------------
        /// Header field keystrokes (QML-debounced 220ms, >= 2 chars): drive
        /// the cortinilla live query.
        #[qinvokable]
        fn search_live(self: Pin<&mut QbzBridge>, query: QString);
        /// Enter with the cortinilla closed: full results page (All tab).
        #[qinvokable]
        fn search_submit(self: Pin<&mut QbzBridge>, query: QString);
        /// Cortinilla: Esc / click-outside / idle-close / page change.
        #[qinvokable]
        fn cortinilla_dismiss(self: Pin<&mut QbzBridge>);
        /// Arrow keys: delta -1 (up) / +1 (down) through the flat list.
        #[qinvokable]
        fn cortinilla_move_selection(self: Pin<&mut QbzBridge>, delta: i32);
        /// Row click or Enter on the keyboard-selected row.
        #[qinvokable]
        fn cortinilla_row_clicked(self: Pin<&mut QbzBridge>, index: i32);
        /// Section "View more" (kind: album | track | artist | playlist).
        #[qinvokable]
        fn cortinilla_view_more(self: Pin<&mut QbzBridge>, kind: QString);
        /// The Enter affordance with no keyboard selection: Search > All.
        #[qinvokable]
        fn cortinilla_search_all(self: Pin<&mut QbzBridge>);
        /// Results page: the five-tab strip.
        #[qinvokable]
        fn search_tab_changed(self: Pin<&mut QbzBridge>, tab: i32);
        /// Per-type tab "Load more".
        #[qinvokable]
        fn search_load_more(self: Pin<&mut QbzBridge>, tab: i32);
        /// searchType filter radios (0 = none, 1..5 = the chips).
        #[qinvokable]
        fn search_filter_changed(self: Pin<&mut QbzBridge>, index: i32);
        /// App-menu intelligent-search toggle (the 2.0.0 opt-out module).
        #[qinvokable]
        fn toggle_intelligent_search(self: Pin<&mut QbzBridge>);

        // --- Playlist view (phase 17) --------------------------------------
        /// Sidebar playlist row / playlist card click: open the detail view.
        #[qinvokable]
        fn open_playlist(self: Pin<&mut QbzBridge>, playlist_id: QString);
        #[qinvokable]
        fn playlist_play_all(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn playlist_shuffle(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn playlist_toggle_favorite(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn playlist_toggle_follow(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn playlist_toggle_pin(self: Pin<&mut QbzBridge>);
        /// "Copy to your library" (foreign playlists).
        #[qinvokable]
        fn playlist_copy(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn playlist_rename(self: Pin<&mut QbzBridge>, name: QString);
        #[qinvokable]
        fn playlist_delete(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn playlist_set_sort(self: Pin<&mut QbzBridge>, field: QString);
        #[qinvokable]
        fn playlist_set_search(self: Pin<&mut QbzBridge>, query: QString);
        /// Row play / row ⋯ queueing / owner Remove-from-playlist / drag
        /// reorder.
        #[qinvokable]
        fn playlist_play_track(self: Pin<&mut QbzBridge>, track_id: QString);
        #[qinvokable]
        fn playlist_enqueue_track(self: Pin<&mut QbzBridge>, track_id: QString, mode: QString);
        #[qinvokable]
        fn playlist_remove_track(self: Pin<&mut QbzBridge>, playlist_track_id: f64);
        #[qinvokable]
        fn playlist_reorder(self: Pin<&mut QbzBridge>, from: i32, slot: i32);

        /// Card-level playlist actions (LibPlaylistCard overlay + menu).
        #[qinvokable]
        fn playlist_set_follow_by_id(self: Pin<&mut QbzBridge>, playlist_id: QString, follow: bool);

        // --- Filter by genre -----------------------------------------------
        /// Genre button click: switch the edited context ("discover" |
        /// "library-all"), publish the popup model, lazy-load the parent
        /// genres on first open.
        #[qinvokable]
        fn genre_open(self: Pin<&mut QbzBridge>, context: QString);
        /// Chip / tree-row click: flip the id in the CURRENT context, persist,
        /// and refresh that context's consumer.
        #[qinvokable]
        fn genre_toggle(self: Pin<&mut QbzBridge>, genre_id: QString);
        /// Advanced tree chevron: expand/collapse + lazy-load that level.
        #[qinvokable]
        fn genre_toggle_expand(self: Pin<&mut QbzBridge>, genre_id: QString);
        /// Advanced-view search box (filters the loaded genres, flat).
        #[qinvokable]
        fn genre_search(self: Pin<&mut QbzBridge>, query: QString);
        /// "Clear filter" — drops the CURRENT context's selection.
        #[qinvokable]
        fn genre_clear(self: Pin<&mut QbzBridge>);
        /// "Remember selection" toggle (off also deletes the persisted file).
        #[qinvokable]
        fn genre_set_remember(self: Pin<&mut QbzBridge>, value: bool);
        /// "Advanced view" toggle (eager-loads every parent's children).
        #[qinvokable]
        fn genre_set_advanced(self: Pin<&mut QbzBridge>, value: bool);

        // --- Discover section configurator ---------------------------------
        /// Gear click: publish the rows for the tab the modal will show.
        #[qinvokable]
        fn discover_config_open(self: Pin<&mut QbzBridge>, tab: QString);
        /// Row click / checkbox: show or hide that section on that tab.
        #[qinvokable]
        fn discover_toggle_section(self: Pin<&mut QbzBridge>, tab: QString, section_id: QString);
        /// Reorder chevrons: -1 up, +1 down (clamped at the boundaries).
        #[qinvokable]
        fn discover_move_section(
            self: Pin<&mut QbzBridge>,
            tab: QString,
            section_id: QString,
            dir: i32,
        );
        /// "Reset to defaults" — that tab only.
        #[qinvokable]
        fn discover_reset_tab(self: Pin<&mut QbzBridge>, tab: QString);
    }

    impl cxx_qt::Threading for QbzBridge {}
}

use core::pin::Pin;

use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

/// Rust side of the bridge. All fields are driven exclusively through the
/// generated `set_*` methods on the Qt thread; the struct itself is plain
/// storage (as required by cxx-qt's Default-constructed qobjects).
pub struct QbzBridgeRust {
    settings_json: QString,
    cortinilla_open: bool,
    cortinilla_loading: bool,
    cortinilla_json: QString,
    selected_index: i32,
    cortinilla_scroll_y: f32,
    search_json: QString,
    intelligent_search: bool,
    playlist_json: QString,
    genre_filter_json: QString,
    discover_config_json: QString,
}

impl Default for QbzBridgeRust {
    fn default() -> Self {
        Self {
            settings_json: QString::from("{}"),
            cortinilla_open: false,
            cortinilla_loading: false,
            cortinilla_json: QString::from("{}"),
            selected_index: -1,
            cortinilla_scroll_y: 0.0,
            search_json: QString::from("{}"),
            intelligent_search: crate::search_qt::intelligent_search_pref(),
            playlist_json: QString::from("{}"),
            // Seeded from the PERSISTED selection (no network): a remembered
            // filter colors both genre buttons and narrows the Library feed
            // before the popup has ever been opened.
            genre_filter_json: QString::from(
                crate::genre_filter_qt::current_json().as_str(),
            ),
            // Seeded for the tab Discover mounts on.
            discover_config_json: QString::from(
                crate::discover_config_qt::rows_json("home").as_str(),
            ),
        }
    }
}

impl qbz_bridge::QbzBridge {
    pub fn boot(self: Pin<&mut Self>) {
        crate::register_qt_thread(self.qt_thread());
    }

    pub fn integrations_action(self: Pin<&mut Self>, action: QString) {
        crate::integrations_action(action.to_string());
    }

    pub fn settings_bool(self: Pin<&mut Self>, key: QString, value: bool) {
        crate::settings_bool(key.to_string(), value);
    }

    pub fn settings_select(self: Pin<&mut Self>, key: QString, index: i32) {
        crate::settings_select(key.to_string(), index);
    }

    pub fn settings_slider(self: Pin<&mut Self>, key: QString, value: i32) {
        crate::settings_slider(key.to_string(), value);
    }

    pub fn settings_string(self: Pin<&mut Self>, key: QString, value: QString) {
        crate::settings_string(key.to_string(), value.to_string());
    }

    pub fn settings_reset(self: Pin<&mut Self>) {
        crate::settings_reset();
    }

    pub fn refresh_devices(self: Pin<&mut Self>) {
        crate::refresh_devices();
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

    pub fn open_playlist(self: Pin<&mut Self>, playlist_id: QString) {
        crate::open_playlist(playlist_id.to_string());
    }

    pub fn playlist_play_all(self: Pin<&mut Self>) {
        crate::playlist_play_all();
    }

    pub fn playlist_shuffle(self: Pin<&mut Self>) {
        crate::playlist_shuffle();
    }

    pub fn playlist_toggle_favorite(self: Pin<&mut Self>) {
        crate::playlist_qt::toggle_favorite();
    }

    pub fn playlist_toggle_follow(self: Pin<&mut Self>) {
        crate::playlist_toggle_follow();
    }

    pub fn playlist_toggle_pin(self: Pin<&mut Self>) {
        crate::playlist_qt::toggle_pin();
    }

    pub fn playlist_copy(self: Pin<&mut Self>) {
        crate::playlist_copy();
    }

    pub fn playlist_rename(self: Pin<&mut Self>, name: QString) {
        crate::playlist_rename(name.to_string());
    }

    pub fn playlist_delete(self: Pin<&mut Self>) {
        crate::playlist_delete();
    }

    pub fn playlist_set_sort(self: Pin<&mut Self>, field: QString) {
        crate::playlist_qt::set_sort(&field.to_string());
    }

    pub fn playlist_set_search(self: Pin<&mut Self>, query: QString) {
        crate::playlist_qt::set_search(&query.to_string());
    }

    pub fn playlist_play_track(self: Pin<&mut Self>, track_id: QString) {
        crate::playlist_play_track(track_id.to_string());
    }

    pub fn playlist_enqueue_track(self: Pin<&mut Self>, track_id: QString, mode: QString) {
        crate::playlist_enqueue_track(track_id.to_string(), mode.to_string());
    }

    pub fn playlist_remove_track(self: Pin<&mut Self>, playlist_track_id: f64) {
        crate::playlist_remove_track(playlist_track_id as u64);
    }

    pub fn playlist_reorder(self: Pin<&mut Self>, from: i32, slot: i32) {
        crate::playlist_qt::reorder_track(from.max(0) as usize, slot.max(0) as usize);
    }

    pub fn playlist_set_follow_by_id(self: Pin<&mut Self>, playlist_id: QString, follow: bool) {
        if let Ok(pid) = playlist_id.to_string().parse::<u64>() {
            crate::playlist_set_follow_by_id(pid, follow);
        }
    }

    pub fn genre_open(self: Pin<&mut Self>, context: QString) {
        crate::genre_filter_qt::open(&context.to_string());
    }

    pub fn genre_toggle(self: Pin<&mut Self>, genre_id: QString) {
        crate::genre_filter_qt::toggle(&genre_id.to_string());
    }

    pub fn genre_toggle_expand(self: Pin<&mut Self>, genre_id: QString) {
        crate::genre_filter_qt::toggle_expand(&genre_id.to_string());
    }

    pub fn genre_search(self: Pin<&mut Self>, query: QString) {
        crate::genre_filter_qt::set_search(&query.to_string());
    }

    pub fn genre_clear(self: Pin<&mut Self>) {
        crate::genre_filter_qt::clear();
    }

    pub fn genre_set_remember(self: Pin<&mut Self>, value: bool) {
        crate::genre_filter_qt::set_remember(value);
    }

    pub fn genre_set_advanced(self: Pin<&mut Self>, value: bool) {
        crate::genre_filter_qt::set_advanced(value);
    }

    pub fn discover_config_open(self: Pin<&mut Self>, tab: QString) {
        crate::discover_config_qt::open(&tab.to_string());
    }

    pub fn discover_toggle_section(self: Pin<&mut Self>, tab: QString, section_id: QString) {
        crate::discover_config_qt::toggle_section(&tab.to_string(), &section_id.to_string());
    }

    pub fn discover_move_section(self: Pin<&mut Self>, tab: QString, section_id: QString, dir: i32) {
        crate::discover_config_qt::move_section(&tab.to_string(), &section_id.to_string(), dir);
    }

    pub fn discover_reset_tab(self: Pin<&mut Self>, tab: QString) {
        crate::discover_config_qt::reset_tab(&tab.to_string());
    }
}
