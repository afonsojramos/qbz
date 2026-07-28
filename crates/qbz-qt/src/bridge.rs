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
        // Now-playing track id (playing-row indicator in track lists).
        #[qproperty(QString, np_track_id)]

        // --- Lyrics panel (phase 9) ----------------------------------------
        #[qproperty(bool, lyrics_open)]
        // One JSON document (lyrics_qt.rs LyricsDoc: status/lines/synced/
        // provider/error).
        #[qproperty(QString, lyrics_json)]

        // --- Queue panel (phase 9) -----------------------------------------
        // One JSON document (queue_qt.rs QueueDoc: current/upcoming/history
        // + pagination + #442 section markers). Supersedes `queueModel`.
        #[qproperty(QString, queue_json)]

        // --- Sidebar playlist tree ---------------------------------------
        // One JSON document: the flattened entries (folders + playlists,
        // expand/sort/search applied Rust-side — sidebar_qt.rs).
        #[qproperty(QString, sidebar_json)]
        #[qproperty(QString, sidebar_sort_by)]
        #[qproperty(bool, sidebar_sort_asc)]

        // --- Settings (phase 10) -----------------------------------------
        // One JSON document (settings_qt.rs SettingsDoc: audio + playback
        // row states, select option lists, output-device groups).
        #[qproperty(QString, settings_json)]

        // --- Window chrome (phase 12) --------------------------------------
        // The APPLIED titlebar mode (the ui_prefs `use_system_title_bar`
        // value read at startup — drives the window flags; never mutated at
        // runtime, matching the Slint restart semantics).
        #[qproperty(bool, system_title_bar)]
        // The PERSISTED pref as it stands NOW (the app-menu check state;
        // flips live when the user toggles, applies on the next launch).
        #[qproperty(bool, system_title_bar_pref)]

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

        /// Album-card click on Home: resolve the album's tracks, enqueue,
        /// and play through the core's resolved path.
        #[qinvokable]
        fn play_album(self: Pin<&mut QbzBridge>, album_id: QString);

        /// AlbumView header Shuffle.
        #[qinvokable]
        fn play_album_shuffled(self: Pin<&mut QbzBridge>, album_id: QString);
        /// AlbumView row play: the album starting AT this track.
        #[qinvokable]
        fn play_album_from(self: Pin<&mut QbzBridge>, album_id: QString, track_id: QString);
        /// AlbumView row "Play next" ("next") / "Add to queue" ("later").
        #[qinvokable]
        fn enqueue_album_track(self: Pin<&mut QbzBridge>, album_id: QString, track_id: QString, mode: QString);
        /// ArtistView Popular Tracks row play (whole list as the queue).
        #[qinvokable]
        fn play_artist_track(self: Pin<&mut QbzBridge>, track_id: QString);
        /// ArtistView "Play all" (shuffle=false) / "Shuffle all" (true).
        #[qinvokable]
        fn play_artist_top(self: Pin<&mut QbzBridge>, shuffle: bool);
        /// ArtistView ⋯ "Add all to queue" — appends the top-tracks queue.
        #[qinvokable]
        fn enqueue_artist_top(self: Pin<&mut QbzBridge>);

        /// NPB lyrics button / lyrics panel close X.
        #[qinvokable]
        fn toggle_lyrics(self: Pin<&mut QbzBridge>);

        /// Queue panel: tabs/pagination/search.
        #[qinvokable]
        fn queue_set_page(self: Pin<&mut QbzBridge>, page: i32);
        #[qinvokable]
        fn queue_set_search(self: Pin<&mut QbzBridge>, query: QString);
        /// Row actions.
        #[qinvokable]
        fn queue_play_upcoming(self: Pin<&mut QbzBridge>, index: i32);
        #[qinvokable]
        fn queue_remove_upcoming(self: Pin<&mut QbzBridge>, index: i32);
        #[qinvokable]
        fn queue_remove_all_after(self: Pin<&mut QbzBridge>, index: i32);
        #[qinvokable]
        fn queue_move_track(self: Pin<&mut QbzBridge>, from: i32, to: i32);
        #[qinvokable]
        fn queue_play_history(self: Pin<&mut QbzBridge>, index: i32);
        #[qinvokable]
        fn queue_toggle_favorite(self: Pin<&mut QbzBridge>, kind: QString, id: QString);
        /// Footer: Clear queue.
        #[qinvokable]
        fn queue_clear(self: Pin<&mut QbzBridge>);

        /// Sidebar navigation: record a content view ("home" | "library")
        /// and lazy-load its data on first visit.
        #[qinvokable]
        fn navigate_to(self: Pin<&mut QbzBridge>, view: QString);

        /// Track-row click (Library): play the track as a 1-element queue.
        #[qinvokable]
        fn play_track(self: Pin<&mut QbzBridge>, track_id: QString);

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

        /// AlbumCard ⋯ menu: Play next ("next") / Add to queue ("later").
        #[qinvokable]
        fn enqueue_album(self: Pin<&mut QbzBridge>, album_id: QString, mode: QString);

        /// AlbumCard pin badge: toggle pin (album/artist/playlist).
        #[qinvokable]
        fn toggle_pin(self: Pin<&mut QbzBridge>, kind: QString, id: QString, title: QString, subtitle: QString, artwork_url: QString);
        /// Emitted after a pin toggle (`{kind}:{id}` key like artKey).
        #[qsignal]
        fn pin_changed(self: Pin<&mut QbzBridge>, key: QString, value: bool);

        /// Sidebar tree: rebuild + republish after load / sort / search /
        /// folder toggle.
        #[qinvokable]
        fn reload_sidebar(self: Pin<&mut QbzBridge>);
        #[qinvokable]
        fn sidebar_set_sort(self: Pin<&mut QbzBridge>, option: QString);
        #[qinvokable]
        fn sidebar_search(self: Pin<&mut QbzBridge>, query: QString);
        #[qinvokable]
        fn sidebar_toggle_folder(self: Pin<&mut QbzBridge>, id: QString);
        /// Sidebar "+" — create an empty playlist (single core call), then
        /// reload the tree.
        #[qinvokable]
        fn create_playlist(self: Pin<&mut QbzBridge>);
        /// Sidebar cover dispatch: JSON array of cover URLS (the tree's
        /// collage is url-keyed, unlike the feed's artKey).
        #[qinvokable]
        fn sidebar_artwork_window(self: Pin<&mut QbzBridge>, urls_json: QString);

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

        /// App-menu chrome toggle: flip the persisted `use_system_title_bar`
        /// pref (applies on the next launch — the window flags are fixed at
        /// creation, 1:1 Slint). Updates `systemTitleBarPref` only.
        #[qinvokable]
        fn toggle_system_title_bar(self: Pin<&mut QbzBridge>);
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
    library_loading: bool,
    library_error: QString,
    library_json: QString,
    library_counts_json: QString,
    sidebar_json: QString,
    sidebar_sort_by: QString,
    sidebar_sort_asc: bool,
    album_loading: bool,
    album_json: QString,
    artist_loading: bool,
    artist_json: QString,
    view_param_id: QString,
    np_track_id: QString,
    queue_json: QString,
    lyrics_open: bool,
    lyrics_json: QString,
    settings_json: QString,
    system_title_bar: bool,
    system_title_bar_pref: bool,
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
            library_loading: false,
            library_error: QString::default(),
            library_json: QString::from("[]"),
            library_counts_json: QString::from("{}"),
            sidebar_json: QString::from("[]"),
            sidebar_sort_by: QString::from("name"),
            sidebar_sort_asc: true,
            album_loading: false,
            album_json: QString::from("{}"),
            artist_loading: false,
            artist_json: QString::from("{}"),
            view_param_id: QString::default(),
            np_track_id: QString::default(),
            queue_json: QString::from("{}"),
            lyrics_open: false,
            lyrics_json: QString::from("{}"),
            settings_json: QString::from("{}"),
            system_title_bar: crate::settings_qt::use_system_title_bar(),
            system_title_bar_pref: crate::settings_qt::use_system_title_bar(),
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
        crate::transport_toggle_play();
    }

    pub fn next(self: Pin<&mut Self>) {
        crate::transport_next();
    }

    pub fn previous(self: Pin<&mut Self>) {
        crate::transport_previous();
    }

    pub fn seek(self: Pin<&mut Self>, frac: f32) {
        crate::transport_seek(frac);
    }

    pub fn set_volume(self: Pin<&mut Self>, volume: f32) {
        crate::transport_set_volume(volume);
    }

    pub fn toggle_mute(self: Pin<&mut Self>) {
        crate::transport_toggle_mute();
    }

    pub fn toggle_shuffle(self: Pin<&mut Self>) {
        crate::transport_toggle_shuffle();
    }

    pub fn cycle_repeat(self: Pin<&mut Self>) {
        crate::transport_cycle_repeat();
    }

    pub fn reload_home(self: Pin<&mut Self>) {
        crate::reload_home();
    }

    pub fn play_album(self: Pin<&mut Self>, album_id: QString) {
        crate::play_album(album_id.to_string());
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

    pub fn toggle_lyrics(self: Pin<&mut Self>) {
        crate::toggle_lyrics();
    }

    pub fn play_album_shuffled(self: Pin<&mut Self>, album_id: QString) {
        crate::play_album_shuffled(album_id.to_string());
    }

    pub fn play_album_from(self: Pin<&mut Self>, album_id: QString, track_id: QString) {
        if let Ok(tid) = track_id.to_string().parse::<u64>() {
            crate::play_album_from_track(album_id.to_string(), tid);
        }
    }

    pub fn enqueue_album_track(self: Pin<&mut Self>, album_id: QString, track_id: QString, mode: QString) {
        if let Ok(tid) = track_id.to_string().parse::<u64>() {
            crate::enqueue_album_track(album_id.to_string(), tid, mode.to_string());
        }
    }

    pub fn play_artist_track(self: Pin<&mut Self>, track_id: QString) {
        if let Ok(tid) = track_id.to_string().parse::<u64>() {
            crate::play_artist_track(tid);
        }
    }

    pub fn play_artist_top(self: Pin<&mut Self>, shuffle: bool) {
        crate::play_artist_top(shuffle);
    }

    pub fn enqueue_artist_top(self: Pin<&mut Self>) {
        crate::enqueue_artist_top();
    }

    pub fn navigate_to(self: Pin<&mut Self>, view: QString) {
        crate::navigate_to(&view.to_string());
    }

    pub fn play_track(self: Pin<&mut Self>, track_id: QString) {
        if let Ok(id) = track_id.to_string().parse::<u64>() {
            crate::play_track(id);
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

    pub fn enqueue_album(self: Pin<&mut Self>, album_id: QString, mode: QString) {
        crate::enqueue_album(album_id.to_string(), mode.to_string());
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

    pub fn reload_sidebar(self: Pin<&mut Self>) {
        crate::reload_sidebar();
    }

    pub fn sidebar_set_sort(self: Pin<&mut Self>, option: QString) {
        crate::sidebar_set_sort(&option.to_string());
    }

    pub fn sidebar_search(self: Pin<&mut Self>, query: QString) {
        crate::sidebar_set_search(&query.to_string());
    }

    pub fn sidebar_toggle_folder(self: Pin<&mut Self>, id: QString) {
        crate::sidebar_toggle_folder(&id.to_string());
    }

    pub fn create_playlist(self: Pin<&mut Self>) {
        crate::create_playlist();
    }

    pub fn sidebar_artwork_window(self: Pin<&mut Self>, urls_json: QString) {
        crate::sidebar_artwork_window(urls_json.to_string());
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

    pub fn toggle_system_title_bar(self: Pin<&mut Self>) {
        crate::toggle_system_title_bar();
    }
}
