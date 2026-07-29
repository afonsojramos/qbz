// Left navigation sidebar — QML port of crates/qbz-ui/ui/shell/Sidebar.slint.
//
// Three states (ShellState.sidebar-state): 0 = open 240px (icon + label),
// 1 = mini (icons only), 2 = closed 0px. Width animates 160ms ease-in-out;
// the header's panel-left button cycles (QbzShell.cycleSidebar).
//
// MINI RAIL: the rail is 50px here, not Slint's 64 — a deliberate,
// one-token owner deviation (theme.sidebarMiniWidth) so the square 34px row
// exactly fills the 8px-padded track. Everything else in the mini state is
// 1:1 with Slint: rows centre their lone glyph, nested playlist rows
// collapse to zero, hovering a row names it in a bubble to the right
// (SidebarTooltip.slint) and clicking a FOLDER opens its playlists in a
// flyout to the right (SidebarFolderPopup.slint) — in mini a folder cannot
// expand in place, so a toggle there would be a control that does nothing.
//
// FILE LENGTH (>500): the two mini affordances above are shared single
// instances declared at this root (never one Popup per row), which is what
// keeps them cheap; they are extraction candidates into
// qml/shell/SidebarFolderFlyout.qml, but a new .qml also needs a build.rs
// entry and an unregistered type fails the WHOLE file at load, so they stay
// here until that glue lands.
//
// Top-level section nav rows (Discover / Library / Local Library / My QBZ)
// replicate SidebarNavRow: 34px rows, radius 6, 16px icons, 13px/w500
// labels, surface-hover on hover, Discover + Library HIDDEN while offline
// (ADR-010 mount-site gating). The whole block — rows AND the hairline under
// them — is mounted only while `QbzShell.navInSidebar` is ON; with it OFF the
// sections move to the header (HeaderBar.qml) and this sidebar starts at the
// PLAYLISTS toolbar, exactly like Sidebar.slint:724/817.
//
// The rows open the same dropdown FLYOUT their Slint counterparts do
// (SidebarNavRow: hover or click, panel to the RIGHT of the row, headed by
// the section name) — the panel, its behaviour and the entry catalog live in
// the shared NavFlyout.qml, which HeaderBar.qml mounts too, so the sections
// behave identically wherever the nav is hosted. The rows keep the
// SidebarDirectRow active treatment (surface-hover bg + primary text/icon)
// for the current section and while their own menu is open.
// Discover / Library / Local Library route into real views; My QBZ does NOT
// (no such surface in this port) and is therefore rendered disabled, flyout
// included (a row that clicks into nothing is a defect, not a stub).
// The playlist/folder tree below the nav IS live (sidebar_qt.rs: load,
// sort, search, expand/collapse, drag-drop target); folder CREATION,
// the Playlist Manager and the importer have no seam and are disabled in
// the "..." menu.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    property bool mini: QbzShell.sidebarState === 1
    // Which section owns the highlight. DERIVED from the live view (in
    // NavFlyout, shared with the header), not set on click: the same rows can
    // live here or in the header, and history navigation (back/forward) must
    // keep the highlight honest. Slint additionally highlights the row whose
    // menu is open — the rows OR `navFlyout.openId` in for that.
    readonly property string activeNav: navFlyout.activeSection
    // Playlist tree state (phase 7).
    property bool searchOpen: false
    property bool playlistsCollapsed: false
    property string activePlaylistId: ""
    // ---- Large-NPB dock space reservation -------------------------------
    // The cover+spectrum dock is an AppShell-ROOT overlay pinned flush to the
    // WINDOW bottom-left, so it extends the bar's height BELOW this sidebar
    // (which stops at npb.top). Only the IN-SIDEBAR portion has to be
    // reserved, and it shrinks when the band is hidden — that is why the
    // height comes from the bridge and not a literal.
    //
    // Without this the playlist list keeps running to the sidebar's bottom
    // edge and the rows sit UNDER the album art (Sidebar.slint:1145 reserves
    // the same way).
    readonly property bool largeDockActive: QbzShell.npbMode === 3 && QbzShell.sidebarState === 0
    readonly property real dockReserve: largeDockActive
        ? Math.max(0, QbzShell.largeDockHeight - theme.npbLargeHeight)
        : 0

    // Flattened entries from the bridge + the url-keyed cover map.
    readonly property var entries: parseEntries(QbzShell.sidebarJson)
    property var coverMap: ({})
    function parseEntries(json) {
        var e = JSON.parse(json)
        // Collect every cover url and dispatch the artwork window.
        var urls = []
        for (var i = 0; i < e.length; i++) {
            for (var j = 0; j < e[i].covers.length; j++) urls.push(e[i].covers[j])
        }
        if (urls.length > 0) QbzShell.sidebarArtworkWindow(JSON.stringify(urls))
        return e
    }

    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
    }

    QbzTheme { id: theme }

    // The section-nav dropdown (catalog + panel + open/close behaviour) —
    // the SAME component HeaderBar.qml mounts. Zero-size and outside the
    // Column so its origin stays this root's origin (the row coordinates are
    // mapped into it). The panel renders in the window overlay, so neither
    // this root's `clip` nor its 240px width can cut it off.
    NavFlyout { id: navFlyout }

    // ---- Collapsed (mini) affordances -----------------------------------
    // In the mini rail a playlist row is an icon and nothing else, so the two
    // things the open sidebar gives for free have to be restored:
    //   - the NAME, as a hover bubble to the row's right (SidebarTooltip.slint);
    //   - a folder's CONTENTS, as a flyout to the row's right, because the
    //     nested rows are hidden in mini (SidebarFolderPopup.slint).
    // Both are ONE shared instance driven by the hovered/clicked row (exactly
    // like Slint's SidebarTooltipState / SidebarFolderPopupState), never one
    // Popup per delegate: the tree can hold hundreds of rows.
    // Both are QtQuick.Controls popups, so their content is reparented to the
    // WINDOW overlay and this root's `clip: true` cannot cut them — the same
    // reason NavFlyout and HeaderBar's playlists flyout are Popups.

    // Race-safe show/hide (Slint SidebarRow.show-tooltip / hide-tooltip): the
    // leaving row only clears the bubble if it still owns it, so sliding the
    // pointer straight from one row to the next never blanks it.
    function showMiniTip(row, entry) {
        if (!root.mini) return
        var p = row.mapToItem(root, row.width + 6, 0)
        miniTip.anchorX = p.x
        miniTip.anchorY = p.y
        miniTip.rowH = row.height
        miniTip.tipText = entry.kind === "folder"
            ? entry.name + " (" + entry.count + ")"
            : entry.name
        miniTip.ownerId = entry.id
    }
    function hideMiniTip(id) {
        if (miniTip.ownerId === id) miniTip.ownerId = ""
    }

    // Mini folder click: the flyout, not a toggle (the nested rows are hidden,
    // so toggling is a control that renders and does nothing — Slint opens the
    // popup instead, Sidebar.slint:355-377).
    function openFolderFlyout(row, entry) {
        folderFlyout.folderId = entry.id
        folderFlyout.folderName = entry.name
        folderFlyout.folderCount = entry.count
        // The children are only present in the published entries while the
        // folder is EXPANDED. Expand it if it is not — never toggle blindly:
        // a folder left open before the sidebar collapsed would be CLOSED by a
        // toggle and the flyout would list nothing (and while a search query is
        // active every folder is force-expanded, sidebar_qt.rs:299).
        if (!entry.expanded) QbzShell.sidebarToggleFolder(entry.id)
        var p = row.mapToItem(root, row.width + 6, 0)
        folderFlyout.x = p.x
        folderFlyout.y = p.y
        folderFlyout.open()
    }

    // Cycling the sidebar while either is open would leave it floating over a
    // rail that no longer exists (the bubble self-hides through `root.mini`;
    // the flyout needs telling).
    Connections {
        target: QbzShell
        function onSidebarStateChanged() {
            folderFlyout.close()
            miniTip.ownerId = ""
        }
    }

    // Hover bubble — SidebarTooltip.slint: surface-elevated, radius sm, 1px
    // border-muted, 10/10/5/5 padding, 12px w500 text-primary, VISUAL ONLY
    // (a ToolTip never takes the pointer, so it cannot block the row it
    // describes).
    ToolTip {
        id: miniTip
        property string ownerId: ""
        property string tipText: ""
        property real anchorX: 0
        property real anchorY: 0
        property real rowH: 32

        // Auto-hides the moment the sidebar leaves mini (SidebarTooltip
        // .slint:14) or the folder flyout takes over the same +6px slot
        // (Slint drops it explicitly on click, Sidebar.slint:353).
        visible: root.mini && ownerId !== "" && !folderFlyout.visible
        delay: 0
        timeout: -1
        padding: 0
        x: anchorX
        y: anchorY + Math.round((rowH - implicitHeight) / 2)
        contentItem: Text {
            text: miniTip.tipText
            color: theme.textPrimary
            font.pixelSize: 12
            font.weight: theme.weightMedium
            verticalAlignment: Text.AlignVCenter
            leftPadding: 10
            rightPadding: 10
            topPadding: 5
            bottomPadding: 5
        }
        background: Rectangle {
            color: theme.surfaceElevated
            radius: theme.radiusSm
            border.width: 1
            border.color: theme.borderMuted
        }
    }

    // Folder flyout — SidebarFolderPopup.slint: 230px, 30px header row +
    // hairline + up to four 32px rows, 4px inner padding, surface-main /
    // radius sm / 1px border-muted.
    Popup {
        id: folderFlyout
        width: 230
        padding: 0
        topPadding: 4
        bottomPadding: 4
        // Slint's literal panel height (34 + 1 + list + 8) — 4px taller than
        // the content, kept 1:1 rather than tightened.
        height: 34 + 1 + folderFlyout.visibleRows * 32 + 8
        // Replaces Slint's full-window scrim TouchArea (SidebarFolderPopup
        // .slint:56-60); the Popup's own background swallows clicks inside,
        // so no hand-built scrim and no click-through.
        closePolicy: Popup.CloseOnPressOutside | Popup.CloseOnEscape

        property string folderId: ""
        property string folderName: ""
        property int folderCount: 0

        // The rows are a BINDING over the published entries, never a snapshot
        // read inside the click handler: sidebarToggleFolder republishes
        // sidebarJson through the cxx-qt UI queue (shell_bridge.rs::ui ->
        // CxxQtThread::queue), so the children land a LATER event-loop turn.
        // Reading `root.entries` at click time would open an empty panel.
        readonly property var rows: root.entries.filter(function (e) {
            return e.kind !== "folder" && e.folderId === folderFlyout.folderId
        })
        // Height off the folder's own count (known at click time) so the panel
        // does not resize when the children land a tick later.
        readonly property int visibleRows:
            Math.min(Math.max(folderFlyout.folderCount, folderFlyout.rows.length), 4)

        background: Rectangle {
            color: theme.surfaceMain
            radius: theme.radiusSm
            border.width: 1
            border.color: theme.borderMuted
        }
        contentItem: Column {
            spacing: 0

            // Header — folder name (30px, 12px side padding, 14px accent
            // folder-open glyph, 13px/600 text-primary).
            Item {
                width: parent.width
                height: 30
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 12
                    anchors.rightMargin: 12
                    spacing: 8
                    QbzIcon {
                        name: "folder-open"
                        width: 14
                        height: 14
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "accent"
                    }
                    Text {
                        width: parent.width - 22
                        height: parent.height
                        text: folderFlyout.folderName
                        color: theme.textPrimary
                        font.pixelSize: 13
                        font.weight: theme.weightSemibold
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
            }
            Rectangle {
                width: parent.width
                height: 1
                color: theme.borderSubtle
            }

            // Playlist list — four rows visible, the rest scroll (the shared
            // QbzScrollBar mounts past four, like Slint's ListScrollbar).
            Item {
                width: parent.width
                height: folderFlyout.visibleRows * 32

                Flickable {
                    id: fpFlick
                    anchors.fill: parent
                    clip: true
                    contentWidth: width
                    contentHeight: fpColumn.implicitHeight
                    boundsBehavior: Flickable.StopAtBounds

                    Column {
                        id: fpColumn
                        width: parent.width
                        Repeater {
                            model: folderFlyout.rows
                            delegate: Rectangle {
                                id: fpRow
                                required property var modelData
                                width: fpColumn.width
                                height: 32
                                radius: 5
                                color: fpArea.containsMouse ? theme.surfaceHover : "transparent"
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 12
                                    anchors.rightMargin: 12
                                    spacing: 8
                                    QbzIcon {
                                        name: "list-music"
                                        width: 14
                                        height: 14
                                        anchors.verticalCenter: parent.verticalCenter
                                        tintName: "muted"
                                    }
                                    Text {
                                        width: parent.width - 22
                                        height: parent.height
                                        text: fpRow.modelData.name
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        verticalAlignment: Text.AlignVCenter
                                        elide: Text.ElideRight
                                    }
                                }
                                MouseArea {
                                    id: fpArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        root.activePlaylistId = fpRow.modelData.id
                                        QbzBridge.openPlaylist(fpRow.modelData.id)
                                        folderFlyout.close()
                                    }
                                }
                            }
                        }
                    }
                }
                QbzScrollBar {
                    visible: folderFlyout.folderCount > 4 && fpFlick.contentHeight > fpFlick.height
                    target: fpFlick
                    width: 10
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                }
            }
        }
    }

    width: QbzShell.sidebarState === 2 ? 0
         : QbzShell.sidebarState === 1 ? theme.sidebarMiniWidth
         : theme.sidebarOpenWidth
    // surface-card @ 0.5 while the ambient background is active (phase 14).
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
       
    // Square edges; clip keeps content from spilling while the width
    // animates (same as the Slint root).
    clip: true

    Behavior on width {
        NumberAnimation { duration: 160; easing.type: Easing.InOutQuad }
    }

    // One section-nav row (SidebarNavRow / SidebarDirectRow metrics). Hovering
    // or clicking it opens that section's flyout to the RIGHT of the row.
    component NavRow: Rectangle {
        id: navRow
        // A section from the shared catalog (NavFlyout.sections).
        property var section: null
        readonly property bool isActive: section
            && (root.activeNav === section.id || navFlyout.openId === section.id)
        // No surface behind this section in this port — rendered as
        // unavailable rather than as a live row that clicks into nothing.
        readonly property bool isEnabled: section && section.enabled
        readonly property bool lit: isActive || (navArea.containsMouse && isEnabled)

        width: parent ? parent.width : 0
        // Mini: a SQUARE row that fills the 34px padded track (theme token,
        // rail 50 - 2*8). Open keeps Slint's 34px (Sidebar.slint:575).
        height: root.mini ? theme.sidebarMiniRow : 34
        radius: 6
        opacity: isEnabled ? 1.0 : 0.45
        color: lit ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: root.mini ? 0 : 8
            anchors.rightMargin: root.mini ? 0 : 8
            spacing: 10

            Item {
                width: root.mini ? parent.width : 16
                height: parent.height
                QbzIcon {
                    name: navRow.section ? navRow.section.icon : ""
                    width: 16
                    height: 16
                    anchors.centerIn: parent
                    tintName: navRow.lit ? "textPrimary" : "secondary"
                }
            }
            Text {
                visible: !root.mini
                height: parent.height
                width: parent.width - (root.mini ? 0 : 26)
                text: navRow.section ? navRow.section.label : ""
                color: navRow.lit ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: navArea
            anchors.fill: parent
            enabled: navRow.isEnabled
            hoverEnabled: navRow.isEnabled
            cursorShape: Qt.PointingHandCursor
            onClicked: navFlyout.openBeside(navRow, navRow.section)
            onContainsMouseChanged: {
                if (containsMouse) {
                    // Hover-to-open; hovering another row overwrites the open
                    // menu (instant switch, single-open by construction).
                    navFlyout.triggerHovered = true
                    navFlyout.openBeside(navRow, navRow.section)
                } else if (navRow.section && navFlyout.openId === navRow.section.id) {
                    // Only the row owning the open menu clears the flag.
                    navFlyout.triggerHovered = false
                }
            }
        }
    }

    Column {
        anchors.fill: parent
        anchors.leftMargin: root.mini ? 8 : theme.spacingMd
        anchors.rightMargin: root.mini ? 8 : theme.spacingMd
        anchors.topMargin: root.mini ? 8 : theme.spacingMd
        // Per-side padding so Large can zero ONLY the bottom (Sidebar.slint:719):
        // the reserved dock band then reaches the true window bottom-left and the
        // cover sits flush (the L corner). Left/right/top are unchanged.
        anchors.bottomMargin: root.largeDockActive ? 0 : (root.mini ? 8 : theme.spacingMd)
        spacing: theme.spacingMd

        // ---- Section nav -------------------------------------------
        Column {
            id: navColumn
            // Section-nav placement: OFF moves the whole block to the header.
            // A Column skips invisible children entirely (no phantom spacing),
            // so this is the `if` mount-site gate Slint uses.
            visible: QbzShell.navInSidebar
            width: parent.width
            spacing: 2

            // The same four sections the header hosts, from the same catalog.
            // Qobuz-only ones are HIDDEN entirely while offline (ADR-010
            // mount-site gating, not rendered-disabled); My QBZ carries
            // enabled: false in the catalog (no surface behind it in this
            // port) and so renders dimmed and inert, flyout included.
            Repeater {
                model: navFlyout.sections
                delegate: NavRow {
                    required property var modelData
                    section: modelData
                    visible: !(modelData.qobuz && QbzSession.offline)
                }
            }
        }

        // Hairline under the section nav — mounted with the nav block, not
        // on its own (Sidebar.slint:817 gates it on the same flag).
        Rectangle {
            visible: QbzShell.navInSidebar
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // ---- Playlists header toolbar (hidden in the mini state) -----
        Row {
            visible: !root.mini
            width: parent.width
            height: 22
            spacing: 4

            // Title (hidden while the search input is open).
            Text {
                visible: !root.searchOpen
                width: parent.width - 4 * 26
                height: parent.height
                text: QbzSession.tr("PLAYLISTS", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: 10
                font.letterSpacing: 1
                verticalAlignment: Text.AlignVCenter
            }
            // Inline search input (filters entries, recursive).
            Rectangle {
                visible: root.searchOpen
                width: parent.width - 4 * 26
                height: 22
                radius: 4
                color: theme.surfaceElevated
                border.width: 1
                border.color: theme.borderSubtle
                TextInput {
                    id: searchEdit
                    anchors.fill: parent
                    anchors.leftMargin: 6
                    color: theme.textPrimary
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                    clip: true
                    onTextEdited: QbzShell.sidebarSearch(text)
                    Text {
                        visible: searchEdit.text === ""
                        anchors.fill: parent
                        text: QbzSession.tr("Search playlists", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 12
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }

            // Search toggle.
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: (searchBtnArea.containsMouse || root.searchOpen) ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "search"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: (searchBtnArea.containsMouse || root.searchOpen) ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: searchBtnArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.searchOpen = !root.searchOpen
                        if (!root.searchOpen) {
                            searchEdit.text = ""
                            QbzShell.sidebarSearch("")
                        } else {
                            searchEdit.forceActiveFocus()
                        }
                    }
                }
            }
            // New playlist (+) — single core call (create_playlist), then
            // reload the tree. Slint opens a naming modal; the POC creates
            // with the default name (POC-NOTE).
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: plusArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "plus"
                    width: 16
                    height: 16
                    anchors.centerIn: parent
                    tintName: plusArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: plusArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzShell.createPlaylist()
                }
            }
            // Sort / more (...) menu.
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: sortBtnArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: "ellipsis"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: sortBtnArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: sortBtnArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: sortMenu.open()
                }
                Popup {
                    id: sortMenu
                    // Opens to the RIGHT of the "..." (overlay into the
                    // content area) — owner request in Sidebar.slint.
                    x: parent.width + 18
                    y: 26
                    width: 200
                    padding: 6
                    closePolicy: Popup.CloseOnPressOutside
                    background: Rectangle {
                        color: theme.surfaceCard
                        radius: 8
                        border.width: 1
                        border.color: theme.borderSubtle
                    }
                    contentItem: Column {
                        spacing: 1
                        Row {
                            leftPadding: 8
                            topPadding: 4
                            bottomPadding: 2
                            spacing: 6
                            QbzIcon { name: "arrow-up-down"; width: 13; height: 13; tintName: "muted" }
                            Text {
                                text: QbzSession.tr("Sort by", QbzSession.trRev)
                                color: theme.textMuted
                                font.pixelSize: 11
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                        Repeater {
                            model: [
                                { "opt": "name", "label": QbzSession.tr("Name (A-Z)", QbzSession.trRev) },
                                { "opt": "recent", "label": QbzSession.tr("Recent", QbzSession.trRev) },
                                { "opt": "tracks", "label": QbzSession.tr("# of tracks", QbzSession.trRev) },
                                { "opt": "playcount", "label": QbzSession.tr("Play Count", QbzSession.trRev) },
                                { "opt": "custom", "label": QbzSession.tr("Custom", QbzSession.trRev) },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                width: parent ? parent.width : 0
                                height: 30
                                radius: 5
                                color: sortOptArea.containsMouse ? theme.surfaceHover : "transparent"
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    anchors.rightMargin: 8
                                    spacing: 6
                                    Text {
                                        width: parent.width - 20
                                        height: parent.height
                                        text: modelData.label
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        font.weight: QbzShell.sidebarSortBy === modelData.opt ? theme.weightSemibold : theme.weightRegular
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                    QbzIcon {
                                        visible: QbzShell.sidebarSortBy === modelData.opt
                                        name: QbzShell.sidebarSortAsc ? "chevron-up" : "chevron-down"
                                        width: 13
                                        height: 13
                                        anchors.verticalCenter: parent.verticalCenter
                                        tintName: "accent"
                                    }
                                }
                                MouseArea {
                                    id: sortOptArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: {
                                        QbzShell.sidebarSetSort(modelData.opt)
                                        sortMenu.close()
                                    }
                                }
                            }
                        }
                        Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
                        // New folder / Manage playlists / Import — DISABLED.
                        // GAP, not a half-path: this port has no folder
                        // creation seam (`sidebar_qt.rs` exposes only
                        // load/sort/search/toggle-folder — no create/delete),
                        // no Playlist Manager view, and no importer. They used
                        // to render as live-looking rows (hover highlight +
                        // pointing-hand cursor) whose click did nothing at
                        // all; a control that renders and no-ops is a defect,
                        // so they now read as unavailable: dimmed, no hover
                        // treatment, no pointer cursor, no MouseArea. Restore
                        // the interactive form together with the seam.
                        Repeater {
                            model: [
                                { "icon": "folder-plus", "label": QbzSession.tr("New folder", QbzSession.trRev) },
                                { "icon": "library-big", "label": QbzSession.tr("Manage playlists", QbzSession.trRev) },
                                { "icon": "import", "label": QbzSession.tr("Import", QbzSession.trRev) },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                width: parent ? parent.width : 0
                                height: 30
                                radius: 5
                                color: "transparent"
                                opacity: 0.4
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    spacing: 8
                                    QbzIcon { name: modelData.icon; width: 14; height: 14; anchors.verticalCenter: parent.verticalCenter; tintName: "muted" }
                                    Text {
                                        height: parent.height
                                        text: modelData.label
                                        color: theme.textMuted
                                        font.pixelSize: 13
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Collapse chevron (down = expanded, right = collapsed).
            Rectangle {
                width: 22
                height: 22
                radius: 4
                color: collapseArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: root.playlistsCollapsed ? "chevron-right" : "chevron-down"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: collapseArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: collapseArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.playlistsCollapsed = !root.playlistsCollapsed
                }
            }
        }

        // ---- Playlist tree (folders + playlists, sidebar_qt.rs) --------
        Item {
            visible: !root.playlistsCollapsed
            width: parent.width
            // This is the element that FILLS (Sidebar.slint gives the playlist
            // Rectangle `vertical-stretch: 1`): everything above it — the nav
            // block, the hairline, the 22px toolbar — takes its natural height
            // and the list eats the rest, down to the Column's bottom margin.
            // `root.dockReserve` keeps the rows clear of the Large-NPB cover
            // (0 in every other mode). Clamped so a short window shrinks the
            // list to nothing instead of giving it a negative height.
            //
            // There used to be a trailing `Item { height: 0 }` "filler" after
            // this one and a matching `- 16` here to pay for the Column spacing
            // it introduced. Net effect: the list stopped 16px short and the
            // sidebar read as too short against Slint. Both are gone — the
            // list is the last child, so its bottom IS the Column's bottom.
            height: Math.max(0, parent.height - y - root.dockReserve)

            Flickable {
                id: plFlick
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: plColumn.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                Column {
                    id: plColumn
                    width: parent.width
                    spacing: 2

                    Repeater {
                        model: root.entries
                        delegate: Rectangle {
                            id: plRow
                            required property var modelData
                            property bool isFolder: modelData.kind === "folder"
                            // Slint SidebarRow.use-collage: opt-OUT setting AND
                            // at least one cover; otherwise the list-music glyph.
                            property bool useCollage: !isFolder
                                && QbzShell.sidebarPlaylistCollage
                                && modelData.covers.length > 0
                            property bool isActive: !isFolder && modelData.id === root.activePlaylistId
                            // Track-drag drop target (Sidebar.slint SidebarRow):
                            // the row self-detects the drop from the shared
                            // window-coord pointer and claims/releases it.
                            property bool dropHot: false
                            function recomputeDrop() {
                                if (isFolder || !QbzShell.dragActive) {
                                    if (dropHot) { QbzShell.dragSetOver(""); dropHot = false }
                                    return
                                }
                                const tl = mapToItem(null, 0, 0)
                                const hot = QbzShell.dragX >= tl.x && QbzShell.dragX <= tl.x + width
                                    && QbzShell.dragY >= tl.y && QbzShell.dragY <= tl.y + height
                                if (hot && !dropHot) QbzShell.dragSetOver(modelData.id)
                                else if (!hot && dropHot) QbzShell.dragSetOver("")
                                dropHot = hot
                            }
                            Connections {
                                target: QbzShell
                                function onDragXChanged() { recomputeDrop() }
                                function onDragYChanged() { recomputeDrop() }
                                function onDragActiveChanged() { recomputeDrop() }
                            }
                            width: plColumn.width
                            // Mini: nested rows collapse to nothing (their
                            // folder opens the flyout instead, Sidebar.slint:
                            // 213-215), top-level rows become the SAME square
                            // the nav rows are — a 34px row in a 34px track.
                            // Slint keeps 32px here; the 2px come from the
                            // narrower rail (theme.sidebarMiniRow).
                            height: root.mini
                                ? (modelData.indent ? 0 : theme.sidebarMiniRow)
                                : 32
                            visible: !(root.mini && modelData.indent)
                            radius: 6
                            // success-bg + success-border while hot (Theme
                            // success family, #3fae6a at 10% / 30%).
                            color: dropHot ? "#3fae6a1a"
                                : ((rowArea.containsMouse || isActive) ? theme.surfaceHover : "transparent")
                            border.width: dropHot ? 1 : 0
                            border.color: "#3fae6a4d"

                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: root.mini ? 0 : (modelData.indent ? 24 : 8)
                                anchors.rightMargin: root.mini ? 0 : 6
                                spacing: 8

                                // Leading icon slot: 24px fixed while open (so
                                // every row's name starts at the same x), the
                                // WHOLE row in mini so the lone glyph lands on
                                // the rail's centre line — Slint gets this from
                                // `alignment: mini ? center : stretch`
                                // (Sidebar.slint:230), which a QML Row has no
                                // equivalent for. Without it the slot is the
                                // only visible child, sits at x=0, and the
                                // playlist/folder glyphs render 12px LEFT of
                                // the nav glyphs above them (owner screenshot).
                                // Same idiom as NavRow's slot above.
                                Item {
                                    width: root.mini ? parent.width : 24
                                    height: parent.height
                                    // Folder icon (accent). Every glyph in this
                                    // slot is placed with rounded arithmetic
                                    // (Sidebar.slint:245-246): `centerIn` of an
                                    // odd 15px glyph in an even track lands on a
                                    // half pixel and blurs it.
                                    QbzIcon {
                                        visible: isFolder
                                        name: modelData.expanded && !root.mini ? "folder-open" : "folder"
                                        width: 15
                                        height: 15
                                        x: Math.round((parent.width - width) / 2)
                                        y: Math.round((parent.height - height) / 2)
                                        tintName: "accent"
                                    }
                                    // 2x2 micro-collage (or list-music glyph).
                                    Rectangle {
                                        visible: useCollage
                                        width: 20
                                        height: 20
                                        x: Math.round((parent.width - width) / 2)
                                        y: Math.round((parent.height - height) / 2)
                                        radius: 3
                                        color: theme.surfaceElevated
                                        clip: true
                                        Grid {
                                            anchors.fill: parent
                                            columns: 2
                                            Repeater {
                                                model: modelData.covers.slice(0, 4)
                                                delegate: Image {
                                                    required property string modelData
                                                    width: 10
                                                    height: 10
                                                    source: root.coverMap[modelData] || ""
                                                    fillMode: Image.PreserveAspectCrop
                                                    asynchronous: true
                                                }
                                            }
                                        }
                                    }
                                    QbzIcon {
                                        visible: !isFolder && !useCollage
                                        name: "list-music"
                                        width: 15
                                        height: 15
                                        x: Math.round((parent.width - width) / 2)
                                        y: Math.round((parent.height - height) / 2)
                                        tintName: (rowArea.containsMouse || isActive) ? "textPrimary" : "muted"
                                    }
                                }
                                // Name (hidden in mini). A Row is a positioner
                                // with no stretch, so this width must reserve
                                // EVERY later cell + its spacing or the excess
                                // is pushed past the row's right edge and the
                                // `clip: true` on the Flickable eats it.
                                // (Sidebar.slint gets this free: its
                                // HorizontalLayout stretches the name cell.)
                                // Reserve: 8 + 24 icon slot always; for a
                                // FOLDER also 8 + 14 chevron, plus 8 + the
                                // count label while it is laid out. The old
                                // flat -22 ignored the count, pushing the
                                // chevron ~15px past the clip — which is why
                                // the caret vanished on exactly the folders
                                // that HAVE children (owner report).
                                Text {
                                    visible: !root.mini
                                    width: Math.max(0, parent.width - 32
                                        - (isFolder
                                            ? 22 + (countLabel.visible
                                                ? countLabel.implicitWidth + 8 : 0)
                                            : 0))
                                    height: parent.height
                                    text: modelData.name
                                    color: (rowArea.containsMouse || isActive) ? theme.textPrimary : theme.textSecondary
                                    font.pixelSize: 13
                                    font.weight: isFolder ? theme.weightSemibold : theme.weightRegular
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                                // Folder count (hover) + chevron. The count is
                                // hover-FADED, not hidden — opacity does not
                                // free layout space, so it must stay in the
                                // name's reserve above at all times.
                                Text {
                                    id: countLabel
                                    visible: !root.mini && isFolder && modelData.count > 0
                                    height: parent.height
                                    text: modelData.count
                                    color: theme.textMuted
                                    font.pixelSize: 11
                                    verticalAlignment: Text.AlignVCenter
                                    opacity: rowArea.containsMouse ? 1.0 : 0.0
                                }
                                QbzIcon {
                                    visible: !root.mini && isFolder
                                    name: modelData.expanded ? "chevron-down" : "chevron-right"
                                    width: 14
                                    height: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: rowArea.containsMouse ? "textPrimary" : "muted"
                                }
                            }
                            // Active-row redundant shape cue: 3px accent bar.
                            Rectangle {
                                visible: isActive
                                x: 0
                                width: 3
                                height: parent.height - 12
                                anchors.verticalCenter: parent.verticalCenter
                                radius: 1.5
                                color: theme.accent
                            }
                            MouseArea {
                                id: rowArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                // Mini: the row is an icon, so hovering names
                                // it (Sidebar.slint:342-349). No-op while open.
                                onContainsMouseChanged: {
                                    if (containsMouse) root.showMiniTip(plRow, plRow.modelData)
                                    else root.hideMiniTip(plRow.modelData.id)
                                }
                                onClicked: {
                                    // Slint drops the bubble on click so it
                                    // does not linger behind the flyout.
                                    root.hideMiniTip(plRow.modelData.id)
                                    if (isFolder) {
                                        // Mini: the nested rows are hidden, so
                                        // a toggle would change NOTHING on
                                        // screen — the contents open in a
                                        // flyout to the right instead
                                        // (Sidebar.slint:355-377).
                                        if (root.mini) root.openFolderFlyout(plRow, plRow.modelData)
                                        else QbzShell.sidebarToggleFolder(modelData.id)
                                    } else {
                                        root.activePlaylistId = modelData.id
                                        QbzBridge.openPlaylist(modelData.id)
                                    }
                                }
                            }
                        }
                    }

                    // Empty state. NOTE: Slint does NOT gate this on mini
                    // (Sidebar.slint:1077-1082) and word-wraps the sentence
                    // inside the 48px track; keeping the gate here is a
                    // deliberate, owner-visible divergence.
                    Text {
                        visible: !root.mini && root.entries.length === 0 && !QbzSession.offline
                        width: plColumn.width
                        text: QbzSession.tr("No playlists yet.", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 12
                        wrapMode: Text.WordWrap
                    }
                }
            }

            // List scrollbar (Sidebar.slint:1090-1137). The shared
            // QbzScrollBar — the port's ListScrollbar replica — rather than a
            // third scrollbar implementation. Two accepted deviations from
            // the sidebar's bespoke Slint one: it reveals on SCROLL (900ms
            // auto-hide) instead of on row-hover, and its thumb is the
            // standard 8/10px instead of the sidebar's thinner 3-6px.
            // Placed in the sidebar's right PADDING (negative margin), like
            // Slint's `x: parent.width - width + (mini ? 8 : Spacing.md)`, so
            // the gutter never overlays the rows and cannot eat their clicks.
            QbzScrollBar {
                target: plFlick
                width: root.mini ? 8 : 10
                anchors.right: parent.right
                anchors.rightMargin: -(root.mini ? 8 : theme.spacingMd)
                anchors.top: parent.top
                anchors.bottom: parent.bottom
            }
        }
    }
}
