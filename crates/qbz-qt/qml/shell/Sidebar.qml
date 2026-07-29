// Left navigation sidebar — QML port of crates/qbz-ui/ui/shell/Sidebar.slint.
//
// Three states (ShellState.sidebar-state): 0 = open 240px (icon + label),
// 1 = mini 64px (icons only), 2 = closed 0px. Width animates 160ms
// ease-in-out; the header's panel-left button cycles (QbzShell.cycleSidebar).
//
// Top-level section nav rows (Discover / Library / Local Library / My QBZ)
// replicate SidebarNavRow: 34px rows, radius 6, 16px icons, 13px/w500
// labels, surface-hover on hover, Discover + Library HIDDEN while offline
// (ADR-010 mount-site gating). The whole block — rows AND the hairline under
// them — is mounted only while `QbzShell.navInSidebar` is ON; with it OFF the
// sections move to the header (HeaderBar.qml) and this sidebar starts at the
// PLAYLISTS toolbar, exactly like Sidebar.slint:724/817.
//
// POC-NOTE: in the Slint app these rows open dropdown flyout menus; the
// flyouts are out of scope — rows here navigate straight to their view and
// carry the SidebarDirectRow active treatment (surface-hover bg + primary
// text/icon) for the current section. Discover / Library / Local Library
// have views; My QBZ does NOT and is therefore rendered disabled (a row
// that clicks into nothing is a defect, not a stub).
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
    // Which section owns the highlight. DERIVED from the live view, not set on
    // click: the same rows can live here or in the header, history navigation
    // (back/forward) must keep the highlight honest, and a click-set local
    // went stale on both counts. Slint highlights its rows off
    // HeaderMenuState.open-index — a flyout this port does not have — so the
    // current view is the faithful stand-in (it is what SidebarDirectRow uses).
    readonly property string activeNav:
          QbzShell.currentView === "home" ? "discover"
        : QbzShell.currentView === "library" ? "library"
        : (QbzShell.currentView === "local" || QbzShell.currentView === "localalbum") ? "local"
        : ""
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

    // One section-nav row (SidebarNavRow / SidebarDirectRow metrics).
    component NavRow: Rectangle {
        id: navRow
        property string navId: ""
        property string name: ""
        property string label: ""
        property bool active: root.activeNav === navId
        // No view behind this section in this port — render it as
        // unavailable rather than as a live row that clicks into nothing.
        property bool disabled: false
        signal clicked()

        width: parent ? parent.width : 0
        height: 34
        radius: 6
        opacity: disabled ? 0.45 : 1.0
        color: (!disabled && (navArea.containsMouse || active)) ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: root.mini ? 0 : 8
            anchors.rightMargin: root.mini ? 0 : 8
            spacing: 10

            Item {
                width: root.mini ? parent.width : 16
                height: parent.height
                QbzIcon {
                    name: navRow.name
                    width: 16
                    height: 16
                    anchors.centerIn: parent
                    tintName: (navArea.containsMouse || navRow.active)
                          ? "primary" : "secondary"
                }
            }
            Text {
                visible: !root.mini
                height: parent.height
                width: parent.width - (root.mini ? 0 : 26)
                text: navRow.label
                color: (navArea.containsMouse || navRow.active)
                       ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                font.weight: theme.weightMedium
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: navArea
            anchors.fill: parent
            enabled: !navRow.disabled
            hoverEnabled: !navRow.disabled
            cursorShape: Qt.PointingHandCursor
            onClicked: navRow.clicked()
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

            // Qobuz-only sections — HIDDEN entirely while offline (ADR-010).
            NavRow {
                navId: "discover"
                name: "compass"
                label: QbzSession.tr("Discover", QbzSession.trRev)
                visible: !QbzSession.offline
                onClicked: QbzShell.navigateTo("home")
            }
            NavRow {
                navId: "library"
                name: "music-library-2"
                label: QbzSession.tr("Library", QbzSession.trRev)
                visible: !QbzSession.offline
                onClicked: QbzShell.navigateTo("library")
            }
            NavRow {
                navId: "local"
                name: "hard-drive"
                label: QbzSession.tr("Local Library", QbzSession.trRev)
                onClicked: QbzShell.navigateTo("local")
            }
            // GAP: there is no MyQBZ view in this port (AppShell's loader
            // maps home/library/local/localalbum/album/artist/settings/
            // search/playlist — no "myqbz"). It used to take the active
            // treatment on click and go nowhere; it now renders disabled.
            NavRow {
                navId: "myqbz"
                name: "qbz-symbolic"
                // Slint: MyQbzBrandingState.label, default "My QBZ".
                label: QbzSession.tr("My QBZ", QbzSession.trRev)
                disabled: true
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
                    tintName: (searchBtnArea.containsMouse || root.searchOpen) ? "primary" : "muted"
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
                    tintName: plusArea.containsMouse ? "primary" : "muted"
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
                    tintName: sortBtnArea.containsMouse ? "primary" : "muted"
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
                    tintName: collapseArea.containsMouse ? "primary" : "muted"
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
                            height: root.mini && modelData.indent ? 0 : 32
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

                                // Leading icon slot (24px fixed).
                                Item {
                                    width: 24
                                    height: parent.height
                                    // Folder icon (accent).
                                    QbzIcon {
                                        visible: isFolder
                                        name: modelData.expanded && !root.mini ? "folder-open" : "folder"
                                        width: 15
                                        height: 15
                                        anchors.centerIn: parent
                                        anchors.horizontalCenterOffset: root.mini ? 0 : -4.5
                                        tintName: "accent"
                                    }
                                    // 2x2 micro-collage (or list-music glyph).
                                    Rectangle {
                                        visible: useCollage
                                        width: 20
                                        height: 20
                                        anchors.centerIn: parent
                                        anchors.horizontalCenterOffset: root.mini ? 0 : -2
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
                                        anchors.centerIn: parent
                                        anchors.horizontalCenterOffset: root.mini ? 0 : -4.5
                                        tintName: (rowArea.containsMouse || isActive) ? "primary" : "muted"
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
                                    tintName: rowArea.containsMouse ? "primary" : "muted"
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
                                onClicked: {
                                    if (isFolder) {
                                        QbzShell.sidebarToggleFolder(modelData.id)
                                    } else {
                                        root.activePlaylistId = modelData.id
                                        QbzBridge.openPlaylist(modelData.id)
                                    }
                                }
                            }
                        }
                    }

                    // Empty state.
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
        }
    }
}
