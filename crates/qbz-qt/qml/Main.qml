// Root window: dark background + a Loader keyed on the bridge's `screen`
// property ("splash" | "login" | "shell"). Kicks off the Rust boot
// sequence once the QML tree is complete (the bridge registers its Qt
// thread handle in that first invokable, so every async UI hop lands).
//
// Font: the Slint app bundles the Inter 18pt faces (app.slint); the same
// TTFs are embedded here via qrc and applied app-wide through the
// ApplicationWindow font (children inherit).

import QtQuick
import QtQuick.Controls
import QtQuick.Window
import com.blitzfc.qbz
import "theme"

ApplicationWindow {
    id: window
    // Geometry is RESTORED, not hardcoded (crates/qbz/src/main.rs:8211-8282 is
    // the Slint restore; :1399-1435 is its save). The bridge seeds these at
    // CONSTRUCTION from the shared ui_prefs.json, so the very first frame is
    // already the user's size — a post-boot push would arrive after layout.
    // `windowWidth`/`windowHeight` already carry the never-saved fallback
    // (1180x760: app.slint's preferred size), so there is no local default to
    // disagree with Slint about.
    //
    // This was the title-truncation bug: at the old hardcoded 1280 the
    // now-playing bar sat one responsive tier below the Slint build on the same
    // machine (1280 < 1366 -> side fraction 0.39 instead of 0.30), so the
    // Classic song card capped at ~446px instead of 560 and elided titles ~114px
    // early. The tier thresholds were right; the window was the wrong size.
    //
    // These stay plain bindings so the size is correct before the window maps.
    // The WM's own resizes write `width`/`height` from C++ — that does not
    // disturb the binding, and nothing ever re-sets the bridge properties, so
    // the value never snaps back under the user.
    //
    // Slint's best-effort monitor clamp (main.rs:8242-8249) is folded INTO the
    // binding rather than applied afterwards: a size saved on the big display
    // must not strand the window on a smaller one, and the clamped number has
    // to be the MAPPED number. Done imperatively in Component.onCompleted (as
    // it was) it ran after componentComplete had already shown the window, so
    // the window flashed at the oversized geometry and then snapped — and the
    // JS assignment broke the binding and fired the save debounce, which is how
    // a 2560px-wide session got permanently rewritten to 1920 after one launch
    // on the laptop panel (for the Slint build too — same file).
    // SHRINK ONLY, and only while the screen itself clears the app floor:
    // Slint skips the clamp on exactly that condition instead of fighting the
    // WM over an impossible minimum. A screen we cannot resolve degrades to
    // "no clamp", like Slint's monitor query returning None.
    width: window.bootScreenWidth >= QbzShell.windowMinWidth
           ? Math.min(QbzShell.windowWidth, window.bootScreenWidth)
           : QbzShell.windowWidth
    height: window.bootScreenHeight >= QbzShell.windowMinHeight
            ? Math.min(QbzShell.windowHeight, window.bootScreenHeight)
            : QbzShell.windowHeight
    // Screen metrics for that clamp, read from `window.screen` (the window's
    // OWN screen; the attached `Screen` wants an Item). Frozen in
    // Component.onCompleted — see the freeze there for why they must stop
    // tracking.
    property real bootScreenWidth: window.screen ? window.screen.width : 0
    property real bootScreenHeight: window.screen ? window.screen.height : 0
    // app.slint:52-53 (`min-width: 940px / UiScale.factor`), carried through the
    // bridge. The old 800x600 let the window go below Slint's floor, which is
    // exactly where the responsive tiers stop being comparable.
    minimumWidth: QbzShell.windowMinWidth
    minimumHeight: QbzShell.windowMinHeight
    visible: true
    // Last session's maximized state, applied DECLARATIVELY so it is part of
    // the first mapped frame: QQuickWindowQmlImpl defers the show until
    // componentComplete and then honours whatever `visibility` holds, whereas
    // the old Component.onCompleted assignment landed AFTER the map and the
    // window visibly jumped from the floating size to maximized. Slint restores
    // before `window.run()` and never flashes (main.rs:8273-8281).
    // One-shot by construction: `windowMaximized` is seeded at bridge
    // construction and never rewritten, so this never re-evaluates, and
    // HeaderBar's imperative `hostWindow.visibility = ...`
    // (shell/HeaderBar.qml:76) removes the binding outright the first time the
    // user toggles — which is the wanted behaviour, not a leak.
    // Windowed (not AutomaticVisibility) on the false branch: a fresh window
    // must land in its natural state, matching "only ever re-apply a true".
    visibility: QbzShell.windowMaximized ? Window.Maximized : Window.Windowed
    title: "QBZ"
    // Custom chrome (phase 7/12): frameless but OPAQUE — the phase-7
    // translucent window was a misread: the Slint MAIN window keeps an
    // OPAQUE swapchain (only the miniplayer blends; crates/qbz/src/main.rs
    // set_surface_prefers_transparent + the Cargo.toml patch comment), and
    // the rounded corners in the Slint screenshots come from the
    // COMPOSITOR, not the app — app.slint's root is opaque surface-main
    // with square corners and a square 1px hairline frame. The system
    // titlebar is the `use_system_title_bar` pref (ui_prefs.json; applied
    // at startup, restart semantics like Slint).
    flags: QbzShell.systemTitleBar ? Qt.Window : (Qt.Window | Qt.FramelessWindowHint)
    color: "#1a1a1a"

    FontLoader { id: interRegular; source: "assets/fonts/Inter_18pt-Regular.ttf" }
    FontLoader { id: interMedium; source: "assets/fonts/Inter_18pt-Medium.ttf" }
    FontLoader { id: interSemiBold; source: "assets/fonts/Inter_18pt-SemiBold.ttf" }
    FontLoader { id: interBold; source: "assets/fonts/Inter_18pt-Bold.ttf" }
    // The quality badge's face in the Tauri build — loading it here registers
    // the family app-wide, so QualityBadge can name it without carrying its
    // own loader into every row it renders in.
    FontLoader { id: lineSeed; source: "assets/fonts/LINESeedJP-Regular.ttf" }
    font.family: interRegular.status === FontLoader.Ready ? interRegular.name : "Sans Serif"

    // Phase 23: every domain singleton boots (registers its Qt-thread
    // hop; QbzSession.boot additionally fires the app boot sequence).
    Component.onCompleted: {
        QbzSession.boot()
        // Qobuz Connect (2026-08-01 contract §2/§8). Booted right after
        // QbzSession because QbzSession.boot fires the whole app boot
        // sequence, whose shell entry runs the §11.5 service wiring — the
        // badge/devices publishes it makes must find the Qt-thread hop
        // already registered or they are dropped silently.
        QbzQConnect.boot()
        // Hotkeys layer (2026-08-03 hotkeys-port contract, block B1): boot
        // after QbzSession like every domain singleton. The B2 dispatcher
        // (AppShell-root Keys.onPressed) is the only other QML side.
        QbzHotkeys.boot()
        // Search (2026-08-03 cortinilla-parity contract, commit C0): the
        // domain extracted from QbzBridge. Its position among the other
        // domain singletons is not load-bearing — every boot() here runs
        // before the first key or click — but the LINE is: boot() is what
        // registers the Qt-thread hop, and without it `search_bridge::ui()`
        // is a silent no-op, so the dropdown would never repaint and
        // nothing would be logged (TRACK-RULES, singleton boot order).
        QbzSearch.boot()
        QbzShell.boot()
        QbzPlayer.boot()
        QbzQueue.boot()
        QbzHome.boot()
        QbzViz.boot()
        // Immersive mode (2026-08-02 immersive-port contract, block B1).
        // Booted right after QbzViz: the overlay's open funnel drives the
        // viz two-source enable (§4.2), and ambient_qt publishes glow_color/
        // atmosphere_url to this singleton on every track change — a missing
        // boot line would drop those publishes silently (see above).
        QbzImmersive.boot()
        // Immersive Suggestions (the same contract, block B4 §4.5) — booted
        // WITH its bridge: every publish the suggestions loader makes rides
        // this hop, so a missing line is the forever-"{}" silent no-op the
        // comment below warns about (trap 2).
        QbzSuggestions.boot()
        QbzLocal.boot()
        QbzLibrary.boot()
        QbzAlbum.boot()
        QbzArtist.boot()
        QbzLyrics.boot()
        QbzCast.boot()
        QbzBridge.boot()
        // MyQBZ (two grids + detail + modals), the app-wide Add picker, the
        // Artist-Collection builder, and the Blacklist manager.
        //
        // boot() is what registers each singleton's CxxQtThread hop, so a
        // missing line here is a SILENT no-op forever: the view mounts, every
        // `ui()` publish from Rust is dropped on the floor, the document stays
        // at its "{}" default and NOTHING is logged on either side. QbzBlacklist
        // is the loudest case — its `blacklistLoading` defaults to true, so the
        // manager would spin forever.
        QbzMyQbz.boot()
        QbzMyQbzAdd.boot()
        QbzDisco.boot()
        QbzBlacklist.boot()
        QbzPlaylistPicker.boot()
        // The folder modals. Its boot() also publishes both closed documents,
        // which is what puts the icon-preset and colour-swatch constants on
        // the QML side BEFORE the first open rather than one turn after it.
        QbzFolderEdit.boot()
        // The SHARED playlist editor (rename · description · offline-only ·
        // delete), opened from the manager's three delegates, the sidebar row
        // menu and the playlist detail header's pencil. Its boot() publishes
        // the closed document so the modal's parse sees the full shape.
        QbzPlaylistEdit.boot()
        // Playlist Manager. Its boot() also seeds the default document and
        // kicks the cache-independent folder read, so the SIDEBAR's folder
        // consumers have a list before the manager view has ever been opened.
        QbzPlaylistManager.boot()
        // Playlist Importer. Nothing to seed — the modal is closed until a
        // sidebar row opens it — but WITHOUT this line every publish the
        // importer makes (the whole progress panel, the log, the summary) is
        // dropped on the floor with nothing logged on either side.
        QbzPlaylistImport.boot()

        // Seed the maximized latch ONCE, imperatively — see its declaration for
        // why it must not be a binding on the bridge property.
        window.maximizedLatch = QbzShell.windowMaximized

        // Freeze the clamp latch. The width/height bindings above read these,
        // and `window.screen` CHANGES when the user drags the window to another
        // monitor — a live dependency would re-run those bindings and snap the
        // window back to the restored size under the user's hands (the bindings
        // survive WM resizes, which come from C++ and do not clear them).
        // Assigning a property from imperative JS removes its binding, so
        // after these two lines the latch is a dead number and the geometry
        // bindings can never re-evaluate. Via locals, not self-assignment, so
        // it cannot be mistaken for a no-op.
        var latchedW = window.bootScreenWidth
        var latchedH = window.bootScreenHeight
        window.bootScreenWidth = latchedW
        window.bootScreenHeight = latchedH
    }

    // --- Geometry persistence -------------------------------------------
    // Slint saves from the winit Resized/Moved handlers (main.rs:1399-1454) and
    // relies on a change guard to survive the no-op events a WM emits. QML has
    // no equivalent of "the WM settled", so the debounce stands in for it: a
    // drag fires widthChanged on every frame and only the last one is worth a
    // file write. The floating-only rule, the app minimum and the >0.5px dirty
    // check all live Rust-side in settings_qt::save_window_geometry, so every
    // call site here can stay a single unconditional line.
    //
    // What CANNOT be left to Rust is the maximized flag itself. Qt's
    // `visibility` is ONE enum in which Minimized REPLACES Maximized, while
    // winit — what Slint reads through `is_maximized()` (main.rs:1404-1408) —
    // keeps the two orthogonal and still reports maximized while iconified.
    // Deriving the flag from the live visibility therefore persisted
    // `window_maximized: false` on every minimize, and since the restore only
    // ever re-applies a true (main.rs:8273-8281) BOTH frontends then opened
    // un-maximized. So the flag is LATCHED from the last non-transient
    // visibility, and the transient states persist nothing at all.
    // Seeded imperatively in Component.onCompleted, NOT bound to
    // QbzShell.windowMaximized. cxx-qt generates a NOTIFY for every
    // #[qproperty], so a binding here would be a live reactive edge into the
    // latch: the day anything pushes window_maximized back to QML (syncing the
    // header icon after a WM-side maximize is the obvious future reason), the
    // binding re-fires on any session where the user has not yet toggled
    // maximize and quietly resets the latch to the BOOT value — re-creating
    // exactly the bug below, with no grep signature. Nothing sets it today
    // (`grep set_window_maximized src/` is empty), which is precisely why the
    // invariant belongs here as one local assignment instead of as a
    // cross-file contract nobody will remember.
    property bool maximizedLatch: false
    property bool fullScreenLatch: false
    // A FUNCTION, not a derived property, and that is load-bearing. A
    // `readonly property` computed from `visibility` is dirtied by the very
    // notify signal `onVisibilityChanged` is connected to, and the handler
    // connection runs FIRST, so a read of that property INSIDE the handler is
    // one transition stale. Measured under Qt 6.11 (offscreen, real
    // transitions): the minimize was seen as "not transient" — clearing the
    // latch, the one thing the transient guard exists to prevent — and the
    // following restore as "transient", so the handler returned and never put
    // it back. A maximized session then closed as `window_maximized: false`
    // with the 2556x1436 maximized footprint written into the FLOATING
    // window_width/height (the Rust size gate lets it through precisely
    // because neither flag is set) — Slint #618, the regression
    // settings_qt::save_window_geometry's doc comment says must never happen.
    // A function has no cached value that can be stale: it computes from the
    // enum it is handed, at call time.
    function transientVisibility(vis) {
        return vis === Window.Minimized || vis === Window.Hidden
    }

    // Last size seen in a REAL windowed frame. The shutdown paths need it
    // because a close from the taskbar happens while MINIMIZED: `window.width`
    // is then whatever the WM left behind, and the debounce may still be
    // holding an unsaved drag that has nowhere left to be deferred to. Only
    // captured once armed, so the boot clamp can never enter it. Zero means
    // "never saw one this session" — below the app floor, so Rust drops the
    // size and still records the flags, which is the honest outcome.
    property real lastFloatingWidth: 0
    property real lastFloatingHeight: 0
    function captureFloatingSize() {
        if (!window.geometryArmed || window.visibility !== Window.Windowed)
            return
        // The enum alone is not enough. An un-maximize delivers the state
        // change and the geometry change in whichever order the platform
        // plugin happens to emit them (xdg_toplevel configure carries both), so
        // on a state-first compositor this runs with `visibility` already
        // Windowed while `width`/`height` are still the MAXIMIZED footprint —
        // and the cache would then hand the shutdown path a screen-sized
        // "floating" size, which Rust accepts because neither flag is set.
        // Reject any frame that is still the full screen; the real floating
        // frame arrives a moment later through onWidthChanged.
        var scr = window.screen
        if (scr && window.width >= scr.width - 1 && window.height >= scr.height - 1)
            return
        window.lastFloatingWidth = window.width
        window.lastFloatingHeight = window.height
    }

    // Persistence is ARMED late, on purpose. The restore clamp can hand the
    // window a SMALLER size than the file holds (a 2560px session opened on a
    // 1920px panel) and the WM may adjust the first frames too; persisting
    // those settling frames would lose the big-monitor size after one launch on
    // the small one — for the Slint build as well, since the file is shared.
    // Slint re-saves the clamped size by design ("the Resized handler re-saves
    // the result", main.rs:8208); this is the one place the Qt port
    // deliberately improves on it instead of matching it. No real user resize
    // happens inside the first 1.2s of a cold start, and anything the user does
    // after that persists normally.
    property bool geometryArmed: false
    Timer {
        id: geometryArmTimer
        interval: 1200
        running: true
        repeat: false
        onTriggered: window.geometryArmed = true
    }

    function persistWindowGeometry() {
        if (!window.geometryArmed)
            return
        // Transient: FLUSH the cached floating size, do not drop it and do not
        // defer it. Dropping was the original bug (a drag that debounced into a
        // minimize was lost; Slint writes synchronously from the winit Resized
        // handler and cannot lose it). Re-arming the timer instead was the
        // over-correction: the re-arm is itself what keeps the save
        // outstanding, so a window left minimized to the tray polls at 2.5 Hz
        // forever — measured at 15 wakeups in 6 s, i.e. ~72,000 over a workday.
        // Flushing the cache ends both problems in one line: `window.width` is
        // untrustworthy while iconified, but `lastFloatingWidth` is the last
        // REAL windowed frame, and Rust's floating-only + app-minimum + >0.5px
        // dirty gate turns a zero or stale cache into a no-op by itself.
        if (window.transientVisibility(window.visibility)) {
            QbzShell.saveWindowGeometry(window.lastFloatingWidth,
                                        window.lastFloatingHeight,
                                        window.maximizedLatch,
                                        window.fullScreenLatch)
            return
        }
        QbzShell.saveWindowGeometry(window.width,
                                    window.height,
                                    window.maximizedLatch,
                                    window.fullScreenLatch)
    }

    // Shutdown variant. Nothing can be deferred once the app is going away, so
    // instead of skipping the write, a transient window persists the last
    // floating size we actually saw. The flags travel either way — they are
    // what the restore reads (main.rs:8273-8281).
    function persistWindowGeometryOnExit() {
        // The 1.2s arming window guards the SIZE against boot-settling frames.
        // It must not swallow the FLAGS: a boot clamp never changes visibility,
        // so a maximize/restore inside the first 1.2s is a genuine user action,
        // and it is the flag — not the size — that both frontends read back
        // (main.rs:8273-8281). Persist the flags with a 0x0 size, which is
        // below the app floor, so Rust records them and drops the size.
        if (!window.geometryArmed) {
            QbzShell.saveWindowGeometry(0, 0,
                                        window.maximizedLatch,
                                        window.fullScreenLatch)
            return
        }
        if (!window.transientVisibility(window.visibility)) {
            window.persistWindowGeometry()
            return
        }
        QbzShell.saveWindowGeometry(window.lastFloatingWidth,
                                    window.lastFloatingHeight,
                                    window.maximizedLatch,
                                    window.fullScreenLatch)
    }

    Timer {
        id: geometrySaveTimer
        interval: 400
        repeat: false
        onTriggered: window.persistWindowGeometry()
    }

    onWidthChanged: { window.captureFloatingSize(); geometrySaveTimer.restart() }
    onHeightChanged: { window.captureFloatingSize(); geometrySaveTimer.restart() }
    // Maximize/restore/fullscreen: the flag itself is the thing that changed,
    // and the sizes that come with it are deliberately not persisted. A
    // minimize or a hide is NOT a state change to record — it is the same
    // window in a transient mode, so the latch keeps its last real value and
    // the debounce is not even restarted.
    onVisibilityChanged: {
        // Read the enum ONCE into a local and decide everything from it — see
        // `transientVisibility` for why nothing here may consult a property
        // derived from `visibility`.
        var vis = window.visibility
        if (window.transientVisibility(vis))
            return
        // FLUSH BEFORE LATCHING. Leaving Windowed for Maximized/FullScreen with
        // a drag still in the debounce used to lose that drag outright: the
        // handler latched the new state and restarted the SAME timer, so when
        // it fired it handed Rust the post-transition size with maximized true
        // — and the floating-only gate refuses a size on exactly that
        // condition. The exit path could not rescue it either, since it passes
        // the same latches. Proven by effect: resize 1180->1600, maximize
        // 157ms later, quit — the file kept 1180x760 while Slint, for the same
        // event stream, kept 1600x900. Commit the floating frame we already
        // hold before the latches stop describing it.
        if (window.geometryArmed && geometrySaveTimer.running
                && !window.maximizedLatch && !window.fullScreenLatch
                && vis !== Window.Windowed) {
            geometrySaveTimer.stop()
            QbzShell.saveWindowGeometry(window.lastFloatingWidth,
                                        window.lastFloatingHeight,
                                        false, false)
        }
        // Maximized and fullscreen are ORTHOGONAL in winit, which is what
        // Slint persists. ASSUMED, NOT MEASURED: on X11
        // `_NET_WM_STATE_MAXIMIZED_*` is expected to survive a fullscreen
        // transition under Mutter/KWin, so `is_maximized()`
        // (main.rs:1404-1408) would keep reporting true from fullscreen. On
        // Wayland a compositor may instead send a states array carrying only
        // `fullscreen`, in which case winit reports false and the two
        // frontends would write OPPOSITE values into the shared
        // window_maximized. Settling this costs one measurement, not an
        // argument: run the Slint build, go maximized -> WM fullscreen, and log
        // `w.window().is_maximized()` from the Resized handler. If it reports
        // false, clear the maximized latch in the FullScreen branch too. Qt
        // collapses both into one enum, so the maximized latch is cleared ONLY
        // by an explicit return to Windowed — never by entering fullscreen.
        // Nothing in QBZ's chrome sets FullScreen (both toggles go
        // Maximized <-> Windowed, shell/HeaderBar.qml:76,830); it arrives from
        // the WM keybind, and quitting from there must not un-maximize the
        // next launch of either frontend.
        if (vis === Window.Maximized) {
            window.maximizedLatch = true
            window.fullScreenLatch = false
        } else if (vis === Window.FullScreen) {
            window.fullScreenLatch = true
        } else if (vis === Window.Windowed) {
            window.maximizedLatch = false
            window.fullScreenLatch = false
        }
        // An un-maximize delivers its size change and its visibility change in
        // either order; capturing here as well as in onWidthChanged means the
        // floating cache is right whichever arrives second.
        window.captureFloatingSize()
        geometrySaveTimer.restart()
    }

    // Two shutdown paths, because the header's close button calls Qt.quit()
    // (shell/HeaderBar.qml) which never raises `closing`, while a WM close or
    // Alt+F4 does. Both end up in the same dirty check, so the duplicate is
    // free — and either way a resize in the last 400ms still lands, including
    // the taskbar close of a minimized window (that is what the exit variant
    // and the floating cache are for).
    onClosing: window.persistWindowGeometryOnExit()
    Connections {
        target: Qt.application
        function onAboutToQuit() { window.persistWindowGeometryOnExit() }
    }




    // The immersive toggle Shortcut is GONE (2026-08-03 hotkeys-port §1.3):
    // Shift+I now fires as the REBINDABLE ui.focusMode action through the
    // AppShell dispatcher (QbzHotkeys.keyPressed), whose central text-input
    // gate replaces the `enabled:` expression this block carried.
    // NavGestureLayer.qml owns mouse Back/Forward; no other Shortcut item
    // exists in the tree (trap 11).

    Loader {
        id: screenLoader
        anchors.fill: parent
        active: QbzSession.screen !== "splash"
        source: QbzSession.screen === "login"
                ? "LoginScreen.qml"
                : (QbzSession.screen === "shell" ? "shell/AppShell.qml" : "")
        // Hand the host window down for drag/maximize/resize (custom chrome).
        onLoaded: if (screenLoader.item) screenLoader.item.hostWindow = window
    }

    // Frameless hairline (app.slint's no-frame 1px edge) — paints at the
    // very rim, over everything. SQUARE (the app draws no corner rounding).
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: 1
        border.color: "#14ffffff"
        visible: !QbzShell.systemTitleBar
            && window.visibility !== Window.Maximized && window.visibility !== Window.FullScreen
    }

    // Edge/corner resize grips (custom chrome — the compositor draws no
    // border). 6px, invisible.
    MouseArea {
        anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 6
        cursorShape: Qt.SizeHorCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 6
        cursorShape: Qt.SizeHorCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.RightEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: 6
        cursorShape: Qt.SizeVerCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.TopEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 6
        cursorShape: Qt.SizeVerCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.BottomEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.top: parent.top; width: 12; height: 12
        cursorShape: Qt.SizeFDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.TopEdge | Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.top: parent.top; width: 12; height: 12
        cursorShape: Qt.SizeBDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.TopEdge | Qt.RightEdge)
    }
    MouseArea {
        anchors.left: parent.left; anchors.bottom: parent.bottom; width: 12; height: 12
        cursorShape: Qt.SizeBDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.BottomEdge | Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right; anchors.bottom: parent.bottom; width: 12; height: 12
        cursorShape: Qt.SizeFDiagCursor
        enabled: !QbzShell.systemTitleBar
        onPressed: window.startSystemResize(Qt.BottomEdge | Qt.RightEdge)
    }

    // Splash (SplashScreen.slint): the same 720px dark card as the login
    // screen while the silent session restore resolves.
    Rectangle {
        anchors.fill: parent
        color: "#0f0f0f"
        visible: QbzSession.screen === "splash"

        Rectangle {
            anchors.centerIn: parent
            width: 720
            height: splashColumn.height + 104
            radius: 16
            color: "#1a1a1a"

            Column {
                id: splashColumn
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 52
                spacing: 0
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: "assets/qbz-logo.png"
                    width: 140
                    height: 140
                    fillMode: Image.PreserveAspectFit
                }
                Item { width: 1; height: 8 }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "QBZ"
                    color: "#ffffff"
                    font.pixelSize: 29
                    font.weight: Font.DemiBold
                    font.letterSpacing: 8
                }
                Item { width: 1; height: 2 }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "QOBUZ™ PLAYER"
                    color: "#888888"
                    font.pixelSize: 15
                    font.letterSpacing: 4
                }
                Item { width: 1; height: 32 }
                QbzSpinner {
                    anchors.horizontalCenter: parent.horizontalCenter
                    size: 32
                }
            }
        }
    }
}
