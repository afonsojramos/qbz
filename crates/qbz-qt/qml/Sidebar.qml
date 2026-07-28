// Left navigation sidebar — QML port of crates/qbz-ui/ui/shell/Sidebar.slint.
//
// Three states (ShellState.sidebar-state): 0 = open 240px (icon + label),
// 1 = mini 64px (icons only), 2 = closed 0px. Width animates 160ms
// ease-in-out; the header's panel-left button cycles (QbzBridge.cycleSidebar).
//
// Top-level section nav rows (Discover / Library / Local Library / My QBZ)
// replicate SidebarNavRow: 34px rows, radius 6, 16px icons, 13px/w500
// labels, surface-hover on hover, Discover + Library HIDDEN while offline
// (ADR-010 mount-site gating).
//
// POC-NOTE: in the Slint app these rows open dropdown flyout menus; the
// flyouts are out of scope — rows here navigate (only "home" exists, so
// only Discover is live) and carry the SidebarDirectRow active treatment
// (surface-hover bg + primary text/icon) for the current section.
// POC-NOTE: the playlist/folder tree below the nav is out of scope; the
// "PLAYLISTS" header + toolbar render for parity and the Slint empty state
// shows ("No playlists yet.").

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

Rectangle {
    id: root

    property bool mini: QbzBridge.sidebarState === 1
    // The current section — Discover (home) is the only real view.
    property string activeNav: "discover"
    // Playlist tree state (phase 7).
    property bool searchOpen: false
    property bool playlistsCollapsed: false
    property string activePlaylistId: ""
    // Flattened entries from the bridge + the url-keyed cover map.
    readonly property var entries: parseEntries(QbzBridge.sidebarJson)
    property var coverMap: ({})
    function parseEntries(json) {
        var e = JSON.parse(json)
        // Collect every cover url and dispatch the artwork window.
        var urls = []
        for (var i = 0; i < e.length; i++) {
            for (var j = 0; j < e[i].covers.length; j++) urls.push(e[i].covers[j])
        }
        if (urls.length > 0) QbzBridge.sidebarArtworkWindow(JSON.stringify(urls))
        return e
    }

    Connections {
        target: QbzBridge
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
    }

    QbzTheme { id: theme }

    width: QbzBridge.sidebarState === 2 ? 0
         : QbzBridge.sidebarState === 1 ? theme.sidebarMiniWidth
         : theme.sidebarOpenWidth
    // surface-card @ 0.5 while the ambient background is active (phase 14).
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzBridge.ambientMode > 0 && QbzBridge.npHasTrack
       
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
        signal clicked()

        width: parent ? parent.width : 0
        height: 34
        radius: 6
        color: (navArea.containsMouse || active) ? theme.surfaceHover : "transparent"

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
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: navRow.clicked()
        }
    }

    Column {
        anchors.fill: parent
        anchors.leftMargin: root.mini ? 8 : theme.spacingMd
        anchors.rightMargin: root.mini ? 8 : theme.spacingMd
        anchors.topMargin: root.mini ? 8 : theme.spacingMd
        anchors.bottomMargin: root.mini ? 8 : theme.spacingMd
        spacing: theme.spacingMd

        // ---- Section nav -------------------------------------------
        Column {
            id: navColumn
            width: parent.width
            spacing: 2

            // Qobuz-only sections — HIDDEN entirely while offline (ADR-010).
            NavRow {
                navId: "discover"
                name: "compass"
                label: QbzBridge.tr("Discover", QbzBridge.trRev)
                visible: !QbzBridge.offline
                onClicked: {
                    root.activeNav = "discover"
                    QbzBridge.navigateTo("home")
                }
            }
            NavRow {
                navId: "library"
                name: "music-library-2"
                label: QbzBridge.tr("Library", QbzBridge.trRev)
                visible: !QbzBridge.offline
                onClicked: {
                    root.activeNav = "library"
                    QbzBridge.navigateTo("library")
                }
            }
            NavRow {
                navId: "local"
                name: "hard-drive"
                label: QbzBridge.tr("Local Library", QbzBridge.trRev)
                onClicked: root.activeNav = "local"
            }
            NavRow {
                navId: "myqbz"
                name: "qbz-symbolic"
                // Slint: MyQbzBrandingState.label, default "My QBZ".
                label: QbzBridge.tr("My QBZ", QbzBridge.trRev)
                onClicked: root.activeNav = "myqbz"
            }
        }

        Rectangle {
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
                text: QbzBridge.tr("PLAYLISTS", QbzBridge.trRev)
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
                    onTextEdited: QbzBridge.sidebarSearch(text)
                    Text {
                        visible: searchEdit.text === ""
                        anchors.fill: parent
                        text: QbzBridge.tr("Search playlists", QbzBridge.trRev)
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
                            QbzBridge.sidebarSearch("")
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
                    onClicked: QbzBridge.createPlaylist()
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
                                text: QbzBridge.tr("Sort by", QbzBridge.trRev)
                                color: theme.textMuted
                                font.pixelSize: 11
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                        Repeater {
                            model: [
                                { "opt": "name", "label": QbzBridge.tr("Name (A-Z)", QbzBridge.trRev) },
                                { "opt": "recent", "label": QbzBridge.tr("Recent", QbzBridge.trRev) },
                                { "opt": "tracks", "label": QbzBridge.tr("# of tracks", QbzBridge.trRev) },
                                { "opt": "playcount", "label": QbzBridge.tr("Play Count", QbzBridge.trRev) },
                                { "opt": "custom", "label": QbzBridge.tr("Custom", QbzBridge.trRev) },
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
                                        font.weight: QbzBridge.sidebarSortBy === modelData.opt ? theme.weightSemibold : theme.weightRegular
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                    QbzIcon {
                                        visible: QbzBridge.sidebarSortBy === modelData.opt
                                        name: QbzBridge.sidebarSortAsc ? "chevron-up" : "chevron-down"
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
                                        QbzBridge.sidebarSetSort(modelData.opt)
                                        sortMenu.close()
                                    }
                                }
                            }
                        }
                        Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
                        // New folder / Manage / Import — visual stubs
                        // (POC-NOTE: folder management + importer are out
                        // of scope).
                        Repeater {
                            model: [
                                { "icon": "folder-plus", "label": QbzBridge.tr("New folder", QbzBridge.trRev) },
                                { "icon": "library-big", "label": QbzBridge.tr("Manage playlists", QbzBridge.trRev) },
                                { "icon": "import", "label": QbzBridge.tr("Import", QbzBridge.trRev) },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                width: parent ? parent.width : 0
                                height: 30
                                radius: 5
                                color: stubArea.containsMouse ? theme.surfaceHover : "transparent"
                                Row {
                                    anchors.fill: parent
                                    anchors.leftMargin: 8
                                    spacing: 8
                                    QbzIcon { name: modelData.icon; width: 14; height: 14; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                                    Text {
                                        height: parent.height
                                        text: modelData.label
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }
                                MouseArea {
                                    id: stubArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
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
            height: parent.height - y - 16

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
                            property bool isActive: !isFolder && modelData.id === root.activePlaylistId
                            // Track-drag drop target (Sidebar.slint SidebarRow):
                            // the row self-detects the drop from the shared
                            // window-coord pointer and claims/releases it.
                            property bool dropHot: false
                            function recomputeDrop() {
                                if (isFolder || !QbzBridge.dragActive) {
                                    if (dropHot) { QbzBridge.dragSetOver(""); dropHot = false }
                                    return
                                }
                                const tl = mapToItem(null, 0, 0)
                                const hot = QbzBridge.dragX >= tl.x && QbzBridge.dragX <= tl.x + width
                                    && QbzBridge.dragY >= tl.y && QbzBridge.dragY <= tl.y + height
                                if (hot && !dropHot) QbzBridge.dragSetOver(modelData.id)
                                else if (!hot && dropHot) QbzBridge.dragSetOver("")
                                dropHot = hot
                            }
                            Connections {
                                target: QbzBridge
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
                                        visible: !isFolder && modelData.covers.length > 0
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
                                        visible: !isFolder && modelData.covers.length === 0
                                        name: "list-music"
                                        width: 15
                                        height: 15
                                        anchors.centerIn: parent
                                        anchors.horizontalCenterOffset: root.mini ? 0 : -4.5
                                        tintName: (rowArea.containsMouse || isActive) ? "primary" : "muted"
                                    }
                                }
                                // Name (hidden in mini).
                                Text {
                                    visible: !root.mini
                                    width: parent.width - 24 - (isFolder ? 22 : 0)
                                    height: parent.height
                                    text: modelData.name
                                    color: (rowArea.containsMouse || isActive) ? theme.textPrimary : theme.textSecondary
                                    font.pixelSize: 13
                                    font.weight: isFolder ? theme.weightSemibold : theme.weightRegular
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                                // Folder count (hover) + chevron.
                                Text {
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
                                        QbzBridge.sidebarToggleFolder(modelData.id)
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
                        visible: !root.mini && root.entries.length === 0 && !QbzBridge.offline
                        width: plColumn.width
                        text: QbzBridge.tr("No playlists yet.", QbzBridge.trRev)
                        color: theme.textMuted
                        font.pixelSize: 12
                        wrapMode: Text.WordWrap
                    }
                }
            }
        }

        // Fill the rest.
        Item { width: 1; height: 0; }
    }
}
