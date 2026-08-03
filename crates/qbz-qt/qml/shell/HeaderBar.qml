// Top header bar — QML port of crates/qbz-ui/ui/shell/HeaderBar.slint.
//
// Left: the three "sacred" nav buttons (sidebar cycle, back, forward) and,
// after them, the section nav in whichever form the placement prefs ask for
// (HeaderBar.slint:858 / :974 — the two blocks are mutually exclusive):
//
//   navInSidebar ON   -> the sections live in Sidebar.qml. The header shows
//                        the COMPACT icon nav only while the sidebar is fully
//                        closed (state 2), plus the separator + playlists
//                        flyout button, so nothing is unreachable.
//   navInSidebar OFF  -> the sections live HERE. Full text tabs while the
//                        sidebar is not fully closed and navHeaderCompact is
//                        OFF; the compact icon form when navHeaderCompact is
//                        ON, or whenever the sidebar is fully closed.
//
// The search field gives up 60px whenever the nav is in the header, and
// re-centers with a 220ms animation (HeaderBar.slint:569).
// Center: the search field, absolutely centered on the window. It owns the
// live-search input path: the >= 2-char gate and the dismiss below it, the
// arrow/Enter/Escape rules, the ↵ affordance and clearSearch(). The 220ms
// debounce is NOT here — it lives in Rust (search_qt::live), so the panel
// opens on the first keystroke instead of 220ms after the last one. The
// dropdown itself is shell/Cortinilla.qml.
// Right: the tri-state offline status badge with its flyout (recovery
// "Sign in" wired to QbzSession.recoveryLogin) and the app menu (user block
// + Keyboard Shortcuts + Documentation + Log Out + Close. Still missing vs
// Slint, each needing a surface the port does not have yet: Open Music Link,
// Report an Issue, What's New, About QBZ.
//
// POC-NOTE: the custom window-chrome parts (drag surface, drawn
// min/max/close WindowControls) are skipped — the POC keeps NATIVE window
// decorations.
//
// FILE LENGTH (>500): this is ONE bar with one placement state machine —
// `navInSidebar` x `navHeaderCompact` x the three sidebar widths decide which
// of the header's forms is mounted, and every block below reads that same
// state. The parts that are self-contained have already been extracted and are
// SHARED with the sidebar rather than duplicated: the section dropdown
// (shell/NavFlyout.qml), its leading glyph (shell/NavSectionGlyph.qml), the
// nav buttons (controls/QbzNavButton.qml) and the hover bubble
// (controls/QbzTooltip.qml, mounted once by AppShell). What remains is the
// bar's own layout — left controls, the absolutely-centred search, the status
// badge, the window controls and the app menu — each a positional slot of this
// single row, so splitting them would hand every half the same placement
// properties to keep in sync while none of them is reusable anywhere else.
// The closed-sidebar playlists flyout is the one candidate left: it is
// self-contained, but it reads `plBtn` for its anchor and the sidebar's shared
// search query, so it moves the day a second surface needs it.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root
    // surface-card @ 0.5 while the ambient background is active (phase 14,
    // HeaderBar.slint's with-alpha(app-background-surface-alpha)).
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
       

    // The host ApplicationWindow (custom chrome); null in previews.
    property var hostWindow: null

    QbzTheme { id: theme }

    // The section-nav dropdown (catalog + panel + open/close behaviour).
    // Zero-size and NOT inside any Row — its origin must stay this root's
    // origin, because the trigger coordinates are mapped into it. The panel
    // itself renders in the window overlay, so it is not clipped by the 42px
    // header (Slint solves the same problem by mounting HeaderMenuOverlay as
    // the last child of the AppShell root).
    NavFlyout { id: navFlyout }

    // Custom-chrome drag surface (declared FIRST so every interactive
    // element above wins hit-testing): press-and-move starts the system
    // move; double-click toggles maximize. The system grab starts only
    // after a real movement so plain clicks/double-clicks still work.
    MouseArea {
        anchors.fill: parent
        // Inert under the system titlebar (the native chrome owns
        // drag/double-click) — phase 12 titlebar toggle.
        enabled: !QbzShell.systemTitleBar
        property bool dragStarted: false
        onPressed: dragStarted = false
        onPositionChanged: {
            if (pressed && !dragStarted && root.hostWindow) {
                dragStarted = true
                root.hostWindow.startSystemMove()
            }
        }
        onDoubleClicked: {
            if (root.hostWindow) {
                root.hostWindow.visibility = root.hostWindow.visibility === Window.Maximized
                    ? Window.Windowed : Window.Maximized
            }
        }
    }

    // Slint OfflineState.badge-state: 0 hidden / 1 hard offline / 2 manual /
    // 3 logged out (wins over the others).
    readonly property int badgeState: QbzSession.offlineSession ? 3
        : QbzSession.offlineMode === 2 ? 2
        : QbzSession.offlineMode === 1 ? 1 : 0

    // --- Section nav (shared by both header forms) ------------------------
    // The catalog (sections, their entries, icons and order) and the flyout
    // panel itself now live in ONE place, NavFlyout.qml — mounted above as
    // `navFlyout`, and mounted the same way by Sidebar.qml. Slint duplicates
    // the same literals three times (Sidebar.slint:724, HeaderBar.slint:858
    // full tabs, :974 compact buttons); the port does not.
    //
    // Triggers no longer navigate: like their Slint counterparts they OPEN the
    // section dropdown (on hover and on click), and the ENTRIES navigate.
    readonly property var navSections: navFlyout.sections

    // Highlighted section — derived from the live view (see NavFlyout), OR'd
    // in the triggers with "my menu is open" (Slint highlights off
    // HeaderMenuState.open-index).
    readonly property string activeNav: navFlyout.activeSection

    // Which of the two header forms is mounted (mutually exclusive, and both
    // off while the nav lives in the sidebar and the sidebar is not closed).
    readonly property bool headerTabsOn: !QbzShell.navInSidebar
        && QbzShell.sidebarState !== 2 && !QbzShell.navHeaderCompact
    readonly property bool headerCompactOn: QbzShell.sidebarState === 2
        || (!QbzShell.navInSidebar && QbzShell.navHeaderCompact)

    // Full text tab (HeaderBar.slint NavTab): 30px tall, radius sm, icon 14
    // (dropped under 1140px), label 11px — semibold + elevated fill when the
    // section is the current view.
    component NavTab: Rectangle {
        id: navTab
        property var section: null
        property bool showIcon: true
        readonly property bool isActive: section
            && (root.activeNav === section.id || navFlyout.openId === section.id)
        readonly property bool isEnabled: section && section.enabled

        height: 30
        width: tabRow.implicitWidth
        radius: theme.radiusSm
        opacity: isEnabled ? 1.0 : 0.5
        color: isActive ? theme.surfaceElevated
            : (tabArea.containsMouse && isEnabled) ? theme.surfaceHover : "transparent"

        Row {
            id: tabRow
            height: parent.height
            leftPadding: navTab.showIcon ? 9 : 11
            rightPadding: 11
            spacing: 6
            // Baked glyph, or the section's own raw image when it carries one
            // (My QBZ branding) — see shell/NavSectionGlyph.qml.
            NavSectionGlyph {
                visible: navTab.showIcon
                section: navTab.section
                size: 14
                // Collapses to zero width under the 1140px breakpoint so the
                // Row reclaims the space (the height stays 14 via `size`).
                width: navTab.showIcon ? 14 : 0
                anchors.verticalCenter: parent.verticalCenter
                // Both arms are THEME tokens (the tab sits on surface-elevated
                // / surface-hover): "textPrimary", never the fixed-white
                // "primary" bake, or the active tab is white-on-white on every
                // light theme. See the vocabulary in QbzIcon.qml.
                tintName: navTab.isActive ? "textPrimary" : "secondary"
            }
            Text {
                height: parent.height
                text: navTab.section ? navTab.section.label : ""
                color: theme.textPrimary
                font.pixelSize: 11
                font.weight: navTab.isActive ? theme.weightSemibold : theme.weightRegular
                verticalAlignment: Text.AlignVCenter
            }
        }
        MouseArea {
            id: tabArea
            anchors.fill: parent
            enabled: navTab.isEnabled
            hoverEnabled: navTab.isEnabled
            cursorShape: Qt.PointingHandCursor
            // Full text tabs name their section already, so the dropdown gets
            // no title row (HeaderBar.slint:167 sets title: "").
            onClicked: navFlyout.openUnder(navTab, navTab.section, false)
            onContainsMouseChanged: {
                if (containsMouse) {
                    // Hover-to-open, and hovering a DIFFERENT tab overwrites
                    // the open menu — instant switch, single-open by design.
                    navFlyout.triggerHovered = true
                    navFlyout.openUnder(navTab, navTab.section, false)
                } else if (navTab.section && navFlyout.openId === navTab.section.id) {
                    // Only the trigger owning the open menu clears the flag
                    // (avoids a leave/enter race between adjacent tabs).
                    navFlyout.triggerHovered = false
                }
            }
        }
    }

    // Compact icon-only button (HeaderBar.slint CompactNavBtn): 30x30, icon
    // 16, elevated fill + primary tint while its section is current.
    component CompactNavBtn: Rectangle {
        id: cnb
        property var section: null
        readonly property bool isActive: section
            && (root.activeNav === section.id || navFlyout.openId === section.id)
        readonly property bool isEnabled: section && section.enabled

        width: 30
        height: 30
        radius: theme.radiusSm
        opacity: isEnabled ? 1.0 : 0.5
        color: isActive ? theme.surfaceElevated
            : (cnbArea.containsMouse && isEnabled) ? theme.surfaceHover : "transparent"
        // Baked glyph, or the section's own raw image when it carries one
        // (My QBZ branding) — see shell/NavSectionGlyph.qml.
        NavSectionGlyph {
            section: cnb.section
            size: 16
            anchors.centerIn: parent
            tintName: cnb.isActive ? "textPrimary" : "secondary"
        }
        MouseArea {
            id: cnbArea
            anchors.fill: parent
            enabled: cnb.isEnabled
            hoverEnabled: cnb.isEnabled
            cursorShape: Qt.PointingHandCursor
            // Icon-only buttons do NOT name their section, so their dropdown
            // is headed by the section name + hairline (HeaderBar.slint:223).
            onClicked: navFlyout.openUnder(cnb, cnb.section, true)
            onContainsMouseChanged: {
                if (containsMouse) {
                    navFlyout.triggerHovered = true
                    navFlyout.openUnder(cnb, cnb.section, true)
                } else if (cnb.section && navFlyout.openId === cnb.section.id) {
                    navFlyout.triggerHovered = false
                }
            }
        }
    }

    // --- Left controls ---------------------------------------------------
    Row {
        id: leftControls
        x: theme.spacingMd
        y: (root.height - height) / 2
        height: 36
        spacing: 6

        QbzNavButton {
            name: "panel-left"
            anchors.verticalCenter: parent.verticalCenter
            onClicked: QbzShell.cycleSidebar()
        }
        QbzNavButton {
            name: "chevron-left"
            anchors.verticalCenter: parent.verticalCenter
            btnEnabled: QbzShell.canBack
            onClicked: QbzShell.navigateBack()
        }
        QbzNavButton {
            name: "chevron-right"
            anchors.verticalCenter: parent.verticalCenter
            btnEnabled: QbzShell.canForward
            onClicked: QbzShell.navigateForward()
        }

        // Full section nav (text tabs) — nav in the header, sidebar not fully
        // closed, compact form OFF. Sits AFTER the three sacred buttons so it
        // can never overlap them (HeaderBar.slint:853).
        Row {
            visible: root.headerTabsOn
            height: parent.height
            spacing: 2

            Item { width: 6; height: 1 }
            Repeater {
                model: root.navSections
                delegate: NavTab {
                    required property var modelData
                    section: modelData
                    // Qobuz-only sections are HIDDEN while offline (ADR-010
                    // mount-site gating, not rendered-disabled).
                    visible: !(modelData.qobuz && QbzSession.offline)
                    // Tabs drop their icons under the first breakpoint.
                    showIcon: root.width >= 1140
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }

        // Compact section nav — while the sidebar is fully closed (so the
        // sections stay reachable), or always when the nav is in the header
        // and "Compact header navigation" is ON (HeaderBar.slint:974).
        Row {
            visible: root.headerCompactOn
            height: parent.height
            spacing: 2

            Item { width: 6; height: 1 }
            Repeater {
                model: root.navSections
                delegate: CompactNavBtn {
                    required property var modelData
                    section: modelData
                    visible: !(modelData.qobuz && QbzSession.offline)
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
            // Thin separator + the playlists flyout button — ONLY while the
            // sidebar is really closed. In the opt-in always-compact mode the
            // sidebar list is still on screen, so the button stays away
            // (HeaderBar.slint:1078).
            Rectangle {
                visible: QbzShell.sidebarState === 2
                width: 1
                height: 18
                anchors.verticalCenter: parent.verticalCenter
                color: theme.borderSubtle
            }
            Rectangle {
                id: plBtn
                visible: QbzShell.sidebarState === 2
                width: 30
                height: 30
                radius: theme.radiusSm
                anchors.verticalCenter: parent.verticalCenter
                color: plPopup.opened ? theme.surfaceElevated
                    : plBtnArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "list-music"
                    width: 16
                    height: 16
                    anchors.centerIn: parent
                    tintName: plPopup.opened ? "textPrimary" : "secondary"
                }
                MouseArea {
                    id: plBtnArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: plPopup.open()
                }

                // Closed-sidebar playlists flyout (SidebarPlaylistsPopup):
                // 248px wide, a search header over the SAME Rust-side filter
                // the sidebar uses, then the flat entries (folders toggle in
                // place, playlists open and close the flyout). Six rows
                // visible, the rest scroll, then Slint's two footer rows —
                // Import and Manage playlists.
                //
                // Those two call the IDENTICAL invokables the sidebar "..."
                // menu's rows do, and both surfaces close themselves and clear
                // the SHARED search query before acting (this popup's onClosed
                // already does the clearing). Wiring only one of them would
                // re-create the gap this comment used to describe: the flyout
                // is the only playlists surface a user with a closed sidebar
                // has, so an entry point missing here is missing outright.
                Popup {
                    id: plPopup
                    y: plBtn.height + 6
                    width: 248
                    padding: 4
                    closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape

                    readonly property var entries: JSON.parse(QbzShell.sidebarJson)

                    onClosed: {
                        // Clear the shared filter so it doesn't silently scope
                        // the expanded sidebar's list later.
                        plSearch.text = ""
                        QbzShell.sidebarSearch("")
                    }

                    background: Rectangle {
                        color: theme.surfaceMain
                        radius: theme.radiusSm
                        border.width: 1
                        border.color: theme.borderMuted
                    }
                    contentItem: Column {
                        width: parent.width
                        spacing: 0

                        // Search header (34px band, 26px field).
                        Item {
                            width: parent.width
                            height: 34
                            Rectangle {
                                anchors.centerIn: parent
                                width: parent.width - 16
                                height: 26
                                radius: 4
                                color: theme.surfaceElevated
                                border.width: 1
                                border.color: theme.borderSubtle
                                TextInput {
                                    id: plSearch
                                    anchors.fill: parent
                                    anchors.leftMargin: 6
                                    anchors.rightMargin: 6
                                    color: theme.textPrimary
                                    font.pixelSize: 12
                                    verticalAlignment: Text.AlignVCenter
                                    clip: true
                                    onTextEdited: QbzShell.sidebarSearch(text)
                                    Text {
                                        visible: plSearch.text === ""
                                        anchors.fill: parent
                                        text: QbzSession.tr("Search playlists", QbzSession.trRev)
                                        color: theme.textMuted
                                        font.pixelSize: 12
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }
                            }
                        }

                        // Entries — 32px rows, six visible then scroll.
                        Flickable {
                            width: parent.width
                            height: Math.max(32, Math.min(plPopup.entries.length, 6) * 32)
                            clip: true
                            contentWidth: width
                            contentHeight: plList.implicitHeight
                            boundsBehavior: Flickable.StopAtBounds

                            Column {
                                id: plList
                                width: parent.width
                                Repeater {
                                    model: plPopup.entries
                                    delegate: Rectangle {
                                        required property var modelData
                                        readonly property bool isFolder: modelData.kind === "folder"
                                        width: plList.width
                                        height: 32
                                        radius: 5
                                        color: ppArea.containsMouse ? theme.surfaceHover : "transparent"
                                        Row {
                                            anchors.fill: parent
                                            anchors.leftMargin: modelData.indent ? 26 : 12
                                            anchors.rightMargin: 12
                                            spacing: 8
                                            QbzIcon {
                                                name: isFolder
                                                    ? (modelData.expanded ? "folder-open" : "folder")
                                                    : "list-music"
                                                width: 14
                                                height: 14
                                                anchors.verticalCenter: parent.verticalCenter
                                                tintName: isFolder ? "accent" : "muted"
                                            }
                                            Text {
                                                width: parent.width - 22 - (ppCount.visible ? ppCount.implicitWidth + 8 : 0)
                                                height: parent.height
                                                text: modelData.name
                                                color: isFolder ? theme.textPrimary : theme.textSecondary
                                                font.pixelSize: 13
                                                font.weight: isFolder ? theme.weightSemibold : theme.weightRegular
                                                verticalAlignment: Text.AlignVCenter
                                                elide: Text.ElideRight
                                            }
                                            Text {
                                                id: ppCount
                                                visible: isFolder && modelData.count > 0
                                                height: parent.height
                                                text: modelData.count
                                                color: theme.textMuted
                                                font.pixelSize: 11
                                                verticalAlignment: Text.AlignVCenter
                                            }
                                        }
                                        MouseArea {
                                            id: ppArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                if (isFolder) {
                                                    QbzShell.sidebarToggleFolder(modelData.id)
                                                } else {
                                                    QbzBridge.openPlaylist(modelData.id)
                                                    plPopup.close()
                                                }
                                            }
                                        }
                                    }
                                }
                                // Empty state — the flyout would otherwise be a
                                // blank 32px band with no explanation.
                                Text {
                                    visible: plPopup.entries.length === 0
                                    width: plList.width
                                    height: 32
                                    leftPadding: 12
                                    text: QbzSession.tr("No playlists yet.", QbzSession.trRev)
                                    color: theme.textMuted
                                    font.pixelSize: 12
                                    verticalAlignment: Text.AlignVCenter
                                }
                            }
                        }

                        // ---- Footer (SidebarPlaylistsPopup.slint:189-269) ---
                        Rectangle {
                            width: parent.width
                            height: 1
                            color: theme.borderSubtle
                        }
                        // Import — offline-gated exactly like the sidebar's
                        // "..." row. The importer modal carries its own offline
                        // banner as the second layer; the row alone is not the
                        // gate.
                        Rectangle {
                            id: plImportRow
                            readonly property bool rowEnabled: !QbzSession.offline
                            width: parent.width
                            height: 40
                            radius: 5
                            color: (plImportRow.rowEnabled && plImportArea.containsMouse)
                                ? theme.surfaceHover : "transparent"
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 12
                                anchors.rightMargin: 12
                                spacing: 8
                                QbzIcon {
                                    name: "import"
                                    width: 15
                                    height: 15
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: plImportRow.rowEnabled ? "secondary" : "muted"
                                }
                                Text {
                                    width: parent.width - 23
                                    height: parent.height
                                    text: QbzSession.tr("Import", QbzSession.trRev)
                                    color: plImportRow.rowEnabled ? theme.textSecondary : theme.textMuted
                                    font.pixelSize: 13
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                            }
                            MouseArea {
                                id: plImportArea
                                anchors.fill: parent
                                enabled: plImportRow.rowEnabled
                                hoverEnabled: plImportRow.rowEnabled
                                cursorShape: plImportRow.rowEnabled
                                    ? Qt.PointingHandCursor : Qt.ArrowCursor
                                onClicked: {
                                    QbzPlaylistImport.open()
                                    // Closing clears the shared search query
                                    // (plPopup.onClosed) so it does not silently
                                    // scope the expanded sidebar's list later.
                                    plPopup.close()
                                }
                            }
                        }
                        // Manage playlists — always available; the manager is a
                        // local-store surface and the only place a hidden
                        // playlist can be un-hidden.
                        Rectangle {
                            id: plManageRow
                            width: parent.width
                            height: 40
                            radius: 5
                            color: plManageArea.containsMouse ? theme.surfaceHover : "transparent"
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 12
                                anchors.rightMargin: 12
                                spacing: 8
                                QbzIcon {
                                    name: "library-big"
                                    width: 15
                                    height: 15
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: "secondary"
                                }
                                Text {
                                    width: parent.width - 23
                                    height: parent.height
                                    text: QbzSession.tr("Manage playlists", QbzSession.trRev)
                                    color: theme.textSecondary
                                    font.pixelSize: 13
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                            }
                            MouseArea {
                                id: plManageArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    QbzPlaylistManager.navigate()
                                    plPopup.close()
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // --- Search (absolutely centered; LIVE — phase 15) ---------------------
    // The Slint HeaderBar search-scope: typing drives the cortinilla (220ms
    // debounce, >= 2 chars), arrows move the keyboard selection, Enter runs
    // the row / full search, Esc dismisses. The × clears + closes.
    function clearSearch() {
        searchInput.text = ""
        QbzSearch.cortinillaDismiss()
    }

    // nav.search / Ctrl+f (2026-08-03 hotkeys-port contract §4.3, divergence
    // K6): QbzHotkeys emits focusSearchRequested, the AppShell Connections
    // calls this. EXCEEDS Slint, whose focus_search only flipped
    // cortinilla_open — with an empty query the only visible effect was the
    // "↵ Enter" hint (the panel is gated on ≥2 chars; the "the field grabs
    // focus on open" comment at keybindings.rs:533-536 is stale). Here the
    // field lands focused and ready to type, which is what Ctrl+f means in
    // every other app.
    function focusSearch() {
        searchInput.forceActiveFocus()
    }

    Rectangle {
        id: searchBox
        x: (root.width - width) / 2
        y: (root.height - height) / 2
        // 80% of the prior search width; gives up 60px to the section nav
        // whenever that nav lives in the header (HeaderBar.slint:569). The
        // width animates and `x` re-centers with it.
        width: (root.width < 960 ? 179 : 256) - (QbzShell.navInSidebar ? 0 : 60)
        height: 32
        Behavior on width {
            NumberAnimation { duration: 220; easing.type: Easing.InOutQuad }
        }
        radius: 6
        border.width: 1
        border.color: searchInput.activeFocus ? theme.accent : theme.borderSubtle
        color: theme.surfaceElevated

        QbzIcon {
            name: "search"
            width: 14
            height: 14
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            tintName: "muted"
        }
        TextInput {
            id: searchInput
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: 30
            anchors.rightMargin: 8
            height: parent.height
            color: theme.textPrimary
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: text === "" && !activeFocus ? Text.AlignHCenter : Text.AlignLeft
            clip: true
            // No QML debounce: the 220ms CORTINILLA_DEBOUNCE lives in Rust
            // now (search_qt::live). Debouncing the whole invokable here
            // meant the panel and its skeleton only appeared 220ms after the
            // LAST keystroke, so continuous typing showed nothing at all;
            // the reference opens on the FIRST keystroke >= 2 chars and
            // debounces only the LOAD. Rust's version guard is what discards
            // the superseded loads.
            onTextEdited: {
                if (text.trim().length < 2) {
                    QbzSearch.cortinillaDismiss()
                } else {
                    QbzSearch.searchLive(text)
                }
            }

            // The Enter rule (HeaderBar.slint on-enter): cortinilla open +
            // a keyboard selection -> activate the row; open + none -> full
            // search; closed -> plain submit (also Search > All).
            Keys.onPressed: function (event) {
                if (event.key === Qt.Key_Down) {
                    if (QbzSearch.cortinillaOpen) {
                        QbzSearch.cortinillaMoveSelection(1)
                        event.accepted = true
                    }
                } else if (event.key === Qt.Key_Up) {
                    if (QbzSearch.cortinillaOpen) {
                        QbzSearch.cortinillaMoveSelection(-1)
                        event.accepted = true
                    }
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    if (QbzSearch.cortinillaOpen) {
                        if (QbzSearch.cortinillaSelectedIndex >= 0) {
                            root.clearSearch()
                            QbzSearch.cortinillaRowClicked(QbzSearch.cortinillaSelectedIndex)
                        } else {
                            root.clearSearch()
                            QbzSearch.cortinillaSearchAll()
                        }
                    } else {
                        // Capture BEFORE clearing (clearSearch wipes the input).
                        var q = searchInput.text
                        root.clearSearch()
                        QbzSearch.searchSubmit(q)
                    }
                    event.accepted = true
                } else if (event.key === Qt.Key_Escape) {
                    if (QbzSearch.cortinillaOpen) {
                        QbzSearch.cortinillaDismiss()
                        event.accepted = true
                    }
                }
            }
        }
        // Placeholder — centered on the WHOLE box, exactly like
        // HeaderBar.slint:792 (`width: parent.width; horizontal-alignment:
        // center`). The magnifier is absolutely positioned at x:10 and takes
        // no layout space, so it must not shift the placeholder either: the
        // 30px left inset this used to carry pushed "Search" 15px right of
        // the field's centre, which is what reads as "not centred".
        Text {
            visible: searchInput.text === "" && !searchInput.activeFocus
            anchors.fill: parent
            text: QbzSession.tr("Search", QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: 13
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
        }
        // Right-edge affordances: the Enter hint while the cortinilla is
        // open (Slint: it lives in the box, opposite the magnifier), else
        // the × clear.
        Text {
            visible: QbzSearch.cortinillaOpen
            anchors.right: parent.right
            anchors.rightMargin: 10
            height: parent.height
            text: "⏎ " + QbzSession.tr("Enter", QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: 12
            verticalAlignment: Text.AlignVCenter
        }
        Rectangle {
            visible: !QbzSearch.cortinillaOpen && searchInput.text !== ""
            anchors.right: parent.right
            anchors.rightMargin: 5
            width: 22
            height: 22
            anchors.verticalCenter: parent.verticalCenter
            radius: 11
            color: clearArea.containsMouse ? theme.surfaceHover : "transparent"
            QbzIcon {
                name: "x"
                width: 12
                height: 12
                anchors.centerIn: parent
                tintName: clearArea.containsMouse ? "textPrimary" : "muted"
            }
            MouseArea {
                id: clearArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.clearSearch()
            }
        }
    }

    // --- Right controls: status badge + app menu --------------------------
    Row {
        id: rightControls
        // Shifts left of the drawn window controls (3x34 + 2x2 = 106px).
        x: root.width - width - theme.spacingMd + 2 - (QbzShell.systemTitleBar ? 0 : 110)
        y: (root.height - height) / 2
        height: 36
        spacing: 4

        // Offline status badge (OfflineStatusBadge) — ghost chrome like
        // NavTab: transparent, hover -> surface-hover, radius sm.
        Rectangle {
            visible: root.badgeState !== 0
            height: 30
            width: badgeRow.implicitWidth
            anchors.verticalCenter: parent.verticalCenter
            radius: theme.radiusSm
            color: badgeArea.containsMouse ? theme.surfaceHover : "transparent"

            readonly property string stateTintName: root.badgeState === 1 ? "warning"
                : root.badgeState === 2 ? "accent" : "muted"

            Row {
                id: badgeRow
                height: parent.height
                leftPadding: 9
                rightPadding: 11
                spacing: 6
                QbzIcon {
                    name: root.badgeState === 3 ? "user" : "cloud-off"
                    width: 14
                    height: 14
                    anchors.verticalCenter: parent.verticalCenter
                    tintName: parent.parent.stateTintName
                }
                Text {
                    height: parent.height
                    text: root.badgeState === 3 ? QbzSession.tr("Logged out", QbzSession.trRev)
                        : root.badgeState === 2 ? QbzSession.tr("Offline (manual)", QbzSession.trRev)
                        : QbzSession.tr("Offline (hard)", QbzSession.trRev)
                    color: theme.textSecondary
                    font.pixelSize: 11
                    verticalAlignment: Text.AlignVCenter
                }
            }
            MouseArea {
                id: badgeArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: badgeFlyout.open()
            }

            // Status flyout — the former banner texts + actions.
            Popup {
                id: badgeFlyout
                x: parent.width - 320
                y: parent.height + 6
                width: 320
                padding: 14
                closePolicy: Popup.CloseOnPressOutside

                background: Rectangle {
                    color: theme.surfaceMain
                    radius: theme.radiusSm
                    border.width: 1
                    border.color: theme.borderMuted
                }
                contentItem: Column {
                    spacing: 12
                    Row {
                        spacing: 8
                        width: parent.width
                        QbzIcon {
                            name: root.badgeState === 3 ? "user" : "cloud-off"
                            width: 15
                            height: 15
                            tintName: "muted"
                        }
                        Text {
                            width: parent.width - 23
                            text: root.badgeState === 3
                                ? (QbzSession.connectivity === 2
                                    ? QbzSession.tr("You're signed out. Sign-in needs a connection.", QbzSession.trRev)
                                    : QbzSession.connectivity === 1
                                        ? QbzSession.tr("Connection available — sign back in to Qobuz.", QbzSession.trRev)
                                        : QbzSession.tr("You're signed out — sign back in to Qobuz.", QbzSession.trRev))
                                : root.badgeState === 2
                                    ? QbzSession.tr("Offline mode is enabled. Disable it in Settings to use Qobuz.", QbzSession.trRev)
                                    : QbzSession.tr("No internet connection — your local library and downloads keep working.", QbzSession.trRev)
                            color: theme.textPrimary
                            font.pixelSize: 12
                            wrapMode: Text.WordWrap
                        }
                    }
                    // Sign in — logged-out state only; disabled only when the
                    // link is CONFIRMED down.
                    Item {
                        visible: root.badgeState === 3
                        width: parent.width
                        height: visible ? 32 : 0
                        Rectangle {
                            anchors.right: parent.right
                            width: signInText.implicitWidth + 28
                            height: 32
                            radius: theme.radiusSm
                            border.width: 1
                            border.color: theme.borderSubtle
                            opacity: QbzSession.connectivity === 2 ? 0.4 : 1.0
                            color: signInArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                            Text {
                                id: signInText
                                anchors.centerIn: parent
                                text: QbzSession.tr("Sign in", QbzSession.trRev)
                                color: theme.textSecondary
                                font.pixelSize: 13
                            }
                            MouseArea {
                                id: signInArea
                                anchors.fill: parent
                                enabled: QbzSession.connectivity !== 2
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    QbzSession.recoveryLogin()
                                    badgeFlyout.close()
                                }
                            }
                        }
                    }
                    // POC-NOTE: the manual-offline quick toggle (state 2)
                    // needs OfflineModeActions.set-offline — not wired; the
                    // status text above still renders.
                }
            }
        }

        QbzIconButton { activeBackground: true 
            name: "menu"
            anchors.verticalCenter: parent.verticalCenter
            onClicked: appMenu.open()
        }
    }

    // --- Drawn window controls (WindowControls.slint, right placement:
    // minimize · maximize · close; close gets the danger-red hover) ------
    Row {
        visible: !QbzShell.systemTitleBar
        x: root.width - width - 8
        y: (root.height - height) / 2
        height: 26
        spacing: 2
        Rectangle {
            width: 34
            height: 26
            radius: theme.radiusSm
            color: wcMinArea.containsMouse ? theme.surfaceHover : "transparent"
            QbzIcon {
                name: "minus"
                width: 14
                height: 14
                anchors.centerIn: parent
                tintName: "secondary"
            }
            MouseArea {
                id: wcMinArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: if (root.hostWindow) root.hostWindow.showMinimized()
            }
        }
        Rectangle {
            width: 34
            height: 26
            radius: theme.radiusSm
            color: wcMaxArea.containsMouse ? theme.surfaceHover : "transparent"
            QbzIcon {
                name: root.hostWindow && root.hostWindow.visibility === Window.Maximized
                    ? "minimize-2" : "maximize-2"
                width: 14
                height: 14
                anchors.centerIn: parent
                tintName: "secondary"
            }
            MouseArea {
                id: wcMaxArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: {
                    if (root.hostWindow) {
                        root.hostWindow.visibility = root.hostWindow.visibility === Window.Maximized
                            ? Window.Windowed : Window.Maximized
                    }
                }
            }
        }
        Rectangle {
            width: 34
            height: 26
            radius: theme.radiusSm
            color: wcCloseArea.containsMouse ? "#e81123" : "transparent"
            QbzIcon {
                name: "x"
                width: 14
                height: 14
                anchors.centerIn: parent
                // The hover fill is a FIXED #e81123 under every theme, so the
                // hovered glyph is literal white ("white"), not a theme
                // token — the one place in this file where the fixed bake is
                // the right answer.
                tintName: wcCloseArea.containsMouse ? "white" : "secondary"
            }
            MouseArea {
                id: wcCloseArea
                anchors.fill: parent
                hoverEnabled: true
                // POC-NOTE: the Slint app hides-to-tray on close; the POC
                // has no tray — close quits.
                onClicked: Qt.quit()
            }
        }
    }

    // --- App menu (user block + Settings + Log Out + Close) ---------------
    // POC-NOTE: the Slint menu also carries Open Music Link / Keyboard
    // Shortcuts / Documentation / Report an Issue / What's New / About QBZ
    // — omitted here (no views/dialogs to open yet).
    Popup {
        id: appMenu
        x: root.width - 234 - theme.spacingMd
        y: theme.headerHeight - 4
        width: 234
        padding: 0
        closePolicy: Popup.CloseOnPressOutside

        background: Rectangle {
            color: theme.surfaceMain
            radius: theme.radiusSm
            border.width: 1
            border.color: theme.borderMuted
        }
        contentItem: Column {
            width: parent.width
            topPadding: 6
            bottomPadding: 6

            // Signed-in user — name and subscription tier.
            Column {
                width: parent.width
                leftPadding: 14
                rightPadding: 14
                topPadding: 6
                bottomPadding: 10
                spacing: 2
                Text {
                    text: QbzSession.sessionUserName === ""
                        ? QbzSession.tr("Guest", QbzSession.trRev) : QbzSession.sessionUserName
                    color: theme.textPrimary
                    font.pixelSize: theme.fontBody
                    font.weight: theme.weightSemibold
                }
                Text {
                    text: QbzSession.sessionSubscription === ""
                        ? QbzSession.tr("Not signed in", QbzSession.trRev) : QbzSession.sessionSubscription
                    color: theme.textMuted
                    font.pixelSize: 12
                }
            }
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
            Item { width: 1; height: 4 }

            // One menu row (MenuItem).
            component AppMenuItem: Rectangle {
                property string name: ""
                property string label: ""
                property bool checkedItem: false
                signal clicked()

                width: parent ? parent.width : 0
                height: 34
                color: miArea.containsMouse ? theme.surfaceHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 18
                    spacing: 10
                    QbzIcon {
                        name: parent.parent.name
                        width: 15
                        height: 15
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "secondary"
                    }
                    Text {
                        id: miLabel
                        height: parent.height
                        text: parent.parent.label
                        color: theme.textSecondary
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                    }
                    Item {
                        visible: parent.parent.checkedItem
                        width: visible ? parent.width - 15 - miLabel.implicitWidth - 14 - 2 * parent.spacing : 0
                        height: 1
                    }
                    QbzIcon {
                        visible: parent.parent.checkedItem
                        name: "check"
                        width: 14
                        height: 14
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "accent"
                    }
                }
                MouseArea {
                    id: miArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: parent.clicked()
                }
            }

            // The app menu carries ACTIONS. Intelligent search, Ambient
            // background and Use system title bar were settings living here by
            // accident; all three already have their real rows in
            // Settings > Appearance, which is where Slint keeps them.
            AppMenuItem {
                name: "settings-2"
                label: QbzSession.tr("Settings", QbzSession.trRev)
                onClicked: {
                    appMenu.close()
                    QbzShell.navigateTo("settings")
                }
            }
            // Slint order (HeaderBar.slint:1175-1212): Settings, Open Music
            // Link, Keyboard Shortcuts, Documentation… The Open Music Link
            // row stays ABSENT (the LinkResolver is not ported, K3).
            AppMenuItem {
                name: "keyboard"
                label: QbzSession.tr("Keyboard Shortcuts", QbzSession.trRev)
                onClicked: {
                    appMenu.close()
                    QbzHotkeys.openCheatsheet()
                }
            }
            AppMenuItem {
                name: "book-open"
                label: QbzSession.tr("Documentation", QbzSession.trRev)
                onClicked: {
                    appMenu.close()
                    QbzShell.openExternalUrl("https://github.com/vicrodh/qbz/wiki")
                }
            }
            AppMenuItem {
                name: "log-out"
                label: QbzSession.tr("Log Out", QbzSession.trRev)
                onClicked: {
                    appMenu.close()
                    QbzSession.logout()
                }
            }
            AppMenuItem {
                name: "x"
                label: QbzSession.tr("Close", QbzSession.trRev)
                // POC-NOTE: the Slint app hides to tray / follows the
                // platform close behavior; the POC just quits.
                onClicked: Qt.quit()
            }
        }
    }
}
