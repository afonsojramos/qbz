// KioskShell — the small-panel touch shell (QML port of
// crates/qbz-ui/ui/shell/KioskShell.slint, 748 lines).
//
// A SIBLING of AppShell, mounted by Main.qml when QbzSession.screen is
// "kiosk". It reads the SAME bridges as the desktop shell — nothing is
// mirrored — so the two swap live with nothing torn down. The only
// differences are chrome: a thin back bar + a bottom NavRail replace the
// HeaderBar + Sidebar, the transport is forced to NowPlayingBarSmall, and
// there is no queue/lyrics side panel.
//
// Structure, top to bottom (KioskShell.slint:229-747). The last three are
// overlays drawn on top of the layout, and the ordering is load-bearing:
//   1. Column: back bar (42) / content frame / transport (42, conditional) /
//      NavRail (80)                                       KioskShell.slint:229-659
//   2. the player-zone focus ring overlay                              :666-677
//   3. the root key handler + the deferred initial focus grab      :684-735
//   4. the kiosk's OWN Immersive mount                                 :741-747
//
// The view mounts live in the SHARED shell/ContentRouter.qml (contract §4.2,
// divergence D3) — the Slint's verbatim copy of AppShell's block declares
// that extraction as its own follow-up (KioskShell.slint:10-17).

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../kiosk"
import "../theme"

Rectangle {
    id: root

    // KioskShell.slint:58 sets NO background — every visible surface is
    // painted by a child, and what shows through is the window's own fill
    // (Main.qml:93, the same surface-card the back bar uses).
    color: "transparent"

    // The host ApplicationWindow. Main.qml:463 assigns this UNCONDITIONALLY
    // on load, so the property must exist or the shell fails to mount
    // (the desktop declaration is shell/AppShell.qml:59).
    property var hostWindow: null

    // The shell-root marker six components walk the parent chain for
    // (immersive/ImmersiveView.qml:93-99 hands focus BACK to whatever carries
    // it). Without it every hotkey after an immersive close is swallowed.
    // `focus` + the explicit grab are AppShell.qml:69,78's hard-won pair:
    // declarative focus alone did not survive the splash -> shell Loader swap
    // in a WM-less session.
    readonly property bool isQbzShellRoot: true
    focus: true
    Component.onCompleted: root.forceActiveFocus()

    QbzTheme { id: theme }

    // KioskShell.slint:204 — the NavRail is a fat touch target.
    readonly property int navRailHeight: 80

    // The chrome hover fill (Theme.alpha-8, KioskShell.slint:255,273). The
    // ramp is empty on the pre-publish frame, where alphaTier() silently
    // returns "transparent" — the tree's required guard idiom, whose reason
    // is written out at rows/TrackRow.qml:264-269.
    readonly property color chromeHoverBg: theme.alphaTiers.length > 0
        ? theme.alphaTier(8)
        : (theme.isDark ? "#14ffffff" : "#14000000")

    // ---------------------------------------------------------------------
    // The offline gate (KioskShell.slint:209-227)
    // ---------------------------------------------------------------------
    // Qobuz-only routes mount only online; the local-DB routes (Local
    // Library, local album, My QBZ, Settings, Now Playing, the manager
    // views) keep mounting offline and are deliberately absent below.
    //
    // Two arms of the reference's predicate have no counterpart here:
    //   - musician / award / award-albums / purchases / purchase-album /
    //     location are ABSENT-BY-RULING (contract R1) — there is no Qt view
    //     and no Qt route id for them, so they can never be `currentView`.
    //   - PlaylistState.offline-subset (the D11.a "mixed Qobuz playlist
    //     rendered from its local sidecar rows" flag, state.slint:2183) has
    //     NO Qt member: playlist_qt.rs's PlaylistDoc carries isLocalPlaylist
    //     (:175) and offlineOnly (:181) and nothing else. The arm below is
    //     therefore the reference's minus that one disjunct.
    readonly property var playlistDoc: root.parsePlaylistDoc()
    function parsePlaylistDoc() {
        try {
            return JSON.parse(QbzBridge.playlistJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property bool qobuzViewBlocked: QbzSession.offline && (
           QbzShell.currentView === "home"
        || QbzShell.currentView === "discoverbrowse"
        || QbzShell.currentView === "search"
        || QbzShell.currentView === "library"
        || QbzShell.currentView === "album"
        || QbzShell.currentView === "artist"
        || QbzShell.currentView === "artistreleases"
        || QbzShell.currentView === "label"
        || QbzShell.currentView === "labelreleases"
        || QbzShell.currentView === "mix"
        || (QbzShell.currentView === "playlist"
            && root.playlistDoc.isLocalPlaylist !== true))

    // ---------------------------------------------------------------------
    // NavRail routing (KioskShell.slint:637-657)
    // ---------------------------------------------------------------------
    // The Slint tiles fire `header-menu-navigate("discover-home")` etc., and
    // the Rust handler behind that route both switches the view AND selects a
    // landing tab (crates/qbz/src/main.rs:18017-18040). Qt has no such
    // combined seam — QbzShell.navigateTo carries only the route — so the
    // landing tab uses the port's existing pending-tab handshake
    // (shell/NavFlyout.qml:263-291): remember the tab, wait for the route to
    // land, stamp it on the mounted view. The view is reached through
    // ContentRouter's public `currentItem` rather than NavFlyout's depth-
    // capped tree walk, which the kiosk's deeper mount chain would outrun.
    property string pendingTab: ""
    property string pendingView: ""

    function navigateWithTab(view, tab) {
        if (QbzShell.currentView === view) {
            // Already mounted — no currentViewChanged will fire.
            root.applyTab(tab)
            return
        }
        root.pendingTab = tab
        root.pendingView = view
        QbzShell.navigateTo(view)
    }

    // Duck-typed exactly like NavFlyout.qml:320-321: only the tabbed views
    // expose `activeTab`, and a view without one is simply left alone.
    function applyTab(tab) {
        if (!tab || tab === "")
            return
        var mounted = contentLoader.item !== null ? contentLoader.item.currentItem : null
        if (mounted !== null && typeof mounted.activeTab === "string")
            mounted.activeTab = tab
    }

    Connections {
        target: QbzShell
        function onCurrentViewChanged() {
            // KioskShell.slint:194-200 — every route change parks the focus
            // ring on the new view's first item. nav_active is NOT reset: the
            // latch is a session-level thing (kiosk_nav_qt.rs:233-242).
            QbzKioskNav.resetForView()

            if (root.pendingTab === "")
                return
            var wanted = root.pendingView
            var tab = root.pendingTab
            root.pendingTab = ""
            root.pendingView = ""
            // A navigation somewhere else beat us to it — drop the request
            // instead of stamping a stale tab later.
            if (QbzShell.currentView !== wanted)
                return
            // The router instantiates the view from its own binding on the
            // same property; binding order between it and this handler is
            // undefined, so apply after the current pass.
            Qt.callLater(function () { root.applyTab(tab) })
        }
    }

    // =====================================================================
    // 1. The layout column (KioskShell.slint:229-659)
    // =====================================================================
    Column {
        id: shellColumn
        width: root.width
        spacing: 0

        // --- Back bar (KioskShell.slint:237-319) -------------------------
        // Persistent global back/forward (ADR-004) + the kiosk search entry.
        // The desktop search box lives in the HeaderBar, which the kiosk
        // drops, so this field is what makes Search reachable at all.
        Rectangle {
            id: backBar
            width: shellColumn.width
            height: theme.headerHeight
            color: theme.surfaceCard

            // The kiosk carries no HeaderBar, so it owns the window chrome
            // too (KioskShell.slint:240-245): macOS overlay leaves the native
            // traffic-light inset, Linux frameless draws the controls.
            //
            // DIVERGENCE, reported: the reference reads
            // AppearanceState.is-macos and .show-window-controls. Neither has
            // a Qt bridge member (QbzShell exposes only systemTitleBar,
            // shell_bridge.rs:111), and this port must not add Rust here. The
            // platform half uses Qt's own Qt.platform.os; the pref half is
            // dropped, which puts the kiosk on exactly the predicate the
            // desktop chrome already uses (HeaderBar.qml:924).
            readonly property real macInset:
                (Qt.platform.os === "osx" && !QbzShell.systemTitleBar) ? 78 : 0
            readonly property bool linChrome:
                Qt.platform.os !== "osx" && !QbzShell.systemTitleBar

            // padding-left 6 + macInset, padding-right 118 (controls) or 8,
            // padding-top/bottom 4, spacing 6 -> a 34px content row.
            // The 118 is not a round number: WindowControls is 34*3 + 2*2 =
            // 106 wide, pinned 8px from the right edge, leaving 4px of
            // clearance to the search field.
            Row {
                id: backBarRow
                x: 6 + backBar.macInset
                y: 4
                width: backBar.width - (6 + backBar.macInset)
                       - (backBar.linChrome ? 118 : 8)
                height: backBar.height - 8
                spacing: 6

                // KioskShell.slint:252-269. The disabled state is expressed
                // THREE times — dimmed, un-hoverable and un-clickable; an
                // opacity-only port leaves a clickable ghost.
                Rectangle {
                    id: backBtn
                    width: 44
                    height: backBarRow.height
                    radius: theme.radiusSm
                    color: (backArea.containsMouse && QbzShell.canBack)
                           ? root.chromeHoverBg : "transparent"
                    opacity: QbzShell.canBack ? 1.0 : 0.32
                    QbzIcon {
                        name: "chevron-left"
                        width: 22
                        height: 22
                        anchors.centerIn: backBtn
                        tintName: "secondary"
                    }
                    MouseArea {
                        id: backArea
                        anchors.fill: backBtn
                        enabled: QbzShell.canBack
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzShell.navigateBack()
                    }
                }

                // KioskShell.slint:270-287.
                Rectangle {
                    id: fwdBtn
                    width: 44
                    height: backBarRow.height
                    radius: theme.radiusSm
                    color: (fwdArea.containsMouse && QbzShell.canForward)
                           ? root.chromeHoverBg : "transparent"
                    opacity: QbzShell.canForward ? 1.0 : 0.32
                    QbzIcon {
                        name: "chevron-right"
                        width: 22
                        height: 22
                        anchors.centerIn: fwdBtn
                        tintName: "secondary"
                    }
                    MouseArea {
                        id: fwdArea
                        anchors.fill: fwdBtn
                        enabled: QbzShell.canForward
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzShell.navigateForward()
                    }
                }

                // KioskShell.slint:288-312. Submit-only: there is NO live /
                // debounced search in the back bar and no cortinilla — do not
                // add one.
                QbzLineEdit {
                    id: searchField
                    width: backBarRow.width - backBtn.width - fwdBtn.width
                           - 2 * backBarRow.spacing
                    height: backBarRow.height
                    placeholder: QbzSession.tr("Search", QbzSession.trRev)
                    // The full accept sequence, KioskShell.slint:301-311.
                    // Rust's search_submit navigates to the results view
                    // itself (search_qt.rs:1971), which is the reference's
                    // explicit `NavState.view = ContentView.search` (:303).
                    // Slint then needs TWO more steps — clear-focus() so the
                    // OS on-screen keyboard closes (:307), then keys.focus()
                    // to hand the arrows back (:310). In Qt keyboard focus is
                    // exclusive, so grabbing it at the root does both at once:
                    // the inner TextInput loses activeFocus (text-input
                    // DISABLE -> squeekboard hides) and the nav scope has it.
                    onAccepted: function (value) {
                        QbzSearch.searchSubmit(value)
                        root.forceActiveFocus()
                    }
                }
            }

            // Linux custom window controls at the right edge
            // (KioskShell.slint:315-318). Inlined rather than extracted from
            // HeaderBar.qml:923-999: a shared component would need a new file
            // in build.rs, and touching HeaderBar risks the desktop
            // non-regression guard (contract D3). Same three 34x26 buttons,
            // same 2px spacing, same fixed #e81123 close hover.
            Row {
                id: windowControls
                visible: backBar.linChrome
                x: backBar.width - width - 8
                y: (backBar.height - height) / 2
                height: 26
                spacing: 2

                Rectangle {
                    id: wcMin
                    width: 34
                    height: 26
                    radius: theme.radiusSm
                    color: wcMinArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        name: "minus"
                        width: 14
                        height: 14
                        anchors.centerIn: wcMin
                        tintName: "secondary"
                    }
                    MouseArea {
                        id: wcMinArea
                        anchors.fill: wcMin
                        hoverEnabled: true
                        onClicked: if (root.hostWindow) root.hostWindow.showMinimized()
                    }
                }
                Rectangle {
                    id: wcMax
                    width: 34
                    height: 26
                    radius: theme.radiusSm
                    color: wcMaxArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        // The glyph swaps while maximized
                        // (WindowControls.slint:52-54).
                        name: root.hostWindow
                              && root.hostWindow.visibility === Window.Maximized
                              ? "minimize-2" : "maximize-2"
                        width: 14
                        height: 14
                        anchors.centerIn: wcMax
                        tintName: "secondary"
                    }
                    MouseArea {
                        id: wcMaxArea
                        anchors.fill: wcMax
                        hoverEnabled: true
                        onClicked: {
                            if (root.hostWindow) {
                                root.hostWindow.visibility =
                                    root.hostWindow.visibility === Window.Maximized
                                    ? Window.Windowed : Window.Maximized
                            }
                        }
                    }
                }
                Rectangle {
                    id: wcClose
                    width: 34
                    height: 26
                    radius: theme.radiusSm
                    color: wcCloseArea.containsMouse ? "#e81123" : "transparent"
                    QbzIcon {
                        name: "x"
                        width: 14
                        height: 14
                        anchors.centerIn: wcClose
                        // The hover fill is a FIXED #e81123 under every
                        // theme, so the hovered glyph is literal white.
                        tintName: wcCloseArea.containsMouse ? "white" : "secondary"
                    }
                    MouseArea {
                        id: wcCloseArea
                        anchors.fill: wcClose
                        hoverEnabled: true
                        // The FIFTH exit. §5.7 of the 2026-08-03 miniplayer/
                        // tray contract enumerates four; this shell grew its
                        // own drawn X after that contract was written, and an
                        // exit still calling Qt.quit() directly is precisely
                        // the half-port the enumeration exists to prevent. Same
                        // choreography as the desktop chrome: Main.qml's
                        // closeOrHide hides to the tray while QbzTray.trayLive
                        // && QbzTray.closeToTray and quits otherwise.
                        onClicked: if (root.hostWindow) root.hostWindow.closeOrHide(null)
                    }
                }
            }
        }

        // --- Content frame (KioskShell.slint:325-611) --------------------
        // The outer surface-card frame takes the leftover height; the inner
        // surface-main panel is inset 8px left/right/bottom (0 top — it butts
        // the back bar) and takes an EXPLICIT height, because a
        // conditionally-mounted view does not stretch to fill a layout cell.
        //
        // THE NOW-PLAYING EXCEPTION (:348-355): the transport row is hidden
        // on that view, so its 42px must NOT be subtracted — otherwise the
        // content stays short and the NavRail rides up, leaving dead space
        // below the footer.
        Rectangle {
            id: contentFrame
            width: shellColumn.width
            height: root.height - theme.headerHeight
                    - (QbzShell.currentView === "nowplaying" ? 0 : theme.headerHeight)
                    - root.navRailHeight
            color: theme.surfaceCard

            // Tap anywhere in the content area to dismiss the OS on-screen
            // keyboard (KioskShell.slint:328-339): releasing the field's
            // focus makes Qt send text-input DISABLE and squeekboard closes.
            // Declared FIRST so it sits BEHIND the views — later siblings get
            // the event first, so this only catches empty-area taps.
            MouseArea {
                anchors.fill: contentFrame
                onClicked: root.forceActiveFocus()
            }

            Rectangle {
                id: contentPanel
                x: 8
                y: 0
                width: contentFrame.width - 16
                height: contentFrame.height - 8
                color: theme.surfaceMain
                radius: theme.radiusMd
                clip: true

                // The offline placeholder (KioskShell.slint:362-365). The
                // reference's `favorites` variant nests a FavoritesOfflineRail
                // inside it; that rail is ABSENT-BY-RULING (contract R1), so
                // the plain placeholder is the correct kiosk rendering for
                // every blocked route, favourites included.
                QbzOfflinePlaceholder {
                    visible: root.qobuzViewBlocked
                    anchors.centerIn: contentPanel
                    showSettingsAction: true
                    onSettingsClicked: QbzShell.navigateTo("settings")
                }

                // The shared router, in its kiosk arm. `active` rather than
                // `visible`: the reference UNMOUNTS the view while the gate is
                // up (every Qobuz route carries its own `&& !offline` guard,
                // :367-608), and an invisible view would keep running.
                Loader {
                    id: contentLoader
                    anchors.fill: contentPanel
                    active: !root.qobuzViewBlocked
                    sourceComponent: kioskRouter
                }
            }
        }

        // --- The transport (KioskShell.slint:617-631) --------------------
        // Forced to the small bar, exactly one header row tall. Hidden on Now
        // Playing — that view carries the full transport, so the bar would be
        // redundant there.
        Loader {
            id: transport
            width: shellColumn.width
            height: transport.active ? theme.headerHeight : 0
            active: QbzShell.currentView !== "nowplaying"
            visible: transport.active
            sourceComponent: transportBar
        }

        // --- Bottom NavRail (KioskShell.slint:635-658) -------------------
        NavRail {
            id: navRail
            width: shellColumn.width
            height: root.navRailHeight

            onDiscover: root.navigateWithTab("home", "home")
            onLibrary: root.navigateWithTab("library", "tracks")
            onLocalLibrary: root.navigateWithTab("local", "albums")
            onMyqbz: QbzShell.navigateTo("mixtapes")
            // The reference assigns NavState.view directly here (:650), so it
            // records NO history entry. Qt's only view-switch path is
            // navigateTo, which always records (shell_bridge.rs:645 ->
            // main.rs:1682); the extra history entry is the accepted
            // divergence rather than a second Rust entry point.
            onPlaying: QbzShell.navigateTo("nowplaying")
            // R2 (contract §0): the tile's only action in the reference is
            // opening the Immersive overlay (:652-654), and the kiosk's
            // Immersive mount is the Immersive task's deliverable (the Loader
            // at the bottom of this file). Until that task lands the ACTION is
            // a sanctioned no-op — the tile's pixels and its focus ring are
            // NOT deferred. Wiring QbzImmersive here instead would be worse
            // than nothing: `open` would go true over an empty mount and the
            // key guard below would then reject every key against a surface
            // that renders nothing.
            onVisualizer: {
                // Intentionally empty — see the note above.
            }
            onMenu: QbzShell.navigateTo("settings")
        }
    }

    // =====================================================================
    // 2. The player-zone focus ring (KioskShell.slint:666-677)
    // =====================================================================
    // An overlay in ROOT coordinates, outside the column, so it never
    // reflows anything. The transport is ONE focusable unit; on Now Playing
    // the bar is hidden and that view draws its own in-page ring instead.
    Rectangle {
        id: playerRing
        visible: QbzShell.currentView !== "nowplaying"
                 && QbzKioskNav.navActive
                 && QbzKioskNav.zone === "player"
        x: 0
        y: root.height - root.navRailHeight - theme.headerHeight
        width: root.width
        height: theme.headerHeight
        border.width: 3
        border.color: theme.accent
        radius: theme.radiusSm
        color: Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.08)
    }

    // =====================================================================
    // 3. Root key handling (KioskShell.slint:684-723)
    // =====================================================================
    // One scope drives the whole kiosk: arrows move the ring through the
    // three zones (rail -> content -> player -> rail on Down, reversed on
    // Up), Enter activates, Esc goes back. MouseAreas never take keyboard
    // focus, so this survives every card/tile tap and only yields to the
    // search field.
    //
    // Evaluation order is literal and must be preserved. The reference's two
    // guards RETURN REJECT — the key keeps its normal path — and in Qt that
    // path is the hotkeys dispatcher at the end of this handler, whose own
    // central text-input gate (hotkeys_bridge.rs:295-299) is the counterpart
    // of the first guard. The textInputFocused predicate is computed exactly
    // as the desktop shell computes it (AppShell.qml:91).
    Keys.onPressed: function (event) {
        var w = root.Window.window
        var afi = w !== null ? w.activeFocusItem : null
        var textInputFocused = (afi instanceof TextInput) || (afi instanceof TextEdit)

        if (!textInputFocused && !QbzImmersive.open) {
            if (event.key === Qt.Key_Left) {
                QbzKioskNav.navLeft()
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_Right) {
                QbzKioskNav.navRight()
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_Up) {
                QbzKioskNav.navUp()
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_Down) {
                QbzKioskNav.navDown()
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                // The three-way dispatch, KioskShell.slint:175-190. The rail
                // is always mounted, so it is called directly; a content view
                // is conditionally mounted and cannot be addressed from here,
                // so its request goes out as the activate-seq pulse. Both the
                // rail and the player arm still have to latch the ring (:176)
                // — bumpActivate() does it for the content arm itself
                // (kiosk_nav_qt.rs:222-225).
                if (QbzKioskNav.zone === "player") {
                    QbzKioskNav.markActive()
                    if (QbzPlayer.npHasTrack)
                        QbzPlayer.togglePlay()
                } else if (QbzKioskNav.zone === "rail") {
                    QbzKioskNav.markActive()
                    navRail.activateIndex(QbzKioskNav.index)
                } else {
                    QbzKioskNav.bumpActivate()
                }
                event.accepted = true
                return
            }
            // Escape only CONSUMES when there is history; otherwise it keeps
            // its normal path (:714-720), which here is the dispatcher's own
            // Escape stack.
            if (event.key === Qt.Key_Escape && QbzShell.canBack) {
                QbzShell.navigateBack()
                event.accepted = true
                return
            }
        }

        event.accepted = QbzHotkeys.keyPressed(event.key, event.modifiers,
                                               event.text, textInputFocused)
    }

    // Deferred mount focus (KioskShell.slint:728-735): synchronous focus
    // during the initial layout races it, so a one-shot timer grabs it a tick
    // later. The port already uses this exact 30ms idiom at
    // controls/QbzLineEdit.qml:123-128.
    Timer {
        interval: 30
        running: true
        repeat: false
        onTriggered: root.forceActiveFocus()
    }

    // =====================================================================
    // 4. The kiosk's OWN Immersive mount (KioskShell.slint:737-747)
    // =====================================================================
    // The desktop hangs its copy inside AppShell, so the kiosk shell must
    // mount its own; declared LAST = on top of everything.
    //
    // SANCTIONED EMPTY MOUNT (contract §4.6 / R2): the kiosk Immersive
    // surface is the Immersive task's deliverable and this Loader is the seam
    // it lands in. The key-rejection arm above (QbzImmersive.open) ships NOW
    // so the two compose the moment this gets a sourceComponent.
    Loader {
        id: immersiveMount
        anchors.fill: root
        active: QbzImmersive.open
    }

    // The two conditionally-mounted children, hoisted out of the column so
    // the positioner never sees a non-visual child.
    Component {
        id: kioskRouter
        ContentRouter { kiosk: true }
    }
    Component {
        id: transportBar
        NowPlayingBarSmall { }
    }
}
