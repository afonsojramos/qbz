//! QbzLocal — the Local Library domain bridge (the phase-23 per-domain
//! pattern, modelled on `home_bridge.rs`: ONE `#[cxx_qt::bridge]` mod, ONE
//! `#[qml_element] #[qml_singleton]` QObject, its own
//! `OnceLock<CxxQtThread>`, a `boot()` invokable and a `pub(crate) fn ui()`).
//!
//! Every property is ONE JSON document (the `library_qt.rs` transport
//! rationale): the QML view parses it once per publish and derives its
//! search / sort / grouping in JS. Artwork never rides the document — it is
//! id-keyed through `localArtworkReady`, windowed by the grid/list, and
//! evicted QML-side, which is what keeps a 16K-track library from decoding
//! covers it never shows.
//!
//! The invokables stay one-line forwards into the `local_*` modules; the
//! blocking DB work runs on `spawn_blocking` so the Qt event loop is never
//! touched by a rusqlite call, and the Plex network work runs on the tokio
//! runtime.

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

use crate::local_bridge_ops::{
    emit_artwork, emit_artwork_one, invalidate_artists, load_tab_impl, load_tracks,
    publish_plex_state, publish_tree, reload_browse, run_sync,
};
use crate::local_library_qt as lib;
use crate::local_plex as plex;

#[cxx_qt::bridge]
pub mod qbz_local {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Availability / chrome ----------------------------------------
        /// False when there is no per-user library.db, no registered folder
        /// AND no cached Plex content — the view then shows the "nothing
        /// indexed yet" state with the route into Settings > Local Library.
        #[qproperty(bool, local_available)]
        /// {albums, artists, folders, tracks, plexTracks} — the tab badges.
        #[qproperty(QString, local_counts_json)]
        /// Album identity: "folder" | "metadata" (persisted; shared with
        /// the Slint frontend's locallibrary_ui.json).
        #[qproperty(QString, local_album_mode)]

        // --- Albums tab ---------------------------------------------------
        #[qproperty(bool, local_albums_loading)]
        #[qproperty(QString, local_albums_error)]
        #[qproperty(QString, local_albums_json)]

        // --- Artists tab --------------------------------------------------
        #[qproperty(bool, local_artists_loading)]
        #[qproperty(QString, local_artists_json)]

        // --- Folders tab, FLAT mode ---------------------------------------
        #[qproperty(bool, local_folders_loading)]
        #[qproperty(QString, local_folders_json)]

        // --- Folders tab, TREE mode ---------------------------------------
        #[qproperty(bool, local_tree_loading)]
        /// The FLATTENED, search-filtered visible tree — a plain array the
        /// rail windows with a ListView (never a recursive component).
        #[qproperty(QString, local_tree_json)]
        /// Tracks selected in the tree rail — the bulk bar's counter and its
        /// visibility gate.
        #[qproperty(i32, local_tree_selected_count)]
        #[qproperty(bool, local_detail_loading)]
        /// {path, name, trackCount, subfolders[], tracks[]}
        #[qproperty(QString, local_detail_json)]

        // --- Tracks tab (server-paginated) --------------------------------
        #[qproperty(bool, local_tracks_loading)]
        #[qproperty(bool, local_tracks_loading_more)]
        #[qproperty(bool, local_tracks_has_more)]
        #[qproperty(QString, local_tracks_sort)]
        /// Tracks-tab grouping: "off" | "album" | "artist" | "name". A
        /// CLIENT-side visual reorder (unlike the sort, which is the SQL
        /// ORDER BY), but persisted in the SAME locallibrary_ui.json key the
        /// Slint writes — so it survives a restart and both frontends agree.
        #[qproperty(QString, local_tracks_group)]
        #[qproperty(QString, local_tracks_json)]

        // --- Local album detail (the album pane) ---------------------------
        #[qproperty(bool, local_album_loading)]
        /// {album:{...}, tracks:[...]} — "" while nothing is open.
        #[qproperty(QString, local_album_json)]
        /// Mirror of AppearanceState.local-library-track-artwork (ui_prefs,
        /// default OFF). OFF is the 16k-row freeze guard — keep the default.
        #[qproperty(bool, local_track_artwork)]
        /// Artist NAME route from the routed local album page into the Artists
        /// tab (local/Plex artists carry no catalog id). "" once consumed.
        #[qproperty(QString, local_pending_artist)]
        /// A pending ROUTE into this view: which tab to show, and an optional
        /// query to pre-filter it with. Set by the cortinilla's local "View
        /// more" links, consumed by LocalLibraryView on mount and on change,
        /// then cleared — the property CHANGE is the trigger, so it has to be
        /// released or the same route cannot fire twice in a row. JSON:
        /// {"tab":"albums|artists|tracks","query":"..."}.
        #[qproperty(QString, local_pending_route)]
        // --- Ephemeral folder (an ad-hoc folder outside the index) ---------
        /// A session is open: its OWN tab appears in Local Library.
        #[qproperty(bool, local_ephemeral_active)]
        /// The folder is being scanned (metadata + CUE + artwork).
        #[qproperty(bool, local_ephemeral_loading)]
        /// {name, path, trackCount, multiAlbum, albums:[…]} — "" while closed.
        #[qproperty(QString, local_ephemeral_json)]
        /// The session's DISPLAY NAME — what the tab and the nav flyout call
        /// it. Computed once here rather than derived twice in QML: the view
        /// already parses the document, but `NavFlyout` does not, and a second
        /// derivation is a second thing that can drift.
        ///
        /// The name is the CONTENT, never a verb: "Now Playing" would be false
        /// the moment a folder is open while something else plays.
        #[qproperty(QString, local_ephemeral_label)]
        /// Bumped ONCE per user-initiated open, and never by the boot restore.
        ///
        /// `local_ephemeral_active` cannot carry this: it is already `true`
        /// when you open a SECOND folder over a first, so a handler watching
        /// it never fires and the view stays on whatever tab you were on. A
        /// sequence changes on every open, which is what "take me to what I
        /// just opened" actually needs.
        #[qproperty(i32, local_ephemeral_open_seq)]
        /// The open session came from a physical CD, so it can be RIPPED.
        /// Derived from the rows themselves rather than passed in — a flag
        /// somebody has to remember to set is a flag that eventually is not.
        #[qproperty(bool, local_ephemeral_is_cd)]
        /// A rip is running; the pane shows progress instead of the button.
        #[qproperty(bool, local_rip_active)]
        /// "3/7 · 45%" — already formatted, because the number of things that
        /// can disagree about how to format it is otherwise the number of
        /// places that show it.
        #[qproperty(QString, local_rip_progress)]

        // --- Plex ----------------------------------------------------------
        /// Master toggle (Settings > Local Library > Plex). Drives whether
        /// the browse union includes Plex at all + the source filter chip.
        #[qproperty(bool, plex_enabled)]
        /// enabled + LAN address + resolved base url + token — the Slint's
        /// `plex-available`: gates the header Sync button and every request
        /// that leaves the process.
        #[qproperty(bool, plex_available)]
        /// A manual sync is in flight (`plex-syncing`) — the Sync button's
        /// busy state.
        #[qproperty(bool, plex_syncing)]
        /// Media-server sweep state. TWO properties rather than one, because
        /// the two questions have different answers: `media_syncing` gates the
        /// spinner, and `media_sync_progress` ("1500/4924") is what makes a
        /// 45.8-second Jellyfin sweep legible instead of a frozen button.
        /// Whether each media server is CONFIGURED — what gates its source
        /// chip in the Local Library filter popup. A chip that can never match
        /// anything is a control that teaches the user the filter is broken,
        /// so the chip only exists when its server does.
        #[qproperty(bool, media_has_jellyfin)]
        #[qproperty(bool, media_has_subsonic)]
        /// The Albums funnel, as a JSON object of the ticked keys
        /// (`{"hires":true,"jellyfin":true}`). Empty string = no filter.
        ///
        /// It lives on the BRIDGE, not on `LocalLibraryView`, for the reason
        /// the owner reported: navigating away DESTROYS the view (see
        /// `project_qt_nav_eager_tabs` — Qt, Slint and Tauri all do), so a
        /// view-local `property var filter` cannot survive a round trip to
        /// Discover, let alone a restart. The bridge outlives the view and
        /// mirrors `ui_prefs.json`, so one property buys BOTH kinds of
        /// persistence the owner asked for.
        #[qproperty(QString, albums_filter)]
        #[qproperty(bool, media_syncing)]
        #[qproperty(QString, media_sync_progress)]
        /// Tracks written by the last sync; -1 = never synced this session.
        #[qproperty(i32, plex_last_sync_tracks)]
        /// Raw error text from the last Plex operation ("" when fine). QML
        /// wraps it in its own translated frame.
        #[qproperty(QString, plex_error)]
        /// [{key, title, selected}] — the cached library sections + the
        /// user's selection (Settings > Local Library > Plex).
        #[qproperty(QString, plex_sections_json)]
        /// --- PIN sign-in (plex_pin_qt.rs) --------------------------------
        /// The outstanding link code ("" when none). Its presence is what
        /// mounts the "Link code" row, 1:1 with the reference's
        /// `if PlexSettingsState.pin-code != ""`
        /// (LocalLibrarySettings.slint:638).
        #[qproperty(QString, pin_code)]
        /// The plex.tv sign-in url for the "Open Plex sign-in" button.
        #[qproperty(QString, pin_auth_url)]
        /// The pin/start request is in flight ("Working...").
        #[qproperty(bool, pin_busy)]

        type QbzLocal = super::QbzLocalRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzLocal>);

        /// Mount / retry for one tab ("albums" | "artists" | "folders" |
        /// "tracks"). Idempotent per publish: the view calls it on mount and
        /// on tab change; a reload is cheap because every tab is one query.
        #[qinvokable]
        fn load_tab(self: Pin<&mut QbzLocal>, tab: QString);

        /// Album identity dropdown ("folder" | "metadata"): persist + reload
        /// the album set (the grouping IS the query).
        #[qinvokable]
        fn set_album_mode(self: Pin<&mut QbzLocal>, mode: QString);

        // --- Tracks tab ----------------------------------------------------
        /// Toolbar search (server-side; resets to page 1).
        #[qinvokable]
        fn tracks_search(self: Pin<&mut QbzLocal>, query: QString);
        /// Toolbar sort (SQL ORDER BY; resets to page 1).
        #[qinvokable]
        fn tracks_set_sort(self: Pin<&mut QbzLocal>, sort: QString);
        /// Toolbar grouping ("off" | "album" | "artist" | "name"): persist +
        /// republish. NO re-query — the group modes reorder what is already
        /// loaded, on top of the SQL sort.
        #[qinvokable]
        fn tracks_set_group(self: Pin<&mut QbzLocal>, mode: QString);
        /// Infinite scroll: append the next page.
        #[qinvokable]
        fn tracks_load_more(self: Pin<&mut QbzLocal>);

        // --- Folder tree ---------------------------------------------------
        /// Chevron: expand (lazy one-level fetch) or collapse (pure UI).
        #[qinvokable]
        fn tree_toggle(self: Pin<&mut QbzLocal>, path: QString, expand: bool);
        /// Rail header: collapse every expanded folder.
        #[qinvokable]
        fn tree_collapse_all(self: Pin<&mut QbzLocal>);
        /// Rail search box (filters the visible set, keeps ancestors).
        #[qinvokable]
        fn tree_search(self: Pin<&mut QbzLocal>, query: QString);
        /// Rail header: enter / leave multi-select. Leaving DROPS the selection.
        #[qinvokable]
        fn tree_set_select_mode(self: Pin<&mut QbzLocal>, on: bool);
        /// Folder checkbox: toggle every track under it, recursively.
        #[qinvokable]
        fn tree_toggle_folder_select(self: Pin<&mut QbzLocal>, path: QString);
        /// Track checkbox: toggle one row by file path.
        #[qinvokable]
        fn tree_toggle_track_select(self: Pin<&mut QbzLocal>, path: QString);
        /// Tree-rail bulk bar.
        #[qinvokable]
        fn folders_bulk_action(self: Pin<&mut QbzLocal>, action: QString);
        /// Albums-grid / Tracks-table bulk bar. scope = "album" | "track".
        #[qinvokable]
        fn bulk_action(self: Pin<&mut QbzLocal>, scope: QString, ids_json: QString, action: QString);
        /// Row body: select a folder and load its detail pane.
        #[qinvokable]
        fn select_folder(self: Pin<&mut QbzLocal>, path: QString);

        // --- Album detail ---------------------------------------------------
        /// Album card click: load the local/Plex album detail (header +
        /// tracks). A `plex:<hash>` id is served from the Plex cache.
        #[qinvokable]
        fn open_album(self: Pin<&mut QbzLocal>, id: QString);
        /// Close the album pane (back to the grid).
        #[qinvokable]
        fn close_album(self: Pin<&mut QbzLocal>);
        // --- Local album actions -------------------------------------------
        /// The view consumed the pending artist-name route.
        #[qinvokable]
        fn clear_pending_artist(self: Pin<&mut QbzLocal>);
        /// The view applied the pending route — release it.
        #[qinvokable]
        fn clear_pending_route(self: Pin<&mut QbzLocal>);
        /// "Go to artist" on a local/Plex album — a NAME route, not an id.
        #[qinvokable]
        fn open_artist_by_name(self: Pin<&mut QbzLocal>, name: QString);
        /// Artists tab, right pane: the ids of the CACHED album rows that
        /// credit `artist`, as a JSON array (PARITY-DEBT #8). SYNCHRONOUS and
        /// cheap — it is a pass over the album document already in memory, and
        /// QML uses it to filter the very array it renders.
        #[qinvokable]
        fn artist_album_ids(self: &QbzLocal, artist: QString) -> QString;
        /// Album header pencil — LOGGED SEAM (no tag-editor modal yet).
        #[qinvokable]
        fn album_edit_tags(self: Pin<&mut QbzLocal>, id: QString);
        /// Album header list-plus — LOGGED SEAM (no playlist picker yet).
        #[qinvokable]
        fn album_add_to_playlist(self: Pin<&mut QbzLocal>, id: QString);
        /// Album header cassette — LOGGED SEAM (no Mixtape store yet).
        #[qinvokable]
        fn album_add_to_mixtape(self: Pin<&mut QbzLocal>, id: QString);
        /// Version picker: switch the shown physical copy.
        #[qinvokable]
        fn album_select_version(self: Pin<&mut QbzLocal>, index: i32);
        /// Per-disc menu: "play" | "next" | "later" | "queue".
        #[qinvokable]
        fn album_disc_action(self: Pin<&mut QbzLocal>, disc: i32, action: QString);
        // --- Ephemeral folder ------------------------------------------------
        /// Pick a folder OUTSIDE the library, scan it, show the pane.
        #[qinvokable]
        fn ephemeral_open(self: Pin<&mut QbzLocal>);
        /// Same, for a KNOWN path (no picker).
        #[qinvokable]
        fn ephemeral_open_path(self: Pin<&mut QbzLocal>, path: QString);
        /// Close the session (stops playback if it came from the session).
        #[qinvokable]
        fn ephemeral_clear(self: Pin<&mut QbzLocal>);
        /// `Open > Open CD…`: read the audio disc in the drive and make it the
        /// ephemeral session. Toasts on every failure it can name (no drive,
        /// no disc, a data-only disc) rather than opening an empty pane.
        #[qinvokable]
        fn ephemeral_open_cd(self: Pin<&mut QbzLocal>);
        /// `Open > Open SACD image…`: pick a .iso and play its stereo area.
        #[qinvokable]
        fn ephemeral_open_sacd(self: Pin<&mut QbzLocal>);
        /// Rip the open CD into a folder the user picks. No-op unless the
        /// session is a disc.
        #[qinvokable]
        fn rip_disc(self: Pin<&mut QbzLocal>);
        /// Header Play / Shuffle: the whole session becomes the queue. Same
        /// `(id, shuffle)` shape as `play_album`, so the two headers behave
        /// identically.
        #[qinvokable]
        fn ephemeral_play_all(self: Pin<&mut QbzLocal>, shuffle: bool);
        /// Per-album Play (multi-album sessions only).
        #[qinvokable]
        fn ephemeral_play_album(self: Pin<&mut QbzLocal>, group_key: QString);
        /// Track row click: the track's album block becomes the queue.
        #[qinvokable]
        fn ephemeral_play_track(self: Pin<&mut QbzLocal>, id: QString);

        // --- Playback --------------------------------------------------------
        /// Album card / detail header Play (optionally shuffled).
        #[qinvokable]
        fn play_album(self: Pin<&mut QbzLocal>, id: QString, shuffle: bool);
        /// Album-detail row click: play the album from that track.
        #[qinvokable]
        fn play_album_track(self: Pin<&mut QbzLocal>, id: QString, track_id: QString);
        /// Tree detail header Play: the whole subtree becomes the queue.
        #[qinvokable]
        fn play_folder(self: Pin<&mut QbzLocal>, path: QString);
        /// Tree detail row click: the folder's DIRECT tracks become the
        /// queue, starting at the clicked row.
        #[qinvokable]
        fn play_folder_track(self: Pin<&mut QbzLocal>, path: QString, track_id: QString);
        /// Tracks-tab row click: the loaded page set becomes the queue, in the
        /// order the tab is RENDERING it (PARITY-DEBT #14).
        /// `visible_ids_json` = the JSON array of the ids on screen, in render
        /// order; `track_id` = the row that was clicked.
        #[qinvokable]
        fn play_tracks_visible(
            self: Pin<&mut QbzLocal>,
            visible_ids_json: QString,
            track_id: QString,
        );
        /// Context menus: kind = "track" | "album" | "folder";
        /// mode = "next" | "later" | "queue".
        #[qinvokable]
        fn enqueue(self: Pin<&mut QbzLocal>, kind: QString, id: QString, mode: QString);

        // --- Plex ------------------------------------------------------------
        /// Header Sync button (#573): re-fetch the Plex sections + tracks
        /// into the shared cache DB, then reload the browse documents in
        /// place. No-op when `plexAvailable` is false.
        /// ── Media servers (Jellyfin / Subsonic) ─────────────────────────
        ///
        /// One set of invokables for both, discriminated by a `server` word
        /// ("jellyfin" | "subsonic"), because the panel is the same form twice
        /// and the store behind it is one table. An unknown word is refused
        /// with a toast rather than silently ignored.
        ///
        /// TEST the address before asking for a password: it separates "wrong
        /// address" from "wrong password", which is the difference between a
        /// user fixing a typo and a user thinking the feature is broken.
        #[qinvokable]
        fn media_test(self: Pin<&mut QbzLocal>, server: QString, url: QString);
        /// Persist the Albums funnel. See the `albums_filter` qproperty.
        #[qinvokable]
        fn set_albums_filter_json(self: Pin<&mut QbzLocal>, json: QString);
        /// Authenticate and persist. Runs a first sweep on success.
        #[qinvokable]
        fn media_connect(
            self: Pin<&mut QbzLocal>,
            server: QString,
            url: QString,
            username: QString,
            password: QString,
        );
        /// Master toggle. OFF collapses the union back immediately; the cache
        /// is KEPT so turning it on again does not re-sweep.
        #[qinvokable]
        fn media_set_enabled(self: Pin<&mut QbzLocal>, server: QString, enabled: bool);
        /// Sweep now. `full` forces a complete pass instead of a delta.
        #[qinvokable]
        fn media_sync(self: Pin<&mut QbzLocal>, server: QString, full: bool);
        /// Sign out: clear credentials and purge this server's cached rows.
        #[qinvokable]
        fn media_disconnect(self: Pin<&mut QbzLocal>, server: QString);
        #[qinvokable]
        fn sync_plex(self: Pin<&mut QbzLocal>);
        /// Master toggle. Turning it OFF collapses the browse union back to
        /// local-only immediately (the cache DB is kept).
        #[qinvokable]
        fn plex_set_enabled(self: Pin<&mut QbzLocal>, enabled: bool);
        /// Manual connect: resolve + persist `proto://host:32400` + token,
        /// then run a first sync. LAN addresses only.
        #[qinvokable]
        fn plex_connect(self: Pin<&mut QbzLocal>, server_url: QString, token: QString);
        /// Sign out of Plex: clear creds/sections and purge the cache DB,
        /// then reload the (now local-only) browse documents.
        #[qinvokable]
        fn plex_disconnect(self: Pin<&mut QbzLocal>);
        /// Persist the selected library section keys (JSON array of strings)
        /// and re-sync so the cache matches the selection.
        #[qinvokable]
        fn set_plex_sections(self: Pin<&mut QbzLocal>, keys_json: QString);
        /// Re-read the Plex gates + sections from the store (call after the
        /// settings panel changed something out of band).
        #[qinvokable]
        fn refresh_plex(self: Pin<&mut QbzLocal>);
        /// Is this address on the local network? A PURE predicate over the
        /// typed text — no state, no IO — so the settings panel can warn and
        /// gate the Connect button WHILE the user types, which is what the
        /// reference does (`PlexSettingsState.is-local-address`, computed in
        /// `plex_auth::refresh_gates` and read at
        /// `LocalLibrarySettings.slint:612` and `:631`).
        #[qinvokable]
        fn plex_url_is_local(self: Pin<&mut QbzLocal>, url: QString) -> bool;

        // --- PIN sign-in (plex_pin_qt.rs) ---------------------------------
        /// "Generate code": ask plex.tv for a PIN and start polling for the
        /// authorization. Takes the address from the panel so a freshly typed
        /// one is used without needing Connect first — the reference does the
        /// same (`plex_auth.rs:415` reads the field, then persists).
        #[qinvokable]
        fn plex_generate_code(self: Pin<&mut QbzLocal>, server_url: QString);
        /// Open the plex.tv sign-in page in the browser.
        #[qinvokable]
        fn plex_open_auth_url(self: Pin<&mut QbzLocal>);
        /// Copy the outstanding link code to the clipboard.
        #[qinvokable]
        fn plex_copy_code(self: Pin<&mut QbzLocal>);
        /// Drop the outstanding PIN and stop polling. The panel calls this on
        /// unmount — the reference has to detect that by watching the
        /// settings section instead, because Slint gives it no unmount hook.
        #[qinvokable]
        fn plex_stop_pin(self: Pin<&mut QbzLocal>);
        /// "Check connection": ping the stored server, report it, and stamp
        /// the machine id onto the cache.
        #[qinvokable]
        fn plex_check_connection(self: Pin<&mut QbzLocal>);

        // --- Windowed artwork -------------------------------------------------
        /// The mounted window reports its artKeys; Rust resolves each to a
        /// 256px thumbnail (local cover) or a disk-cached Plex thumb and
        /// emits `localArtworkReady` per hit.
        #[qinvokable]
        fn artwork_window(self: Pin<&mut QbzLocal>, keys_json: QString);
        /// (key, "file://…") — id-keyed so a cover can never land on the
        /// wrong row.
        #[qsignal]
        fn local_artwork_ready(self: Pin<&mut QbzLocal>, key: QString, path: QString);
    }

    impl cxx_qt::Threading for QbzLocal {}
}

use qbz_local::QbzLocal;

/// Rust side of the local-library bridge (plain storage, phase-1 pattern).
pub struct QbzLocalRust {
    local_available: bool,
    local_counts_json: QString,
    local_album_mode: QString,
    local_albums_loading: bool,
    local_albums_error: QString,
    local_albums_json: QString,
    local_artists_loading: bool,
    local_artists_json: QString,
    local_folders_loading: bool,
    local_folders_json: QString,
    local_tree_loading: bool,
    local_tree_json: QString,
    local_tree_selected_count: i32,
    local_detail_loading: bool,
    local_detail_json: QString,
    local_tracks_loading: bool,
    local_tracks_loading_more: bool,
    local_tracks_has_more: bool,
    local_tracks_sort: QString,
    local_tracks_group: QString,
    local_tracks_json: QString,
    local_album_loading: bool,
    local_album_json: QString,
    local_track_artwork: bool,
    local_pending_artist: QString,
    local_pending_route: QString,
    local_ephemeral_active: bool,
    local_ephemeral_loading: bool,
    local_ephemeral_json: QString,
    local_ephemeral_label: QString,
    local_ephemeral_open_seq: i32,
    local_ephemeral_is_cd: bool,
    local_rip_active: bool,
    local_rip_progress: QString,
    plex_enabled: bool,
    plex_available: bool,
    plex_syncing: bool,
    media_has_jellyfin: bool,
    media_has_subsonic: bool,
    albums_filter: QString,
    media_syncing: bool,
    media_sync_progress: QString,
    plex_last_sync_tracks: i32,
    plex_error: QString,
    plex_sections_json: QString,
    pin_code: QString,
    pin_auth_url: QString,
    pin_busy: bool,
}

impl Default for QbzLocalRust {
    fn default() -> Self {
        Self {
            local_available: false,
            local_counts_json: QString::from("{}"),
            local_album_mode: QString::from("folder"),
            local_albums_loading: false,
            local_albums_error: QString::default(),
            local_albums_json: QString::from("[]"),
            local_artists_loading: false,
            local_artists_json: QString::from("[]"),
            local_folders_loading: false,
            local_folders_json: QString::from("[]"),
            local_tree_loading: false,
            local_tree_json: QString::from("[]"),
            local_tree_selected_count: 0,
            local_detail_loading: false,
            local_detail_json: QString::from(""),
            local_tracks_loading: false,
            local_tracks_loading_more: false,
            local_tracks_has_more: false,
            local_tracks_sort: QString::from("default"),
            local_tracks_group: QString::from("off"),
            local_tracks_json: QString::from("[]"),
            local_album_loading: false,
            local_album_json: QString::from(""),
            local_track_artwork: false,
            local_pending_artist: QString::default(),
            local_pending_route: QString::default(),
            local_ephemeral_active: false,
            local_ephemeral_loading: false,
            local_ephemeral_json: QString::from(""),
            local_ephemeral_label: QString::from(""),
            local_ephemeral_open_seq: 0,
            local_ephemeral_is_cd: false,
            local_rip_active: false,
            local_rip_progress: QString::default(),
            plex_enabled: false,
            plex_available: false,
            plex_syncing: false,
            media_has_jellyfin: false,
            media_has_subsonic: false,
            albums_filter: QString::default(),
            media_syncing: false,
            media_sync_progress: QString::default(),
            plex_last_sync_tracks: -1,
            plex_error: QString::default(),
            plex_sections_json: QString::from("[]"),
            pin_code: QString::default(),
            pin_auth_url: QString::default(),
            pin_busy: false,
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzLocal>> = OnceLock::new();

/// Queue a local-bridge mutation onto the Qt event loop (no-op before boot
/// registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzLocal>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}


impl qbz_local::QbzLocal {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] local Qt thread already registered");
        }
        // Seed the persisted toolbar choices so the first paint matches
        // what the user last picked (shared with the Slint frontend).
        // `tracks_group` joined the seed with PARITY-DEBT #13: the key was
        // read and preserved on disk but never reached the UI, so the Tracks
        // tab silently reset to "No grouping" on every launch.
        let mode = lib::album_mode();
        let sort = lib::tracks_sort();
        let group = lib::tracks_group();
        crate::spawn(async move {
            let available = tokio::task::spawn_blocking(lib::has_library)
                .await
                .unwrap_or(false);
            ui(move |mut b| {
                b.as_mut().set_local_available(available);
                b.as_mut()
                    .set_local_album_mode(QString::from(mode.as_str()));
                b.as_mut()
                    .set_local_tracks_sort(QString::from(sort.as_str()));
                b.as_mut()
                    .set_local_tracks_group(QString::from(group.as_str()));
            });
        });
        publish_plex_state();
        crate::local_album_actions::publish_track_artwork();
        // Slint parity: re-open the persisted ad-hoc folder at startup.
        crate::local_ephemeral::rehydrate();
    }

    pub fn load_tab(self: Pin<&mut Self>, tab: QString) {
        load_tab_impl(tab.to_string());
    }

    pub fn set_album_mode(self: Pin<&mut Self>, mode: QString) {
        let mode = mode.to_string();
        lib::set_album_mode(&mode);
        let published = lib::album_mode();
        ui(move |mut b| {
            b.as_mut()
                .set_local_album_mode(QString::from(published.as_str()));
        });
        // The grouping IS the query — reload both album surfaces.
        load_tab_impl("albums".to_string());
        // ...and the ARTISTS tab derives from that same album set: its album
        // cache is keyed by the group key, so a folder-mode compilation
        // cross-lists under every artist until the tab is revisited
        // (PARITY-DEBT #9). The Slint drops the model and lets
        // `ensure_artists_loaded` re-fetch on the next visit
        // (`local_library.rs:727-738 invalidate_artists`); this port has no
        // such guard — `loadTab` re-queries on every tab change — so the
        // equivalent is to drop the cache AND re-run the load right here.
        // That also covers the case the lazy version cannot: the mode is
        // reachable from Settings, so the user can flip it while STANDING on
        // the Artists tab.
        invalidate_artists();
    }

    pub fn tracks_search(self: Pin<&mut Self>, query: QString) {
        lib::set_tracks_query(&query.to_string());
        load_tracks(true);
    }

    pub fn tracks_set_sort(self: Pin<&mut Self>, sort: QString) {
        let sort = sort.to_string();
        lib::set_tracks_sort(&sort);
        ui(move |mut b| {
            b.as_mut()
                .set_local_tracks_sort(QString::from(sort.as_str()));
        });
        load_tracks(true);
    }

    pub fn tracks_set_group(self: Pin<&mut Self>, mode: QString) {
        let mode = mode.to_string();
        lib::set_tracks_group(&mode);
        ui(move |mut b| {
            b.as_mut()
                .set_local_tracks_group(QString::from(mode.as_str()));
        });
        // NO reload: unlike the sort, grouping is a client-side visual
        // reorder over the pages already loaded (the reference's
        // `set_tracks_group` only persists + re-derives).
    }

    pub fn tracks_load_more(self: Pin<&mut Self>) {
        if !lib::tracks_has_more() {
            return;
        }
        load_tracks(false);
    }

    pub fn tree_toggle(self: Pin<&mut Self>, path: QString, expand: bool) {
        let path = path.to_string();
        if !expand {
            lib::tree_collapse(&path);
            publish_tree(lib::to_json(&lib::tree_visible()));
            return;
        }
        crate::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || lib::tree_expand_blocking(&path)).await;
            publish_tree(lib::to_json(&lib::tree_visible()));
        });
    }

    pub fn tree_collapse_all(self: Pin<&mut Self>) {
        lib::tree_collapse_all();
        publish_tree(lib::to_json(&lib::tree_visible()));
    }

    pub fn tree_search(self: Pin<&mut Self>, query: QString) {
        lib::set_tree_search(&query.to_string());
        publish_tree(lib::to_json(&lib::tree_visible()));
    }

    pub fn select_folder(self: Pin<&mut Self>, path: QString) {
        let path = path.to_string();
        ui(|mut b| b.as_mut().set_local_detail_loading(true));
        crate::spawn(async move {
            let detail = tokio::task::spawn_blocking(move || lib::load_folder_detail_blocking(&path))
                .await
                .ok();
            let json = detail.map(|d| lib::to_json(&d)).unwrap_or_default();
            ui(move |mut b| {
                b.as_mut()
                    .set_local_detail_json(QString::from(json.as_str()));
                b.as_mut().set_local_detail_loading(false);
            });
        });
    }

    pub fn open_album(self: Pin<&mut Self>, id: QString) {
        open_album_by_id(id.to_string());
    }

    pub fn close_album(self: Pin<&mut Self>) {
        ui(|mut b| {
            b.as_mut().set_local_album_json(QString::from(""));
            b.as_mut().set_local_album_loading(false);
        });
    }

    pub fn play_album(self: Pin<&mut Self>, id: QString, shuffle: bool) {
        let id = id.to_string();
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_album(&runtime, id, None, shuffle).await;
        });
    }

    pub fn play_album_track(self: Pin<&mut Self>, id: QString, track_id: QString) {
        let id = id.to_string();
        let row = track_id.to_string().parse::<i64>().ok();
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_album(&runtime, id, row, false).await;
        });
    }

    pub fn play_folder(self: Pin<&mut Self>, path: QString) {
        let path = path.to_string();
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_folder(&runtime, path).await;
        });
    }

    pub fn play_folder_track(self: Pin<&mut Self>, path: QString, track_id: QString) {
        let path = path.to_string();
        let Ok(row) = track_id.to_string().parse::<i64>() else {
            return;
        };
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_folder_track(&runtime, path, row).await;
        });
    }

    pub fn play_tracks_visible(
        self: Pin<&mut Self>,
        visible_ids_json: QString,
        track_id: QString,
    ) {
        let Ok(row) = track_id.to_string().parse::<i64>() else {
            return;
        };
        let ids = visible_ids_json.to_string();
        let runtime = crate::app();
        crate::spawn(async move {
            lib::play_tracks_visible(&runtime, ids, row).await;
        });
    }

    pub fn enqueue(self: Pin<&mut Self>, kind: QString, id: QString, mode: QString) {
        let (kind, id, mode) = (kind.to_string(), id.to_string(), mode.to_string());
        let runtime = crate::app();
        crate::spawn(async move {
            lib::enqueue(&runtime, kind, id, mode).await;
        });
    }

    // --- Plex --------------------------------------------------------------

    pub fn sync_plex(self: Pin<&mut Self>) {
        if plex::is_syncing() {
            return;
        }
        run_sync();
    }

    // ── Media servers (Jellyfin / Subsonic) ─────────────────────────────

    pub fn set_albums_filter_json(mut self: Pin<&mut Self>, json: QString) {
        let text = json.to_string();
        // The qproperty is what the view re-seeds from on its next mount, so
        // it is written on EVERY toggle, not only on close.
        self.as_mut().set_albums_filter(QString::from(text.as_str()));
        crate::local_bridge_ops::save_albums_filter(&text);
    }

    pub fn media_test(self: Pin<&mut Self>, server: QString, url: QString) {
        let (Some(kind), url) = (media_kind(&server), url.to_string()) else {
            return;
        };
        crate::spawn(async move {
            match crate::media_servers_qt::probe(kind, &url).await {
                Ok(name) => crate::toast_qt::success(format!("Connected to {name}")),
                Err(e) => crate::toast_qt::error(e),
            }
        });
    }

    pub fn media_connect(
        self: Pin<&mut Self>,
        server: QString,
        url: QString,
        username: QString,
        password: QString,
    ) {
        let Some(kind) = media_kind(&server) else {
            return;
        };
        let (url, user, pass) = (url.to_string(), username.to_string(), password.to_string());
        crate::spawn(async move {
            match crate::media_servers_qt::connect(kind, &url, &user, &pass).await {
                Ok(()) => {
                    crate::toast_qt::success(qbz_i18n::t("Connected"));
                    crate::settings_qt::publish_snapshot().await;
                    run_media_sync(kind, true).await;
                }
                Err(e) => crate::toast_qt::error(e),
            }
        });
    }

    pub fn media_set_enabled(self: Pin<&mut Self>, server: QString, enabled: bool) {
        let Some(kind) = media_kind(&server) else {
            return;
        };
        crate::spawn(async move {
            let mut cfg = crate::media_servers_qt::get(kind);
            cfg.enabled = enabled;
            crate::media_servers_qt::put(kind, &cfg);
            // The panel reads `state.enabled` off the SETTINGS DOCUMENT, not
            // off a bridge property the way Plex's toggle does — so a write
            // that does not republish leaves the switch visually stuck in its
            // old position while the store underneath has already changed
            // (caught by driving the real window, 2026-08-20).
            crate::settings_qt::publish_snapshot().await;
            // The union IS the query — the grid/tracks/badges must re-run.
            reload_browse();
        });
    }

    pub fn media_sync(self: Pin<&mut Self>, server: QString, full: bool) {
        let Some(kind) = media_kind(&server) else {
            return;
        };
        // Refuse HERE rather than letting the sweep's own guard reject it: a
        // second click should say why, and by the time the guard sees it the
        // caller has already been told the task started.
        if crate::media_sync_qt::is_syncing(kind) {
            crate::toast_qt::info(qbz_i18n::t("A sync is already running"));
            return;
        }
        crate::spawn(async move { run_media_sync(kind, full).await });
    }

    pub fn media_disconnect(self: Pin<&mut Self>, server: QString) {
        let Some(kind) = media_kind(&server) else {
            return;
        };
        crate::spawn(async move {
            crate::media_servers_qt::disconnect(kind);
            crate::settings_qt::publish_snapshot().await;
            // Purge the rows too: leaving them would keep a signed-out
            // server's music in the grid until something else cleared it,
            // and the master-toggle path deliberately does NOT purge (so it
            // can be undone cheaply). Disconnect is the destructive one.
            crate::media_servers_qt::purge_cache(kind);
            reload_browse();
        });
    }

    pub fn plex_set_enabled(self: Pin<&mut Self>, enabled: bool) {
        crate::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || plex::set_enabled(enabled)).await;
            publish_plex_state();
            // The union IS the query — the grid/tracks/badges must re-run.
            reload_browse();
        });
    }

    pub fn plex_url_is_local(self: Pin<&mut Self>, url: QString) -> bool {
        plex::is_local_address(&url.to_string())
    }

    pub fn plex_generate_code(self: Pin<&mut Self>, server_url: QString) {
        let url = server_url.to_string();
        crate::spawn(async move { crate::plex_pin_qt::generate_code(url).await });
    }

    pub fn plex_open_auth_url(self: Pin<&mut Self>) {
        crate::spawn(async move { crate::plex_pin_qt::open_auth_url().await });
    }

    pub fn plex_copy_code(self: Pin<&mut Self>) {
        let code = crate::plex_pin_qt::current_code();
        if code.is_empty() {
            return;
        }
        crate::share_qt::copy_to_clipboard(code);
        crate::toast_qt::success(qbz_i18n::t("Code copied"));
    }

    pub fn plex_stop_pin(self: Pin<&mut Self>) {
        crate::plex_pin_qt::stop_poll();
    }

    pub fn plex_check_connection(self: Pin<&mut Self>) {
        crate::spawn(async move { crate::plex_pin_qt::check_connection().await });
    }

    pub fn plex_connect(self: Pin<&mut Self>, server_url: QString, token: QString) {
        let (url, token) = (server_url.to_string(), token.to_string());
        crate::spawn(async move {
            // The LAN gate, enforced BEFORE anything is persisted. Until
            // 2026-08-04 the only thing that produced the error below was an
            // UNPARSEABLE url: `connect_manual` resolves and stores whatever
            // parses, and never consulted `is_local_address` (which had
            // exactly one caller, a read-side gate). So a WAN address was
            // saved silently and the user was told "Plex is not configured"
            // by a later gate — the wrong error for the actual mistake.
            if !plex::is_local_address(&url) {
                // Reusing the Slint warning's msgid rather than the port's own
                // "Plex server must be a local network address", which has no
                // entry in ANY of the eight catalogs and therefore rendered
                // English for everyone. This one ships translated in all
                // seven non-English locales (`en` falls back to the msgid).
                let msg = qbz_i18n::t("Only local network servers are supported.");
                ui(move |mut b| {
                    b.as_mut().set_plex_error(QString::from(msg.as_str()));
                });
                return;
            }
            let base = tokio::task::spawn_blocking(move || {
                plex::set_enabled(true);
                plex::connect_manual(&url, &token)
            })
            .await
            .unwrap_or_default();
            if base.is_empty() {
                // Unusable input that still is not a LAN violation (an
                // unparseable url, or a scheme that is not http/https).
                // Translated Rust-side (qbz_i18n is the same catalog the
                // Slint build ships); QML shows `plexError` verbatim.
                let msg = qbz_i18n::t("Enter a valid server address.");
                ui(move |mut b| {
                    b.as_mut().set_plex_error(QString::from(msg.as_str()));
                });
                return;
            }
            publish_plex_state();
            run_sync();
        });
    }

    pub fn plex_disconnect(self: Pin<&mut Self>) {
        crate::spawn(async move {
            let _ = tokio::task::spawn_blocking(plex::disconnect).await;
            ui(|mut b| {
                b.as_mut().set_plex_last_sync_tracks(-1);
                b.as_mut().set_plex_error(QString::from(""));
            });
            publish_plex_state();
            reload_browse();
        });
    }

    pub fn set_plex_sections(self: Pin<&mut Self>, keys_json: QString) {
        let keys: Vec<String> = serde_json::from_str(&keys_json.to_string()).unwrap_or_default();
        crate::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || plex::set_selected_sections(&keys)).await;
            publish_plex_state();
            run_sync();
        });
    }

    pub fn refresh_plex(self: Pin<&mut Self>) {
        publish_plex_state();
    }

    // --- Artwork -----------------------------------------------------------

    pub fn artwork_window(self: Pin<&mut Self>, keys_json: QString) {
        let keys: Vec<String> = serde_json::from_str(&keys_json.to_string()).unwrap_or_default();
        if keys.is_empty() {
            return;
        }
        crate::spawn(async move {
            // Phase 1 (cheap: memos + one stat per key, no decode) — emit
            // everything already on disk right away.
            let window = tokio::task::spawn_blocking(move || lib::resolve_window_blocking(keys))
                .await
                .ok();
            let Some(window) = window else {
                return;
            };
            emit_artwork(window.hits);
            // Phase 2, both arms in parallel and both STREAMING: each cover
            // reaches QML through `localArtworkReady` the moment it resolves,
            // so a first visit fills in progressively instead of landing as
            // one lump after the whole window is done.
            //   - cold local covers: bounded blocking pool, started in
            //     display order (row 1 before row 40);
            //   - Plex/http misses: network, independent of the CPU work, so
            //     a slow decode never holds up a downloaded cover.
            // (`stream_cold` is reached directly — `local_library_qt` only
            // re-exports the two entry points the older batch flow used.)
            let cold = crate::local_artwork::stream_cold(window.cold, emit_artwork_one);
            let remote = async {
                let fetched = lib::fetch_plex_misses(window.plex_misses).await;
                emit_artwork(fetched);
            };
            tokio::join!(cold, remote);
        });
    }
    // --- Tree selection + bulk actions --------------------------------------

    pub fn tree_set_select_mode(self: Pin<&mut Self>, on: bool) {
        crate::local_bulk::set_select_mode(on);
    }

    pub fn tree_toggle_folder_select(self: Pin<&mut Self>, path: QString) {
        crate::local_bulk::toggle_folder_select(path.to_string());
    }

    pub fn tree_toggle_track_select(self: Pin<&mut Self>, path: QString) {
        crate::local_bulk::toggle_track_select(path.to_string());
    }

    pub fn folders_bulk_action(self: Pin<&mut Self>, action: QString) {
        crate::local_bulk::folders_bulk_action(action.to_string());
    }

    pub fn bulk_action(self: Pin<&mut Self>, scope: QString, ids_json: QString, action: QString) {
        crate::local_bulk::bulk_action(scope.to_string(), ids_json.to_string(), action.to_string());
    }

    // --- Local album actions -------------------------------------------------

    pub fn clear_pending_artist(self: Pin<&mut Self>) {
        crate::local_album_actions::clear_pending_artist();
    }

    pub fn clear_pending_route(self: Pin<&mut Self>) {
        crate::local_album_actions::clear_pending_route();
    }

    pub fn open_artist_by_name(self: Pin<&mut Self>, name: QString) {
        crate::local_album_actions::open_artist_by_name(name.to_string());
    }

    pub fn artist_album_ids(&self, artist: QString) -> QString {
        QString::from(crate::local_albums::artist_album_ids(&artist.to_string()).as_str())
    }

    pub fn album_edit_tags(self: Pin<&mut Self>, id: QString) {
        crate::local_album_actions::edit_tags(id.to_string());
    }

    pub fn album_add_to_playlist(self: Pin<&mut Self>, id: QString) {
        crate::local_album_actions::add_to_playlist(id.to_string());
    }

    pub fn album_add_to_mixtape(self: Pin<&mut Self>, id: QString) {
        crate::local_album_actions::add_to_mixtape(id.to_string());
    }

    pub fn album_select_version(self: Pin<&mut Self>, index: i32) {
        crate::local_album_actions::select_version(index);
    }

    pub fn album_disc_action(self: Pin<&mut Self>, disc: i32, action: QString) {
        crate::local_album_actions::disc_action(disc, action.to_string());
    }

    // --- Ephemeral folder ----------------------------------------------------

    pub fn ephemeral_open(self: Pin<&mut Self>) {
        crate::local_ephemeral::open();
    }

    pub fn ephemeral_open_path(self: Pin<&mut Self>, path: QString) {
        crate::local_ephemeral::open_path(path.to_string());
    }

    pub fn ephemeral_clear(self: Pin<&mut Self>) {
        crate::local_ephemeral::clear();
    }

    pub fn ephemeral_open_cd(self: Pin<&mut Self>) {
        // Reading a TOC spins the drive up and can take a second or two, so it
        // does not run on the UI thread.
        crate::spawn(async move {
            match crate::cdda_qt::open_disc().await {
                Ok(n) => log::info!("[qbz-qt] cd opened: {n} tracks"),
                Err(msg) => crate::toast_qt::error(msg),
            }
        });
    }

    pub fn ephemeral_open_sacd(self: Pin<&mut Self>) {
        crate::spawn(async move {
            let picked = tokio::task::spawn_blocking(crate::local_ephemeral::pick_image_blocking)
                .await
                .ok()
                .flatten();
            let Some(path) = picked else { return };
            let p = std::path::PathBuf::from(&path);
            // Reading the area TOC seeks around a multi-gigabyte file.
            let outcome = tokio::task::spawn_blocking(move || crate::sacd_qt::open_image(&p)).await;
            match outcome {
                Ok(Ok(n)) => log::info!("[qbz-qt] sacd opened: {n} tracks"),
                Ok(Err(msg)) => crate::toast_qt::error(msg),
                Err(e) => log::warn!("[qbz-qt] sacd open task failed: {e}"),
            }
        });
    }

    pub fn rip_disc(self: Pin<&mut Self>) {
        crate::rip_qt::start();
    }

    pub fn ephemeral_play_all(self: Pin<&mut Self>, shuffle: bool) {
        let runtime = crate::app();
        crate::spawn(async move {
            crate::local_ephemeral::play_all(&runtime, shuffle).await;
        });
    }

    pub fn ephemeral_play_album(self: Pin<&mut Self>, group_key: QString) {
        let key = group_key.to_string();
        let runtime = crate::app();
        crate::spawn(async move {
            crate::local_ephemeral::play_album(&runtime, key).await;
        });
    }

    pub fn ephemeral_play_track(self: Pin<&mut Self>, id: QString) {
        let Ok(row) = id.to_string().parse::<i64>() else {
            return;
        };
        let runtime = crate::app();
        crate::spawn(async move {
            crate::local_ephemeral::play_track(&runtime, row).await;
        });
    }

}

// ---------------------------------------------------------------------------
// Local album routing
// ---------------------------------------------------------------------------

/// Open a LOCAL/Plex album. Lifted out of the invokable so `open_album` in
/// main.rs can route to it when a local id reaches the catalog path.
pub(crate) fn open_album_by_id(id: String) {
    ui(|mut b| {
        // Clear the previous local album with the loading flag — the same
        // stale-render the catalog album view had.
        b.as_mut().set_local_album_json(QString::from(""));
        b.as_mut().set_local_album_loading(true);
    });
    crate::spawn(async move {
        let detail = tokio::task::spawn_blocking(move || lib::load_album_detail_blocking(&id))
            .await
            .ok()
            .flatten();
        let json = detail.map(|d| lib::to_json(&d)).unwrap_or_default();
        ui(move |mut b| {
            b.as_mut()
                .set_local_album_json(QString::from(json.as_str()));
            b.as_mut().set_local_album_loading(false);
        });
    });
}

/// Map the QML `server` word to a kind, toasting on an unknown one.
///
/// A silent `return` would make a typo in QML look like a dead button; this at
/// least says which word was not understood.
fn media_kind(server: &QString) -> Option<qbz_app::settings::media_servers::MediaServerKind> {
    let w = server.to_string();
    let kind = qbz_app::settings::media_servers::MediaServerKind::from_word(w.trim());
    if kind.is_none() {
        log::error!("[qbz-qt] media server: unknown server word {w:?}");
    }
    kind
}

/// Run a sweep and report it, then reload the browse documents so the new rows
/// appear without the user navigating away and back.
async fn run_media_sync(kind: qbz_app::settings::media_servers::MediaServerKind, full: bool) {
    use qbz_app::settings::media_servers::MediaServerKind;
    let result = match kind {
        MediaServerKind::Jellyfin => crate::media_sync_qt::sync_jellyfin(full).await,
        MediaServerKind::Subsonic => crate::media_sync_qt::sync_subsonic(full).await,
    };
    match result {
        Ok(r) => {
            log::info!(
                "[qbz-qt] {} sync: {} saved, {} pruned, {} cached",
                kind.as_str(),
                r.saved,
                r.pruned,
                r.total
            );
            // The sweep stamped `last_sync_at` / `last_sync_tracks`; without a
            // republish the panel keeps reporting "Not synced yet".
            crate::settings_qt::publish_snapshot().await;
            reload_browse();
        }
        Err(e) => crate::toast_qt::error(e),
    }
}
