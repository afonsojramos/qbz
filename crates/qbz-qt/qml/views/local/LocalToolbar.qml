// Row-2 right-hand toolbar — the four per-tab control groups from
// LocalLibraryView.slint:774 (Albums), :924 (Tracks), :1009 (Folders) and
// :1103 (Artists). One file because they are one row: exactly one group is
// visible at a time and they share the search box, the select chrome and
// the 8px rhythm.
//
// Every Slint gate is kept: the Albums/Tracks groups only appear once the
// tab has rows OR a live search; the Folders flat-only block hides in tree
// mode while the Flat/Tree toggle stays; Artists shows the search only when
// there are artists.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Row {
    id: root

    /// The LocalLibraryView root (state + actions).
    property var view: null

    QbzTheme { id: theme }

    readonly property var sortIds: ["artist-asc", "artist-desc", "title-asc",
                                    "title-desc", "year-desc", "year-asc"]
    readonly property var sortLabels: [
        QbzSession.tr("Artist A-Z", QbzSession.trRev),
        QbzSession.tr("Artist Z-A", QbzSession.trRev),
        QbzSession.tr("Title A-Z", QbzSession.trRev),
        QbzSession.tr("Title Z-A", QbzSession.trRev),
        QbzSession.tr("Year (newest)", QbzSession.trRev),
        QbzSession.tr("Year (oldest)", QbzSession.trRev),
    ]
    readonly property var groupIds: ["off", "alpha", "artist"]
    readonly property var groupLabels: [
        QbzSession.tr("No grouping", QbzSession.trRev),
        QbzSession.tr("Group by letter", QbzSession.trRev),
        QbzSession.tr("Group by artist", QbzSession.trRev),
    ]

    height: 30
    spacing: 8

    // ============================ ALBUMS =================================
    Row {
        visible: root.view && root.view.activeTab === "albums"
            && (root.view.albums.length > 0 || root.view.albumsSearch !== "")
        spacing: 8
        height: parent.height

        LocalSearchBox {
            elevated: true
            placeholder: QbzSession.tr("Search", QbzSession.trRev)
            onEdited: function (v) { root.view.albumsSearch = v }
        }
        QbzSelect {
            menuWidth: 180
            height: 30
            anchors.verticalCenter: parent.verticalCenter
            options: root.sortLabels
            currentIndex: Math.max(0, root.sortIds.indexOf(root.view.albumsSort))
            onSelected: function (i) { root.view.albumsSort = root.sortIds[i] }
        }
        QbzSelect {
            menuWidth: 160
            height: 30
            anchors.verticalCenter: parent.verticalCenter
            options: root.groupLabels
            currentIndex: Math.max(0, root.groupIds.indexOf(root.view.albumsGroup))
            onSelected: function (i) { root.view.albumsGroup = root.groupIds[i] }
        }
        // Album identity ("Albums by") — what one album IS. A QUERY change
        // (the grouping happens in SQL), so it reloads the set; persisted,
        // shared with the Slint app.
        QbzSelect {
            menuWidth: 190
            height: 30
            anchors.verticalCenter: parent.verticalCenter
            options: [
                QbzSession.tr("Albums by folder", QbzSession.trRev),
                QbzSession.tr("Albums by metadata", QbzSession.trRev),
            ]
            currentIndex: QbzLocal.localAlbumMode === "metadata" ? 1 : 0
            onSelected: function (i) {
                QbzLocal.setAlbumMode(i === 1 ? "metadata" : "folder")
            }
        }
        // Quality/format/source filter popup trigger + active-count badge.
        Item {
            width: 34
            height: 30
            anchors.verticalCenter: parent.verticalCenter
            Rectangle {
                anchors.fill: parent
                radius: 6
                border.width: 1
                border.color: root.view.filterCount > 0 ? theme.accent : theme.borderSubtle
                color: filtArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                QbzIcon {
                    name: "list-filter"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: root.view.filterCount > 0 ? "accent" : "secondary"
                }
                MouseArea {
                    id: filtArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.view.filterOpen = !root.view.filterOpen
                }
            }
            Rectangle {
                visible: root.view.filterCount > 0
                x: parent.width - width + 3
                y: -4
                width: 15
                height: 15
                radius: 7.5
                color: theme.accent
                Text {
                    anchors.centerIn: parent
                    text: root.view.filterCount
                    color: theme.accentText
                    font.pixelSize: 9
                    font.weight: theme.weightBold
                }
            }
        }
        LocalSegToggle {
            anchors.verticalCenter: parent.verticalCenter
            segments: [{ "id": "grid", "icon": "layout-grid" },
                       { "id": "list", "icon": "list" }]
            mode: root.view.albumsView
            onSetMode: function (v) { root.view.albumsView = v }
        }
        QbzNavButton {
            anchors.verticalCenter: parent.verticalCenter
            name: "square-check-big"
            onClicked: root.view.toggleAlbumsMultiSelect()
        }
    }

    // ============================ TRACKS =================================
    Row {
        visible: root.view && root.view.activeTab === "tracks"
            && (root.view.tracks.length > 0 || root.view.tracksSearch !== "")
        spacing: 8
        height: parent.height

        QbzNavButton {
            anchors.verticalCenter: parent.verticalCenter
            name: "square-check-big"
            onClicked: root.view.toggleTracksMultiSelect()
        }
        LocalSearchBox {
            elevated: true
            placeholder: QbzSession.tr("Search", QbzSession.trRev)
            onEdited: function (v) {
                root.view.tracksSearch = v
                tracksSearchDebounce.restart()
            }
            // Server-side search: debounce so a keystroke is not a query.
            Timer {
                id: tracksSearchDebounce
                interval: 250
                onTriggered: QbzLocal.tracksSearch(root.view.tracksSearch)
            }
        }
        QbzSelect {
            menuWidth: 180
            height: 30
            anchors.verticalCenter: parent.verticalCenter
            options: [
                QbzSession.tr("Default", QbzSession.trRev),
                QbzSession.tr("Artist A-Z", QbzSession.trRev),
                QbzSession.tr("Artist Z-A", QbzSession.trRev),
                QbzSession.tr("Title A-Z", QbzSession.trRev),
                QbzSession.tr("Title Z-A", QbzSession.trRev),
                QbzSession.tr("Year (newest)", QbzSession.trRev),
                QbzSession.tr("Year (oldest)", QbzSession.trRev),
                QbzSession.tr("Date added", QbzSession.trRev),
            ]
            currentIndex: Math.max(0, ["default", "artist-asc", "artist-desc",
                "title-asc", "title-desc", "year-desc", "year-asc",
                "added-desc"].indexOf(QbzLocal.localTracksSort))
            onSelected: function (i) {
                QbzLocal.tracksSetSort(["default", "artist-asc", "artist-desc",
                    "title-asc", "title-desc", "year-desc", "year-asc",
                    "added-desc"][i])
            }
        }
        QbzSelect {
            menuWidth: 160
            height: 30
            anchors.verticalCenter: parent.verticalCenter
            options: [
                QbzSession.tr("No grouping", QbzSession.trRev),
                QbzSession.tr("By album", QbzSession.trRev),
                QbzSession.tr("By artist", QbzSession.trRev),
                QbzSession.tr("By name", QbzSession.trRev),
            ]
            currentIndex: Math.max(0, ["off", "album", "artist", "name"]
                .indexOf(root.view.tracksGroup))
            onSelected: function (i) {
                root.view.tracksGroup = ["off", "album", "artist", "name"][i]
            }
        }
    }

    // ============================ FOLDERS ================================
    Row {
        visible: root.view && root.view.activeTab === "folders"
            && !root.view.ephemeralActive
        spacing: 8
        height: parent.height

        // Flat-only controls — the tree rail carries its own search.
        Row {
            visible: root.view.foldersMode === "flat"
                && (root.view.folders.length > 0 || root.view.foldersSearch !== "")
            spacing: 8
            height: parent.height
            LocalSearchBox {
                elevated: true
                placeholder: QbzSession.tr("Search", QbzSession.trRev)
                onEdited: function (v) { root.view.foldersSearch = v }
            }
            QbzSelect {
                menuWidth: 150
                height: 30
                anchors.verticalCenter: parent.verticalCenter
                options: root.sortLabels
                currentIndex: Math.max(0, root.sortIds.indexOf(root.view.foldersSort))
                onSelected: function (i) { root.view.foldersSort = root.sortIds[i] }
            }
            QbzSelect {
                menuWidth: 150
                height: 30
                anchors.verticalCenter: parent.verticalCenter
                options: root.groupLabels
                currentIndex: Math.max(0, root.groupIds.indexOf(root.view.foldersGroup))
                onSelected: function (i) { root.view.foldersGroup = root.groupIds[i] }
            }
            LocalSegToggle {
                anchors.verticalCenter: parent.verticalCenter
                segments: [{ "id": "grid", "icon": "layout-grid" },
                           { "id": "list", "icon": "list" }]
                mode: root.view.foldersGridView
                onSetMode: function (v) { root.view.foldersGridView = v }
            }
        }
        // Flat / Tree — always visible on the Folders tab.
        // ASSET GAP: Slint uses disc-album / folder-tree; neither glyph is
        // baked in the Qt icon set yet (see GLUE), so this uses the closest
        // shipped pair.
        LocalSegToggle {
            anchors.verticalCenter: parent.verticalCenter
            segments: [{ "id": "flat", "icon": "disc" },
                       { "id": "tree", "icon": "folder-open" }]
            mode: root.view.foldersMode
            onSetMode: function (v) { root.view.foldersMode = v }
        }
    }

    // ============================ ARTISTS ================================
    LocalSearchBox {
        visible: root.view && root.view.activeTab === "artists"
            && root.view.artists.length > 0
        elevated: true
        placeholder: QbzSession.tr("Search artists", QbzSession.trRev)
        onEdited: function (v) { root.view.artistsSearch = v }
    }
}
