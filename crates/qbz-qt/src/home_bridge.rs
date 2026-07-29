//! QbzHome — Discover > Home domain bridge (phase 23 split of the QbzBridge
//! God-object; the pattern is documented in main.rs). Props: the three tab
//! section documents + loading/error. Invokable: reloadHome (the retry
//! button / manual refresh).
//!
//! It also carries the six pages reachable FROM Home (the rails' "View all"
//! links and the label funnel), because they are the same domain and a new
//! `#[cxx_qt::bridge]` singleton would need its own `build.rs` `rust_files`
//! entry and its own `Main.qml` boot line — both forbidden files:
//!   `discoverBrowse*`  -> src/browse_qt.rs  (route "discoverbrowse")
//!   `playlistBrowse*`  -> src/browse_qt.rs  ("playlistbrowse")
//!   `playHistory*`     -> src/browse_qt.rs  ("recentalbums" / "mostplayedalbums")
//!   `label*`           -> src/label_qt.rs   ("label")
//!   `labelReleases*`   -> src/label_qt.rs   ("labelreleases")

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
        // --- "View all" full-list pages (src/browse_qt.rs) ----------------
        #[qproperty(QString, discover_browse_json)]
        #[qproperty(bool, discover_browse_loading)]
        #[qproperty(bool, discover_browse_loading_more)]
        #[qproperty(QString, playlist_browse_json)]
        #[qproperty(bool, playlist_browse_loading)]
        #[qproperty(bool, playlist_browse_loading_more)]
        // Recently Played Albums + Most Played Albums share ONE document and
        // ONE view; `mode` on the document arms the differences.
        #[qproperty(QString, play_history_json)]
        #[qproperty(bool, play_history_loading)]
        // --- Label landing + releases sub-view (src/label_qt.rs) ----------
        #[qproperty(QString, label_json)]
        #[qproperty(bool, label_loading)]
        #[qproperty(QString, label_releases_json)]
        #[qproperty(bool, label_releases_loading)]

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

        // --- DiscoverBrowse ("View all" on a generic album carousel) ------
        #[qinvokable]
        fn open_discover_browse(self: Pin<&mut QbzHome>, endpoint: QString, title: QString);
        #[qinvokable]
        fn discover_browse_load_more(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn discover_browse_search(self: Pin<&mut QbzHome>, query: QString);
        #[qinvokable]
        fn discover_browse_set_view_mode(self: Pin<&mut QbzHome>, mode: QString);
        /// The shared "discover" genre selection changed while the page is
        /// mounted — re-fetch from offset 0.
        #[qinvokable]
        fn discover_browse_genre_changed(self: Pin<&mut QbzHome>);

        // --- PlaylistBrowse ("View all" on the Qobuz Playlists rail) ------
        #[qinvokable]
        fn open_playlist_browse(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn playlist_browse_load_more(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn playlist_browse_search(self: Pin<&mut QbzHome>, query: QString);
        #[qinvokable]
        fn playlist_browse_select_tag(self: Pin<&mut QbzHome>, slug: QString);
        #[qinvokable]
        fn playlist_browse_set_view_mode(self: Pin<&mut QbzHome>, mode: QString);
        #[qinvokable]
        fn playlist_browse_genre_changed(self: Pin<&mut QbzHome>);

        // --- Play history -------------------------------------------------
        #[qinvokable]
        fn open_recent_albums(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn open_most_played_albums(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn most_played_filter(self: Pin<&mut QbzHome>, query: QString);

        // --- Label landing -------------------------------------------------
        #[qinvokable]
        fn open_label(self: Pin<&mut QbzHome>, label_id: QString);
        #[qinvokable]
        fn label_follow(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn label_play_top(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn label_shuffle(self: Pin<&mut QbzHome>);
        /// Popular Tracks row click — the whole list becomes the queue.
        #[qinvokable]
        fn label_play_track(self: Pin<&mut QbzHome>, track_id: QString);
        /// Popular Tracks row menu: "next" | "later" | "queue".
        #[qinvokable]
        fn label_enqueue_track(self: Pin<&mut QbzHome>, track_id: QString, mode: QString);
        /// The two ⋯ menus' all-tracks queue entries.
        #[qinvokable]
        fn label_top_action(self: Pin<&mut QbzHome>, mode: QString);
        #[qinvokable]
        fn label_open_releases(self: Pin<&mut QbzHome>);

        // --- Label releases sub-view --------------------------------------
        #[qinvokable]
        fn label_releases_load_more(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn label_releases_set_sort(self: Pin<&mut QbzHome>, sort: QString);
        #[qinvokable]
        fn label_releases_set_hires(self: Pin<&mut QbzHome>, on: bool);
        #[qinvokable]
        fn label_releases_set_group(self: Pin<&mut QbzHome>, on: bool);
        #[qinvokable]
        fn label_releases_search(self: Pin<&mut QbzHome>, query: QString);
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
    discover_browse_json: QString,
    discover_browse_loading: bool,
    discover_browse_loading_more: bool,
    playlist_browse_json: QString,
    playlist_browse_loading: bool,
    playlist_browse_loading_more: bool,
    play_history_json: QString,
    play_history_loading: bool,
    label_json: QString,
    label_loading: bool,
    label_releases_json: QString,
    label_releases_loading: bool,
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
            // "{}" so the views' JSON.parse never throws on the first frame.
            discover_browse_json: QString::from("{}"),
            discover_browse_loading: false,
            discover_browse_loading_more: false,
            playlist_browse_json: QString::from("{}"),
            playlist_browse_loading: false,
            playlist_browse_loading_more: false,
            play_history_json: QString::from("{}"),
            play_history_loading: false,
            label_json: QString::from("{}"),
            label_loading: false,
            label_releases_json: QString::from("{}"),
            label_releases_loading: false,
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

    // --- DiscoverBrowse ---------------------------------------------------

    pub fn open_discover_browse(self: Pin<&mut Self>, endpoint: QString, title: QString) {
        crate::browse_qt::open_discover_browse(endpoint.to_string(), title.to_string());
    }

    pub fn discover_browse_load_more(self: Pin<&mut Self>) {
        crate::browse_qt::discover_browse_load_more();
    }

    pub fn discover_browse_search(self: Pin<&mut Self>, query: QString) {
        crate::browse_qt::discover_browse_search(query.to_string());
    }

    pub fn discover_browse_set_view_mode(self: Pin<&mut Self>, mode: QString) {
        crate::browse_qt::discover_browse_set_view_mode(mode.to_string());
    }

    pub fn discover_browse_genre_changed(self: Pin<&mut Self>) {
        crate::browse_qt::discover_browse_genre_changed();
    }

    // --- PlaylistBrowse ---------------------------------------------------

    pub fn open_playlist_browse(self: Pin<&mut Self>) {
        crate::browse_qt::open_playlist_browse();
    }

    pub fn playlist_browse_load_more(self: Pin<&mut Self>) {
        crate::browse_qt::playlist_browse_load_more();
    }

    pub fn playlist_browse_search(self: Pin<&mut Self>, query: QString) {
        crate::browse_qt::playlist_browse_search(query.to_string());
    }

    pub fn playlist_browse_select_tag(self: Pin<&mut Self>, slug: QString) {
        crate::browse_qt::playlist_browse_select_tag(slug.to_string());
    }

    pub fn playlist_browse_set_view_mode(self: Pin<&mut Self>, mode: QString) {
        crate::browse_qt::playlist_browse_set_view_mode(mode.to_string());
    }

    pub fn playlist_browse_genre_changed(self: Pin<&mut Self>) {
        crate::browse_qt::playlist_browse_genre_changed();
    }

    // --- Play history ------------------------------------------------------

    pub fn open_recent_albums(self: Pin<&mut Self>) {
        crate::browse_qt::open_recent_albums();
    }

    pub fn open_most_played_albums(self: Pin<&mut Self>) {
        crate::browse_qt::open_most_played_albums();
    }

    pub fn most_played_filter(self: Pin<&mut Self>, query: QString) {
        crate::browse_qt::most_played_filter(query.to_string());
    }

    // --- Label -------------------------------------------------------------

    pub fn open_label(self: Pin<&mut Self>, label_id: QString) {
        crate::label_qt::open_label(label_id.to_string());
    }

    pub fn label_follow(self: Pin<&mut Self>) {
        crate::label_qt::toggle_follow();
    }

    pub fn label_play_top(self: Pin<&mut Self>) {
        crate::label_qt::play_top(false);
    }

    pub fn label_shuffle(self: Pin<&mut Self>) {
        crate::label_qt::play_top(true);
    }

    pub fn label_play_track(self: Pin<&mut Self>, track_id: QString) {
        crate::label_qt::play_track(track_id.to_string());
    }

    pub fn label_enqueue_track(self: Pin<&mut Self>, track_id: QString, mode: QString) {
        crate::label_qt::enqueue_track(track_id.to_string(), mode.to_string());
    }

    pub fn label_top_action(self: Pin<&mut Self>, mode: QString) {
        crate::label_qt::enqueue_top(mode.to_string());
    }

    pub fn label_open_releases(self: Pin<&mut Self>) {
        crate::label_qt::open_releases();
    }

    // --- Label releases ----------------------------------------------------

    pub fn label_releases_load_more(self: Pin<&mut Self>) {
        crate::label_qt::releases_load_more();
    }

    pub fn label_releases_set_sort(self: Pin<&mut Self>, sort: QString) {
        crate::label_qt::releases_set_sort(sort.to_string());
    }

    pub fn label_releases_set_hires(self: Pin<&mut Self>, on: bool) {
        crate::label_qt::releases_set_hires(on);
    }

    pub fn label_releases_set_group(self: Pin<&mut Self>, on: bool) {
        crate::label_qt::releases_set_group(on);
    }

    pub fn label_releases_search(self: Pin<&mut Self>, query: QString) {
        crate::label_qt::releases_search(query.to_string());
    }
}
