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
        // The "Pinned" rail's ROWS, on their own property (a bare HomeCard
        // array). The three documents above carry only an EMPTY `pinned`
        // section, as the ordering slot the discover prefs place — 1:1 with
        // the Slint arm, where the descriptor supplies the position and the
        // `PinnedState.items` global supplies the rows
        // (discover/HomeView.slint:561). Pinning is a per-click mutation, and
        // folding the rows into the tab documents made every click republish
        // all three: every rail's delegate model torn down and rebuilt, and
        // every horizontal scroll reset to 0.
        #[qproperty(QString, pinned_json)]
        // Discover > For You, the two rails whose rows do NOT travel inside
        // the tab documents (src/foryou_qt.rs — same split, same reason as
        // `pinned_json` above): the documents carry an EMPTY ordering slot
        // (`kind: "radio"` / `"spotlight"`) where the prefs place the rail,
        // and these carry the rows. The Spotlight costs one extra
        // /artist/page round trip that lands well after the index, and
        // folding it into the tab documents would republish — and re-create
        // the delegates of — every rail on all three tabs when it did.
        #[qproperty(QString, radio_stations_json)]
        #[qproperty(QString, spotlight_json)]
        // The Qobuz mix landing page (DailyQ / WeeklyQ / FavQ / TopQ), route
        // "mix" — opened from the Qobuz Mixes tiles on For You.
        #[qproperty(QString, mix_json)]
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
        // --- Award landing + "See all" listing (src/award_qt.rs) ----------
        // Awards ride THIS bridge, not one of their own, because they are the
        // label pair's sibling in every respect (same two-route shape, same
        // controller-in-its-own-file split) and the album sidebar already
        // reaches labels through `QbzHome`. One domain, one place to look.
        #[qproperty(QString, award_json)]
        #[qproperty(bool, award_loading)]
        #[qproperty(QString, award_albums_json)]
        #[qproperty(bool, award_albums_loading)]

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
        /// Label header ⋯ → Share. Rust-side because the reference raises a
        /// toast and only Rust can publish `QbzShell.toastJson` — and because
        /// the URL host is per-entity (`play.qobuz.com` for a label), which a
        /// QML call site formatting its own string got wrong.
        #[qinvokable]
        fn label_share(self: Pin<&mut QbzHome>, label_id: QString);

        // --- Discover > For You: Qobuz Mixes / Radio / Spotlight ----------
        /// A Qobuz Mixes tile: open the mix landing page ("daily" | "weekly"
        /// | "fav" | "top"). Slint: `media-action("mix", which, "open")`.
        #[qinvokable]
        fn open_mix(self: Pin<&mut QbzHome>, kind: QString);
        #[qinvokable]
        fn mix_play_all(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn mix_shuffle(self: Pin<&mut QbzHome>);
        /// The header's refresh disc — re-fetch the mix on screen.
        #[qinvokable]
        fn mix_refresh(self: Pin<&mut QbzHome>);
        /// A row click / row Play: the mix becomes the queue, anchored there.
        #[qinvokable]
        fn mix_play_track(self: Pin<&mut QbzHome>, track_id: QString);
        /// A row's context menu: "next" | "later" | "queue".
        #[qinvokable]
        fn mix_enqueue_track(self: Pin<&mut QbzHome>, track_id: QString, mode: QString);
        /// Radio Stations tile — Qobuz `/radio/album` off the seed album.
        #[qinvokable]
        fn start_album_radio(self: Pin<&mut QbzHome>, album_id: QString);
        /// Spotlight RADIO card — the smart `qbz-radio` artist pool.
        #[qinvokable]
        fn start_artist_radio(self: Pin<&mut QbzHome>, artist_id: QString);
        /// Spotlight play / TOP TRACKS card — the artist's popular tracks.
        #[qinvokable]
        fn play_artist_top_tracks(self: Pin<&mut QbzHome>, artist_id: QString);
        /// HomeView MOUNTED: re-hand it every For You cover already resolved.
        ///
        /// `AppShell.qml` binds `viewLoader.source` to `QbzShell.currentView`,
        /// so HomeView is DESTROYED on every navigation and the per-url cover
        /// map it keeps (`forYouArt`) dies with it — while the two rails'
        /// documents are published ONCE per session and deliberately never
        /// republished for artwork (see `foryouArtReady`). Without this the
        /// Radio rail and the Spotlight hero + tiles are blank for the rest of
        /// the session after the first navigation away, and the view's
        /// "something is still pending" probe never goes false, which pins its
        /// 900 ms skeleton pulse Timer on. Idempotent, emits nothing when the
        /// stores hold no resolved path yet (`foryou_qt::resolved_art_patch`).
        #[qinvokable]
        fn refresh_foryou_art(self: Pin<&mut QbzHome>);

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

        // --- Award landing -------------------------------------------------
        /// Album sidebar laurel (with an id), an "Other awards" card, or the
        /// all-awards dropdown. `fallback_name` paints the hero until
        /// `/award/page` answers — and stays as the hero when that endpoint
        /// omits the award, which it may.
        #[qinvokable]
        fn open_award(self: Pin<&mut QbzHome>, award_id: QString, fallback_name: QString);
        /// The album sidebar's other arm: Qobuz gave a NAME and no id.
        /// Resolves through the catalog / an `/award/explore` crawl, then
        /// opens — or toasts, rather than opening a page that cannot load.
        #[qinvokable]
        fn open_award_by_name(self: Pin<&mut QbzHome>, name: QString);
        #[qinvokable]
        fn award_retry(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn award_follow(self: Pin<&mut QbzHome>);
        /// "See All" — push the listing route and load its first page.
        #[qinvokable]
        fn award_open_albums(self: Pin<&mut QbzHome>);
        #[qinvokable]
        fn award_albums_load_more(self: Pin<&mut QbzHome>);
        /// Client-side filter over what is loaded (the reference never
        /// re-queries for this box either).
        #[qinvokable]
        fn award_albums_search(self: Pin<&mut QbzHome>, query: QString);

        /// A batch of For You covers landed: a JSON `{ "<artUrl>": "<file://
        /// path>" }` patch. Per-URL, so the rails patch the TILES instead of
        /// taking a new `radioStationsJson` / `spotlightJson` document — a
        /// republished document hands `model:` a new JS array and
        /// `QQuickItemView::setModel()` resets the rail's scroll offset AND
        /// tears down the QQmlDelegateModel, which is this build's only crash
        /// signature (a null read at libQt6QmlModels[420c5], reproduced live).
        /// Deliberately NOT `QbzLibrary.libraryArtworkReady`: six views listen
        /// to that one and each rebuilds its whole cover map per emit.
        ///
        /// ONE emit per batch, not one per url: the receiving map is a QML
        /// `var`, so it only notifies on a NEW object reference — an emit per
        /// url copied the growing map and invalidated every cover binding in
        /// both rails once per cover (quadratic in the ~38 a cold load
        /// resolves) for no earlier paint, since the whole batch settles inside
        /// one `download_missing().await`. See `foryou_qt::emit_foryou_art`.
        ///
        /// `#[auto_cxx_name]` on this block makes the QML spelling
        /// `foryouArtReady`, i.e. the handler is `onForyouArtReady(patchJson)`.
        #[qsignal]
        fn foryou_art_ready(self: Pin<&mut QbzHome>, patch_json: QString);
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
    pinned_json: QString,
    radio_stations_json: QString,
    spotlight_json: QString,
    mix_json: QString,
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
    award_json: QString,
    award_loading: bool,
    award_albums_json: QString,
    award_albums_loading: bool,
}

impl Default for QbzHomeRust {
    fn default() -> Self {
        Self {
            home_loading: false,
            home_error: QString::default(),
            home_sections_json: QString::from("[]"),
            editor_sections_json: QString::from("[]"),
            for_you_sections_json: QString::from("[]"),
            pinned_json: QString::from("[]"),
            radio_stations_json: QString::from("[]"),
            // "{}" so `JSON.parse(...).visible` is a plain `undefined` on the
            // first frame instead of throwing (the browse-document rule).
            spotlight_json: QString::from("{}"),
            mix_json: QString::from("{}"),
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
            award_json: QString::from("{}"),
            award_loading: false,
            award_albums_json: QString::from("{}"),
            award_albums_loading: false,
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

    /// Straight through to `share_qt` — no nav and no bridge state is touched,
    /// exactly like `QbzArtist::share`.
    pub fn label_share(self: Pin<&mut Self>, label_id: QString) {
        crate::share_qt::share_label(label_id.to_string());
    }

    pub fn open_award(self: Pin<&mut Self>, award_id: QString, fallback_name: QString) {
        crate::award_qt::open_award(award_id.to_string(), fallback_name.to_string());
    }
    pub fn open_award_by_name(self: Pin<&mut Self>, name: QString) {
        crate::award_qt::open_award_by_name(name.to_string());
    }
    pub fn award_retry(self: Pin<&mut Self>) {
        crate::award_qt::retry();
    }
    pub fn award_follow(self: Pin<&mut Self>) {
        crate::award_qt::toggle_follow();
    }
    pub fn award_open_albums(self: Pin<&mut Self>) {
        crate::award_qt::open_albums();
    }
    pub fn award_albums_load_more(self: Pin<&mut Self>) {
        crate::award_qt::albums_load_more();
    }
    pub fn award_albums_search(self: Pin<&mut Self>, query: QString) {
        crate::award_qt::albums_search(query.to_string());
    }

    // --- Discover > For You: mixes / radio / spotlight ---------------------

    pub fn open_mix(self: Pin<&mut Self>, kind: QString) {
        crate::foryou_qt::open_mix(kind.to_string());
    }

    pub fn mix_play_all(self: Pin<&mut Self>) {
        crate::foryou_qt::mix_play_all();
    }

    pub fn mix_shuffle(self: Pin<&mut Self>) {
        crate::foryou_qt::mix_shuffle();
    }

    pub fn mix_refresh(self: Pin<&mut Self>) {
        crate::foryou_qt::refresh_mix();
    }

    pub fn mix_play_track(self: Pin<&mut Self>, track_id: QString) {
        crate::foryou_qt::mix_play_track(track_id.to_string());
    }

    pub fn mix_enqueue_track(self: Pin<&mut Self>, track_id: QString, mode: QString) {
        crate::foryou_qt::mix_enqueue_track(track_id.to_string(), mode.to_string());
    }

    pub fn start_album_radio(self: Pin<&mut Self>, album_id: QString) {
        crate::foryou_qt::start_album_radio(album_id.to_string());
    }

    pub fn start_artist_radio(self: Pin<&mut Self>, artist_id: QString) {
        crate::foryou_qt::start_artist_radio(artist_id.to_string());
    }

    pub fn play_artist_top_tracks(self: Pin<&mut Self>, artist_id: QString) {
        crate::foryou_qt::spotlight_play_top_tracks(artist_id.to_string());
    }

    /// Emitted SYNCHRONOUSLY here rather than through `home_bridge::ui`: this
    /// runs on the Qt thread already (HomeView's `Component.onCompleted`), and
    /// the `ui()` hop silently no-ops until `boot()` has registered the thread,
    /// which is exactly the failure mode this fix exists to remove.
    pub fn refresh_foryou_art(mut self: Pin<&mut Self>) {
        let patch = crate::foryou_qt::resolved_art_patch();
        if patch.is_empty() {
            return;
        }
        self.as_mut()
            .foryou_art_ready(QString::from(patch.as_str()));
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
