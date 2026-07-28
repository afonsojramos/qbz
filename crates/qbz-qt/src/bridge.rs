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
        // --- Library view --------------------------------------------------
        #[qproperty(bool, library_loading)]
        #[qproperty(QString, library_error)]
        // One JSON document: the FULL merged feed (tabs/search/sort/source
        // filters derive QML-side from the parsed array — see library_qt.rs
        // for the rationale + the measured parse cost).
        #[qproperty(QString, library_json)]
        // {tracks, albums, artists, playlists, labels, all} — tab badges.
        #[qproperty(QString, library_counts_json)]

        // --- Album / Artist detail views (phase 8) -------------------------
        #[qproperty(bool, album_loading)]
        #[qproperty(QString, album_json)]
        #[qproperty(bool, artist_loading)]
        #[qproperty(QString, artist_json)]
        // The id backing the current "album"/"artist" view ("" = none).
        #[qproperty(QString, view_param_id)]

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

        type QbzBridge = super::QbzBridgeRust;

        /// TEMP (phase 23 split): registers the QbzBridge Qt-thread
        /// hop while the remaining domains still live here. Removed
        /// when the last domain moves out.
        #[qinvokable]
        fn boot(self: Pin<&mut QbzBridge>);

        /// Library retry button / manual refresh.
        #[qinvokable]
        fn reload_library(self: Pin<&mut QbzBridge>);

        /// Windowed artwork: the grid/list reports the mounted window as a
        /// JSON array of artKeys (tab views filter the feed, so raw indices
        /// don't map); Rust dispatches covers for those keys (id-keyed via
        /// `libraryArtworkReady`).
        #[qinvokable]
        fn library_artwork_window(self: Pin<&mut QbzBridge>, keys_json: QString);

        /// Card heart: toggle favorite (Qobuz API or the local store,
        /// routed by id shape); the result arrives via
        /// `libraryFavoriteChanged`.
        #[qinvokable]
        fn library_toggle_favorite(self: Pin<&mut QbzBridge>, kind: QString, id: QString);

        /// Emitted when a dispatched cover lands on disk (id-keyed —
        /// `{kind}:{id}`); QML updates its artwork map.
        #[qsignal]
        fn library_artwork_ready(self: Pin<&mut QbzBridge>, key: QString, path: QString);

        /// Emitted after a heart toggle (optimistic flip + rollback).
        #[qsignal]
        fn library_favorite_changed(self: Pin<&mut QbzBridge>, key: QString, value: bool);

        /// AlbumCard pin badge: toggle pin (album/artist/playlist).
        #[qinvokable]
        fn toggle_pin(self: Pin<&mut QbzBridge>, kind: QString, id: QString, title: QString, subtitle: QString, artwork_url: QString);
        /// Emitted after a pin toggle (`{kind}:{id}` key like artKey).
        #[qsignal]
        fn pin_changed(self: Pin<&mut QbzBridge>, key: QString, value: bool);

        /// Open the album detail view (pushes "album" on the nav stack).
        #[qinvokable]
        fn open_album(self: Pin<&mut QbzBridge>, album_id: QString);
        /// Open the artist detail view (pushes "artist" on the nav stack).
        #[qinvokable]
        fn open_artist(self: Pin<&mut QbzBridge>, artist_id: QString);
        /// ArtistView per-section "Load more" — the next releases page.
        #[qinvokable]
        fn load_release_section(self: Pin<&mut QbzBridge>, artist_id: QString, release_type: QString, offset: i32);
        /// Emitted with the next page of a releases bucket.
        #[qsignal]
        fn release_section_ready(self: Pin<&mut QbzBridge>, release_type: QString, cards_json: QString, has_more: bool);

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
    library_loading: bool,
    library_error: QString,
    library_json: QString,
    library_counts_json: QString,
    album_loading: bool,
    album_json: QString,
    artist_loading: bool,
    artist_json: QString,
    view_param_id: QString,
    settings_json: QString,
    cortinilla_open: bool,
    cortinilla_loading: bool,
    cortinilla_json: QString,
    selected_index: i32,
    cortinilla_scroll_y: f32,
    search_json: QString,
    intelligent_search: bool,
    playlist_json: QString,
}

impl Default for QbzBridgeRust {
    fn default() -> Self {
        Self {
            library_loading: false,
            library_error: QString::default(),
            library_json: QString::from("[]"),
            library_counts_json: QString::from("{}"),
            album_loading: false,
            album_json: QString::from("{}"),
            artist_loading: false,
            artist_json: QString::from("{}"),
            view_param_id: QString::default(),
            settings_json: QString::from("{}"),
            cortinilla_open: false,
            cortinilla_loading: false,
            cortinilla_json: QString::from("{}"),
            selected_index: -1,
            cortinilla_scroll_y: 0.0,
            search_json: QString::from("{}"),
            intelligent_search: crate::search_qt::intelligent_search_pref(),
            playlist_json: QString::from("{}"),
        }
    }
}

impl qbz_bridge::QbzBridge {
    pub fn boot(self: Pin<&mut Self>) {
        crate::register_qt_thread(self.qt_thread());
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

    pub fn open_album(self: Pin<&mut Self>, album_id: QString) {
        crate::open_album(album_id.to_string());
    }

    pub fn open_artist(self: Pin<&mut Self>, artist_id: QString) {
        crate::open_artist(artist_id.to_string());
    }

    pub fn load_release_section(self: Pin<&mut Self>, artist_id: QString, release_type: QString, offset: i32) {
        crate::load_release_section(artist_id.to_string(), release_type.to_string(), offset);
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
}
