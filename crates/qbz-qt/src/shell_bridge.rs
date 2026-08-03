//! QbzShell — shell-chrome domain bridge (phase 23 split of the QbzBridge
//! God-object; the pattern is documented in main.rs). Props: sidebar / queue
//! / nav-history / now-playing-bar mode / lyrics panel / window chrome /
//! ambient background / theme / sidebar playlist tree / drag & drop.
//! Invokables: the shell actions (sidebar cycle, nav, npb mode, theme,
//! sidebar tree, drag & drop) — one-line forwards into the crate handlers.

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_shell {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Shell chrome (Slint ShellState) ----------------------------
        // Three-state sidebar: 0 = open (240px), 1 = mini (64px), 2 = closed.
        // Seeded from ui_prefs `sidebar_state` and rewritten by cycle_sidebar
        // (1:1 Slint: persist-sidebar-state).
        #[qproperty(i32, sidebar_state)]
        #[qproperty(bool, queue_open)]

        // --- Section-nav placement (Slint ShellState) ---------------------
        // ON  = the Discover / Library / Local Library / My QBZ rows live in
        //       the SIDEBAR (with a hairline under them); the header shows the
        //       compact icon nav only while the sidebar is fully closed.
        // OFF = the sections live in the HEADER, after the three history
        //       buttons, as full text tabs — or as the icon-only compact form
        //       when `nav_header_compact` is ON (or the sidebar is closed).
        // Both are LIVE (ui_prefs `nav_in_sidebar` / `nav_header_compact`).
        #[qproperty(bool, nav_in_sidebar)]
        #[qproperty(bool, nav_header_compact)]
        // Playlist rows show a 2x2 micro-collage of track covers instead of
        // the generic list-music glyph (Slint SidebarState.playlist-collage,
        // ui_prefs `sidebar_playlist_collage`). Opt-OUT, live.
        #[qproperty(bool, sidebar_playlist_collage)]
        // Content view id — only "home" exists (phase 3 adds the rest).
        #[qproperty(QString, current_view)]
        // Nav history (src/nav_qt.rs):
        #[qproperty(bool, can_back)]
        #[qproperty(bool, can_forward)]

        // Now-playing bar mode (phase 18): 0 New / 1 Classic / 2 Small /
        // 3 Large — the ui_prefs npb_mode key, live-switchable.
        #[qproperty(i32, npb_mode)]

        // --- Large-NPB dock (SidebarNowPlayingDock.slint) -----------------
        // The dock is the L's vertical arm: a square cover pinned flush to the
        // window bottom-left, with an optional FFT band above it.
        // Eye toggle on the cover — persists ui_prefs.large_visualizer_on.
        #[qproperty(bool, large_visualizer_on)]
        // Band click cycles 0 Bars / 1 Waveform / 2 Energy
        // (ui_prefs.large_spectrum_mode).
        #[qproperty(i32, large_spectrum_mode)]
        // Total dock height, in lockstep with the toggle. The Sidebar reserves
        // `largeDockHeight - npb height` so the playlist list stops ABOVE the
        // cover instead of running under it, and AppShell pins the dock at
        // `height - largeDockHeight`. Both consumers read THIS — never a
        // literal (Sidebar.slint:1145 is the same contract).
        #[qproperty(f32, large_dock_height)]

        // --- Theme (phase 19) --------------------------------------------
        // ONE JSON token document (theme_qt.rs ThemeTokens: 30 colors + 24
        // alpha tiers + ambient derivations + isDark) — QbzTheme.qml binds
        // to it, so a theme switch repaints the whole app live (the Slint
        // theme::push_colors equivalent).
        #[qproperty(QString, theme_json)]
        // The persisted ui_prefs theme slug ("oled", "auto", "custom", ...).
        #[qproperty(QString, theme_slug)]
        // The dropdown catalog (36 registry themes + Auto/Custom rows).
        #[qproperty(QString, theme_list_json)]
        // Dropdown filter: 0 All / 1 Dark / 2 Light (ui_prefs theme_filter).
        #[qproperty(i32, theme_filter)]

        // --- Lyrics panel (phase 9) ----------------------------------------
        #[qproperty(bool, lyrics_open)]
        // One JSON document (lyrics_qt.rs LyricsDoc: status/lines/synced/
        // provider/error).
        #[qproperty(QString, lyrics_json)]

        // --- Sidebar playlist tree ---------------------------------------
        // One JSON document: the flattened entries (folders + playlists,
        // expand/sort/search applied Rust-side — sidebar_qt.rs).
        #[qproperty(QString, sidebar_json)]
        #[qproperty(QString, sidebar_sort_by)]
        #[qproperty(bool, sidebar_sort_asc)]
        // The MINI-RAIL folder flyout's own document (contract §4.7):
        // `{folderId, folderName, count, rows:[{id,name,isLocal}]}`. It is a
        // separate document from `sidebar_json` because a COLLAPSED folder's
        // children are absent from the flattened entries — listing them used
        // to require force-expanding the folder, a persistent side effect the
        // reference does not have. The default is the FULL SHAPE, never "{}":
        // `JSON.parse("{}").rows.length` throws in the pre-publish frame.
        #[qproperty(QString, sidebar_folder_popup_json)]

        // --- Window chrome (phase 12) --------------------------------------
        // The APPLIED titlebar mode (the ui_prefs `use_system_title_bar`
        // value read at startup — drives the window flags; never mutated at
        // runtime, matching the Slint restart semantics).
        #[qproperty(bool, system_title_bar)]
        // The PERSISTED pref as it stands NOW (the app-menu check state;
        // flips live when the user toggles, applies on the next launch).
        #[qproperty(bool, system_title_bar_pref)]

        // --- Main-window geometry -----------------------------------------
        // The restored LOGICAL size + maximized flag from the shared
        // ui_prefs.json (settings_qt::window_size / window_maximized; the
        // Slint keys). Seeded at CONSTRUCTION, not at boot(): Main.qml binds
        // `width`/`height` to these, and the very first frame is already too
        // late for a post-boot push — same reason `theme_json` is seeded in
        // Default. Never rewritten at runtime; the save path goes the other
        // way, through `save_window_geometry`.
        #[qproperty(f32, window_width)]
        #[qproperty(f32, window_height)]
        #[qproperty(bool, window_maximized)]
        // The app floor (940x600), carried instead of literalised in QML so
        // the number stays single-sourced with the restore clamp and the
        // save gate that use it in settings_qt.
        #[qproperty(f32, window_min_width)]
        #[qproperty(f32, window_min_height)]

        // --- Ambient background (phase 14) --------------------------------
        // 0 = off, 1 = on (the ambient look; the owner's store carries
        // "ambient"). Live — toggling applies immediately (pure QML
        // layering). Combined in QML with npHasTrack for the D4 no-track
        // rule.
        /// Reduce-motion: throttle every continuous animation to a coarse
        /// tick instead of the display rate.
        ///
        /// DERIVED, never a user preference — the reference computes it as
        /// `kiosk_profile || !use_gpu_renderer`
        /// (`qbz-nix/crates/qbz/src/main.rs:8597`) and there is no Settings
        /// row for it in either frontend. Do not add one: the play-indicator
        /// animation toggle is a *preference* and is a different thing.
        ///
        /// Built by the cortinilla-parity contract (§5.g, ruling R10) so the
        /// KIOSK port can consume it rather than having to invent it. See
        /// `set_reduce_motion` for the two halves and why one is dormant.
        #[qproperty(bool, reduce_motion)]
        #[qproperty(i32, ambient_mode)]
        // Album-art triad (ambient_qt.rs), pushed on track change.
        #[qproperty(QString, ambient_primary)]
        #[qproperty(QString, ambient_secondary)]
        #[qproperty(QString, ambient_accent)]
        // Look knobs (Slint AppearanceState defaults; QBZ_BG_* env seed).
        #[qproperty(f32, ambient_dim)]
        #[qproperty(f32, ambient_surface_alpha)]
        #[qproperty(f32, ambient_bar_alpha)]

        // --- Rounded-cover render arm (manual override) --------------------
        // `RoundedImage` masks covers with a MultiEffect layer and falls back
        // to its CPU Canvas raster automatically when `GraphicsInfo.api`
        // reports Software/Null. This pins the Canvas arm by hand on a machine
        // where GraphicsInfo reports a GPU and the mask still misbehaves,
        // without a rebuild: env `QBZ_QT_ROUND_MODE=canvas`. Default false.
        // Seeded in `impl Default`, NOT in `boot()` — QML singletons
        // instantiate lazily and the first access is a `RoundedImage`
        // `_useCanvas` read, which can precede `QbzShell.boot()`.
        #[qproperty(bool, force_canvas_art)]

        // --- In-app toasts (shared) ---------------------------------------
        // ONE JSON document (toast_qt.rs ToastDoc: seq / kind / message /
        // persistent), rendered by the single `controls/QbzToast.qml` host
        // mounted in AppShell. Default "{}"; `seq` is monotonic and starts at
        // 1, so the host hides itself while `seq <= 0` and re-shows even when
        // the SAME message repeats.
        //
        // Lives on QbzShell rather than getting a bridge of its own for the
        // same reason the drag ghost does: the producers are everywhere (any
        // controller, any thread) and the consumer is one window-level
        // overlay. Rust owns no timer — the auto-hide delay is keyed off `seq`
        // in QML, so a toast never needs a hide round-trip.
        #[qproperty(QString, toast_json)]

        // --- Drag & drop (phase 17) ----------------------------------------
        // The shared drag state (DragState in state.slint): the ghost reads
        // count/title/subtitle + the window-coord pointer; sidebar playlist
        // rows self-detect the drop via dragX/dragY + over-playlist-id.
        #[qproperty(bool, drag_active)]
        #[qproperty(i32, drag_count)]
        #[qproperty(QString, drag_title)]
        #[qproperty(QString, drag_subtitle)]
        #[qproperty(f32, drag_x)]
        #[qproperty(f32, drag_y)]
        #[qproperty(QString, drag_over_playlist_id)]
        // Floating album-art preview over the now-playing bar's small cover
        // (SongCard.slint:296 + ArtPreviewOverlay.slint). Anchor is in WINDOW
        // coordinates: x = the cover's horizontal centre, y = its top edge.
        // Carried here for the same reason the drag ghost is: the trigger
        // lives inside the bar, the overlay must paint above every surface, and
        // the two are in different .qml files.
        #[qproperty(bool, art_preview_show)]
        #[qproperty(f32, art_preview_x)]
        #[qproperty(f32, art_preview_y)]

        // --- Multi-select seam (2026-08-03 hotkeys-port contract §4.6) ----
        // QML-REPORTED — selection state is in-view QML JS
        // (`library_bulk.rs:8`: "select-all / clear never reach Rust"), so the
        // AppShell reporter Bindings write these: a view implementing the
        // duck-typed `selectAll()` is mounted (capable) / a view's
        // `multiSelectOn` flag is up (active). CUSTOM WRITE (the
        // `QbzImmersive.open` funnel precedent) so the Rust-side AtomicBool
        // mirrors stay exact for the cross-singleton hotkeys reads — the
        // Ctrl+A consumption predicate and the §1.2 Escape stack arm 6 run
        // on the QbzHotkeys singleton and cannot reach this QObject's
        // properties.
        #[qproperty(bool, multi_select_active, READ, WRITE = set_multi_select_active, NOTIFY)]
        #[qproperty(bool, multi_select_capable, READ, WRITE = set_multi_select_capable, NOTIFY)]

        type QbzShell = super::QbzShellRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzShell>);

        // --- Shell chrome -------------------------------------------------
        /// Header panel-left button: cycle the sidebar open -> mini ->
        /// closed -> open (Slint `ShellState.cycle-sidebar()`).
        #[qinvokable]
        fn cycle_sidebar(self: Pin<&mut QbzShell>);
        /// NPB queue button / queue panel close.
        #[qinvokable]
        fn toggle_queue(self: Pin<&mut QbzShell>);
        /// Header history buttons.
        #[qinvokable]
        fn navigate_back(self: Pin<&mut QbzShell>);
        #[qinvokable]
        fn navigate_forward(self: Pin<&mut QbzShell>);

        /// NPB lyrics button / lyrics panel close X.
        #[qinvokable]
        fn toggle_lyrics(self: Pin<&mut QbzShell>);

        /// Sidebar navigation: record a content view ("home" | "library")
        /// and lazy-load its data on first visit.
        #[qinvokable]
        fn navigate_to(self: Pin<&mut QbzShell>, view: QString);
        /// Open a url in the system browser (the Slint
        /// AlbumActions.open-external-link). Spawns xdg-open detached.
        #[qinvokable]
        fn open_external_url(self: Pin<&mut QbzShell>, url: QString);

        /// Now-Playing-view flyout: switch the bar mode (0 New / 1 Classic /
        /// 2 Small / 3 Large) — persists ui_prefs.npb_mode; Large forces the
        /// sidebar open (the Slint "large" arm).
        #[qinvokable]
        fn npb_set_mode(self: Pin<&mut QbzShell>, mode: i32);

        /// Large dock, cover eye button: show/hide the FFT band. Persists the
        /// pref, republishes `largeDockHeight`, and gates the capture tap —
        /// hiding the band stops the FFT producer outright.
        #[qinvokable]
        fn large_toggle_visualizer(self: Pin<&mut QbzShell>);
        /// Large dock, band click: cycle Bars -> Waveform -> Energy.
        #[qinvokable]
        fn large_cycle_spectrum(self: Pin<&mut QbzShell>);

        // --- Theme (phase 19) ---------------------------------------------
        /// Appearance > Theme row: persist the picked slug + republish
        /// `themeJson` (live switch).
        #[qinvokable]
        fn theme_set(self: Pin<&mut QbzShell>, slug: QString);
        /// Appearance > theme filter cycle button (0 All / 1 Dark / 2 Light).
        #[qinvokable]
        fn theme_set_filter(self: Pin<&mut QbzShell>, index: i32);

        /// Main.qml, debounced off every settled resize / visibility flip and
        /// fired once more on close: persist the FLOATING size plus the
        /// maximized flag. The whole rule set (floating-only sizes, the app
        /// minimum, the >0.5px dirty check) lives in
        /// `settings_qt::save_window_geometry`, mirroring the Slint
        /// `WindowEvent::Resized` handler — QML only supplies the numbers.
        #[qinvokable]
        fn save_window_geometry(
            self: Pin<&mut QbzShell>,
            width: f32,
            height: f32,
            maximized: bool,
            fullscreen: bool,
        );

        /// App-menu chrome toggle: flip the persisted `use_system_title_bar`
        /// pref (applies on the next launch — the window flags are fixed at
        /// creation, 1:1 Slint). Updates `systemTitleBarPref` only.
        #[qinvokable]
        fn toggle_system_title_bar(self: Pin<&mut QbzShell>);

        /// App-menu ambient toggle: flip the persisted `app_background` pref
        /// off <-> "ambient" and apply LIVE (pure QML layering — no restart,
        /// unlike the titlebar).
        #[qinvokable]
        fn toggle_ambient_background(self: Pin<&mut QbzShell>);

        /// Sidebar tree: rebuild + republish (the `…` menu's Refresh row).
        ///
        /// Routes to `crate::reload_sidebar_including_local()`, NOT
        /// `crate::reload_sidebar()`. The latter early-returns while offline,
        /// which made the ONE recovery affordance for a broken tree unusable in
        /// exactly the state where the user needs it — and an account-less user
        /// has nothing else. The local-safe verb is not a no-op offline: it
        /// re-reads folders, folder membership, the hidden set and the LOCAL
        /// playlists from library.db and republishes, while `sidebar_qt::load`
        /// preserves the cached Qobuz set rather than wiping it (see its
        /// header). Online the two verbs do the same thing.
        #[qinvokable]
        fn reload_sidebar(self: Pin<&mut QbzShell>);
        #[qinvokable]
        fn sidebar_set_sort(self: Pin<&mut QbzShell>, option: QString);
        #[qinvokable]
        fn sidebar_search(self: Pin<&mut QbzShell>, query: QString);
        #[qinvokable]
        fn sidebar_toggle_folder(self: Pin<&mut QbzShell>, id: QString);
        /// Mini-rail folder click: publish that folder's playlists into
        /// `sidebar_folder_popup_json`. Reads the sidebar CACHE only — no DB,
        /// no network, no expand side effect — so it behaves identically
        /// offline (contract §4.7 / block 6).
        #[qinvokable]
        fn sidebar_open_folder_popup(self: Pin<&mut QbzShell>, folder_id: QString);
        /// Sidebar "+" — create an empty playlist (single core call), then
        /// reload the tree.
        #[qinvokable]
        fn create_playlist(self: Pin<&mut QbzShell>);
        /// Sidebar cover dispatch: JSON array of cover URLS (the tree's
        /// collage is url-keyed, unlike the feed's artKey).
        #[qinvokable]
        fn sidebar_artwork_window(self: Pin<&mut QbzShell>, urls_json: QString);

        // --- Drag & drop (phase 17) ----------------------------------------
        /// Track-row press-drag start (row id, ghost texts, window coords).
        #[qinvokable]
        fn drag_start(self: Pin<&mut QbzShell>, track_id: QString, title: QString, subtitle: QString, x: f32, y: f32);
        #[qinvokable]
        fn drag_move(self: Pin<&mut QbzShell>, x: f32, y: f32);
        /// A sidebar playlist row claims / releases the drop target.
        #[qinvokable]
        fn drag_set_over(self: Pin<&mut QbzShell>, playlist_id: QString);
        /// Release: add the dragged track(s) to the over-playlist target.
        #[qinvokable]
        fn drag_end(self: Pin<&mut QbzShell>);

        // --- Multi-select seam (§4.6) --------------------------------------
        /// The hotkeys (C2) Ctrl+A arm / the §1.2 Escape-stack arm 6. Rust
        /// cannot reach the content-view Loader (AppShell.qml), so both
        /// bounce to QML as a signal; the AppShell router forwards to the
        /// mounted view's duck-typed `selectAll()` /
        /// `exitMultiSelectMode()`.
        #[qinvokable]
        fn select_all_active(self: Pin<&mut QbzShell>);
        #[qinvokable]
        fn exit_multi_select(self: Pin<&mut QbzShell>);
        #[qsignal]
        fn select_all_requested(self: Pin<&mut QbzShell>);
        #[qsignal]
        fn exit_multi_select_requested(self: Pin<&mut QbzShell>);
    }

    // The custom WRITE targets of the §4.6 seam properties. NOT qinvokables —
    // QML reaches them by writing `QbzShell.multiSelectActive` /
    // `multiSelectCapable`, never by name (the `QbzImmersive.open` cxx-qt
    // custom-setter pattern: the property's auto setter is replaced by this
    // method, which owns the store + notify + mirror).
    unsafe extern "RustQt" {
        fn set_multi_select_active(self: Pin<&mut QbzShell>, value: bool);
        fn set_multi_select_capable(self: Pin<&mut QbzShell>, value: bool);
    }

    impl cxx_qt::Threading for QbzShell {}
}

use qbz_shell::QbzShell;

/// Rust side of the shell bridge (plain storage, phase-1 pattern).
pub struct QbzShellRust {
    sidebar_state: i32,
    nav_in_sidebar: bool,
    nav_header_compact: bool,
    sidebar_playlist_collage: bool,
    queue_open: bool,
    current_view: QString,
    can_back: bool,
    can_forward: bool,
    npb_mode: i32,
    large_visualizer_on: bool,
    large_spectrum_mode: i32,
    large_dock_height: f32,
    theme_json: QString,
    theme_slug: QString,
    theme_list_json: QString,
    theme_filter: i32,
    lyrics_open: bool,
    lyrics_json: QString,
    sidebar_json: QString,
    sidebar_sort_by: QString,
    sidebar_sort_asc: bool,
    sidebar_folder_popup_json: QString,
    system_title_bar: bool,
    system_title_bar_pref: bool,
    window_width: f32,
    window_height: f32,
    window_maximized: bool,
    window_min_width: f32,
    window_min_height: f32,
    reduce_motion: bool,
    ambient_mode: i32,
    ambient_primary: QString,
    ambient_secondary: QString,
    ambient_accent: QString,
    ambient_dim: f32,
    ambient_surface_alpha: f32,
    ambient_bar_alpha: f32,
    force_canvas_art: bool,
    toast_json: QString,
    drag_active: bool,
    drag_count: i32,
    drag_title: QString,
    drag_subtitle: QString,
    drag_x: f32,
    drag_y: f32,
    drag_over_playlist_id: QString,
    art_preview_show: bool,
    art_preview_x: f32,
    art_preview_y: f32,
    multi_select_active: bool,
    multi_select_capable: bool,
}

impl Default for QbzShellRust {
    fn default() -> Self {
        // One file read for the restored size (the pair is all-or-nothing —
        // see settings_qt::window_size).
        let (window_width, window_height) = crate::settings_qt::window_size();
        Self {
            sidebar_state: crate::settings_qt::sidebar_state(),
            nav_in_sidebar: crate::settings_qt::nav_in_sidebar(),
            nav_header_compact: crate::settings_qt::nav_header_compact(),
            sidebar_playlist_collage: crate::settings_qt::sidebar_playlist_collage(),
            queue_open: false,
            current_view: QString::from("home"),
            can_back: false,
            can_forward: false,
            npb_mode: crate::settings_qt::npb_mode_index(),
            large_visualizer_on: crate::settings_qt::large_visualizer_on(),
            large_spectrum_mode: crate::settings_qt::large_spectrum_mode(),
            large_dock_height: large_dock_height(crate::settings_qt::large_visualizer_on()),
            theme_json: QString::from(crate::theme_qt::theme_json().as_str()),
            theme_slug: QString::from(crate::theme_qt::current_slug().as_str()),
            theme_list_json: QString::from(crate::theme_qt::theme_list_json().as_str()),
            theme_filter: crate::theme_qt::theme_filter(),
            lyrics_open: false,
            lyrics_json: QString::from("{}"),
            sidebar_json: QString::from("[]"),
            sidebar_sort_by: QString::from("name"),
            sidebar_sort_asc: true,
            // FULL SHAPE, not "{}" — see the qproperty comment.
            sidebar_folder_popup_json: QString::from(
                r#"{"folderId":"","folderName":"","count":0,"rows":[]}"#,
            ),
            system_title_bar: crate::settings_qt::use_system_title_bar(),
            system_title_bar_pref: crate::settings_qt::use_system_title_bar(),
            window_width,
            window_height,
            window_maximized: crate::settings_qt::window_maximized(),
            window_min_width: crate::settings_qt::WINDOW_MIN_WIDTH,
            window_min_height: crate::settings_qt::WINDOW_MIN_HEIGHT,
            reduce_motion: reduce_motion_at_boot(),
            ambient_mode: crate::settings_qt::app_background_mode(),
            // The Slint ImmersiveState default triad (pre-artwork colors).
            ambient_primary: QString::from("#00dcc8"),
            ambient_secondary: QString::from("#9632ff"),
            ambient_accent: QString::from("#3fd9c8"),
            ambient_dim: crate::settings_qt::ambient_dim(),
            ambient_surface_alpha: crate::settings_qt::ambient_surface_alpha(),
            ambient_bar_alpha: crate::settings_qt::ambient_bar_alpha(),
            // Read here, not in boot(): the first QML access to this singleton
            // is a RoundedImage `_useCanvas` read, which can happen before
            // QbzShell.boot() runs, and a boot() seed would leave the arm
            // decided against a stale `false`.
            force_canvas_art: std::env::var("QBZ_QT_ROUND_MODE").as_deref() == Ok("canvas"),
            // seq 0 = "nothing has been published"; the host stays hidden.
            toast_json: QString::from("{}"),
            drag_active: false,
            drag_count: 0,
            drag_title: QString::default(),
            drag_subtitle: QString::default(),
            drag_x: 0.0,
            drag_y: 0.0,
            drag_over_playlist_id: QString::default(),
            art_preview_show: false,
            art_preview_x: 0.0,
            art_preview_y: 0.0,
            multi_select_active: false,
            multi_select_capable: false,
        }
    }
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Large-NPB dock geometry
// ---------------------------------------------------------------------------
// SidebarNowPlayingDock.slint's height formula, verbatim. The cover is square
// and as wide as the sidebar's content column (240 sidebar - 16 left - 16
// right gutters = 208), so the totals are constants:
//   ON  = 9 top pad + 42 band + 10 gap + 208 art + 4 bottom gap = 273
//   OFF = 9 top pad                    + 208 art + 4 bottom gap = 221
// SidebarNowPlayingDock.qml lays itself out from these SAME numbers — if you
// change one, change both, or the cover shifts off the window bottom edge.
pub(crate) const DOCK_ART_SIZE: f32 = 208.0;
pub(crate) const DOCK_BAND_HEIGHT: f32 = 42.0;
pub(crate) const DOCK_PAD_TOP: f32 = 9.0;
pub(crate) const DOCK_PAD_BOTTOM: f32 = 4.0;
pub(crate) const DOCK_BAND_GAP: f32 = 10.0;

/// Total dock height for a visualizer state.
pub(crate) fn large_dock_height(visualizer_on: bool) -> f32 {
    let base = DOCK_PAD_TOP + DOCK_ART_SIZE + DOCK_PAD_BOTTOM;
    if visualizer_on {
        base + DOCK_BAND_HEIGHT + DOCK_BAND_GAP
    } else {
        base
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzShell>> = OnceLock::new();

/// Queue a shell-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
/// Kiosk profile — NOT PORTED YET. The kiosk contract
/// (`qbz-nix-docs/qt-frontend/2026-08-02-kiosk-port/00-CONTRACT.md:244`)
/// mandates "FORCED reduce-motion while in kiosk", so when that lands it
/// replaces this literal with the real profile flag and the seam is already
/// tested. One line, on their side.
const KIOSK_PROFILE_ACTIVE: bool = false;

/// The boot value of [`reduce_motion`].
///
/// The reference is `kiosk_profile || !use_gpu_renderer`. Only the FIRST half
/// is wired here, deliberately:
///
/// The renderer-tier half has no honest source in this port. The tier is a
/// QML-side fact (`GraphicsInfo.api`), not a Rust one, and this port runs on
/// the GPU in every real session (OpenGL RHI, measured 2026-07-29). The one
/// place where a tier probe WOULD report software is the offscreen smoke,
/// which forces it by definition — so wiring it would turn reduce-motion on
/// exactly where it means nothing and stay off everywhere it would matter.
/// If a genuine software-tier session ever appears (a remote desktop, a
/// llvmpipe box), that is when the second half earns its probe.
fn reduce_motion_at_boot() -> bool {
    KIOSK_PROFILE_ACTIVE
}

pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzShell>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

/// Rust-side mirror of the `queue_open` property — `toggle_queue` below is
/// the ONLY writer (the NPB button and the queue panel's close both call it;
/// QML never writes the property directly). Read by the hotkeys §1.2 Escape
/// stack arm 7 (2026-08-03 hotkeys-port contract), which runs on the
/// QbzHotkeys singleton and cannot reach this QObject's properties.
static QUEUE_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn queue_open() -> bool {
    QUEUE_OPEN.load(std::sync::atomic::Ordering::SeqCst)
}

/// Rust-side mirrors of the §4.6 multi-select properties, written inside the
/// custom-WRITE setters below — the funnel EVERY write passes through (QML
/// reporter Bindings included), so they are exact (the QUEUE_OPEN
/// precedent). Read by the hotkeys pipeline: the (C2) Ctrl+A consumption
/// predicate and the §1.2 Escape stack arm 6 (2026-08-03 hotkeys-port
/// contract), which run on ANOTHER singleton and cannot reach this QObject's
/// properties.
static MULTI_SELECT_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static MULTI_SELECT_CAPABLE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn multi_select_active() -> bool {
    MULTI_SELECT_ACTIVE.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn multi_select_capable() -> bool {
    MULTI_SELECT_CAPABLE.load(std::sync::atomic::Ordering::SeqCst)
}

impl qbz_shell::QbzShell {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] shell Qt thread already registered");
        }
    }

    pub fn cycle_sidebar(mut self: Pin<&mut Self>) {
        let next = (self.sidebar_state() + 1) % 3;
        self.as_mut().set_sidebar_state(next);
        // 1:1 Slint (`ShellState.cycle-sidebar` -> persist-sidebar-state): the
        // state survives the relaunch instead of snapping back to open.
        crate::settings_qt::set_sidebar_state(next);
        // state.slint:4094-4096 — the Large dock lives in the open sidebar, so
        // collapsing falls back to New. LIVE only: this is the generated
        // property setter, never npb_set_mode, which would persist the fallback
        // and lose the user's Large preference.
        if next != 0 && *self.npb_mode() == 3 {
            self.as_mut().set_npb_mode(0);
        }
    }

    pub fn toggle_queue(mut self: Pin<&mut Self>) {
        let next = !self.queue_open();
        QUEUE_OPEN.store(next, std::sync::atomic::Ordering::SeqCst);
        self.as_mut().set_queue_open(next);
    }

    pub fn navigate_back(self: Pin<&mut Self>) {
        crate::nav_qt::back();
    }

    pub fn navigate_forward(self: Pin<&mut Self>) {
        crate::nav_qt::forward();
    }

    pub fn toggle_lyrics(self: Pin<&mut Self>) {
        crate::toggle_lyrics();
    }

    pub fn navigate_to(self: Pin<&mut Self>, view: QString) {
        crate::navigate_to(&view.to_string());
    }

    pub fn npb_set_mode(self: Pin<&mut Self>, mode: i32) {
        crate::npb_set_mode(mode);
    }

    pub fn large_toggle_visualizer(self: Pin<&mut Self>) {
        crate::large_toggle_visualizer();
    }

    pub fn large_cycle_spectrum(self: Pin<&mut Self>) {
        crate::large_cycle_spectrum();
    }

    pub fn theme_set(self: Pin<&mut Self>, slug: QString) {
        crate::theme_set(slug.to_string());
    }

    pub fn theme_set_filter(self: Pin<&mut Self>, index: i32) {
        crate::theme_set_filter(index);
    }

    pub fn save_window_geometry(
        self: Pin<&mut Self>,
        width: f32,
        height: f32,
        maximized: bool,
        fullscreen: bool,
    ) {
        crate::settings_qt::save_window_geometry(width, height, maximized, fullscreen);
    }

    pub fn toggle_system_title_bar(self: Pin<&mut Self>) {
        crate::toggle_system_title_bar();
    }

    pub fn toggle_ambient_background(self: Pin<&mut Self>) {
        crate::toggle_ambient_background();
    }

    pub fn reload_sidebar(self: Pin<&mut Self>) {
        // The OFFLINE-SAFE verb — see the declaration's doc comment.
        crate::reload_sidebar_including_local();
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

    pub fn sidebar_open_folder_popup(self: Pin<&mut Self>, folder_id: QString) {
        crate::sidebar_open_folder_popup(&folder_id.to_string());
    }

    pub fn create_playlist(self: Pin<&mut Self>) {
        crate::create_playlist();
    }

    pub fn sidebar_artwork_window(self: Pin<&mut Self>, urls_json: QString) {
        crate::sidebar_artwork_window(urls_json.to_string());
    }

    pub fn drag_start(self: Pin<&mut Self>, track_id: QString, title: QString, subtitle: QString, x: f32, y: f32) {
        crate::drag_start(track_id.to_string(), title.to_string(), subtitle.to_string(), x, y);
    }

    pub fn drag_move(self: Pin<&mut Self>, x: f32, y: f32) {
        crate::drag_move(x, y);
    }

    pub fn drag_set_over(self: Pin<&mut Self>, playlist_id: QString) {
        crate::drag_set_over(playlist_id.to_string());
    }

    pub fn drag_end(self: Pin<&mut Self>) {
        crate::drag_end();
    }

    /// The custom WRITE targets of the §4.6 seam properties — the funnel
    /// every QML reporter-Binding write passes through (the
    /// `QbzImmersive.open` precedent), keeping the cross-singleton mirrors
    /// exact.
    pub fn set_multi_select_active(mut self: Pin<&mut Self>, value: bool) {
        use cxx_qt::CxxQtType as _;
        if self.multi_select_active == value {
            return;
        }
        self.as_mut().rust_mut().multi_select_active = value;
        self.as_mut().multi_select_active_changed();
        MULTI_SELECT_ACTIVE.store(value, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn set_multi_select_capable(mut self: Pin<&mut Self>, value: bool) {
        use cxx_qt::CxxQtType as _;
        if self.multi_select_capable == value {
            return;
        }
        self.as_mut().rust_mut().multi_select_capable = value;
        self.as_mut().multi_select_capable_changed();
        MULTI_SELECT_CAPABLE.store(value, std::sync::atomic::Ordering::SeqCst);
    }

    /// §4.6: Ctrl+A select-all — bounce to the AppShell router, which
    /// forwards to the mounted view's duck-typed `selectAll()`.
    pub fn select_all_active(mut self: Pin<&mut Self>) {
        self.as_mut().select_all_requested();
    }

    /// §4.6 / §1.2 Escape stack arm 6: exit the mounted view's multi-select
    /// mode via its duck-typed `exitMultiSelectMode()`.
    pub fn exit_multi_select(mut self: Pin<&mut Self>) {
        self.as_mut().exit_multi_select_requested();
    }
    pub fn open_external_url(self: Pin<&mut Self>, url: QString) {
        let url = url.to_string();
        if let Err(e) = open::that(&url) {
            log::warn!("[qbz-qt] failed to open '{url}': {e}");
        }
    }

}
