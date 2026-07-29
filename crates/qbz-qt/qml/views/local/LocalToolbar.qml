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
//
// PRESENTATION DIVERGENCE (owner request — the row was too crowded):
//   * search   -> the EXPANDABLE form (Slint already uses ExpandableSearch
//                 here; the port used to mount an always-open 200px box);
//   * sort     -> LocalIconSelect "arrow-up-down"  (Slint: text QbzSelect)
//   * grouping -> LocalIconSelect "layers"         (Slint: text QbzSelect)
//   * filter   -> unchanged icon-only button (matches Slint) + a tooltip.
// Every option list, key mapping and callback is untouched.

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
    // Tooltip leads for the icon-only controls.
    readonly property string sortTip: QbzSession.tr("Sort", QbzSession.trRev)
    readonly property string groupTip: QbzSession.tr("Grouping", QbzSession.trRev)

    readonly property var tracksSortIds: ["default", "artist-asc", "artist-desc",
        "title-asc", "title-desc", "year-desc", "year-asc", "added-desc"]
    readonly property var tracksSortLabels: [
        QbzSession.tr("Default", QbzSession.trRev),
        QbzSession.tr("Artist A-Z", QbzSession.trRev),
        QbzSession.tr("Artist Z-A", QbzSession.trRev),
        QbzSession.tr("Title A-Z", QbzSession.trRev),
        QbzSession.tr("Title Z-A", QbzSession.trRev),
        QbzSession.tr("Year (newest)", QbzSession.trRev),
        QbzSession.tr("Year (oldest)", QbzSession.trRev),
        QbzSession.tr("Date added", QbzSession.trRev),
    ]
    readonly property var tracksGroupIds: ["off", "album", "artist", "name"]
    readonly property var tracksGroupLabels: [
        QbzSession.tr("No grouping", QbzSession.trRev),
        QbzSession.tr("By album", QbzSession.trRev),
        QbzSession.tr("By artist", QbzSession.trRev),
        QbzSession.tr("By name", QbzSession.trRev),
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
            anchors.verticalCenter: parent.verticalCenter
            placeholder: QbzSession.tr("Search", QbzSession.trRev)
            onEdited: function (v) { root.view.albumsSearch = v }
        }
        LocalIconSelect {
            anchors.verticalCenter: parent.verticalCenter
            iconName: "arrow-up-down"
            label: root.sortTip
            menuWidth: 190
            options: root.sortLabels
            currentIndex: Math.max(0, root.sortIds.indexOf(root.view.albumsSort))
            onSelected: function (i) { root.view.albumsSort = root.sortIds[i] }
        }
        LocalIconSelect {
            anchors.verticalCenter: parent.verticalCenter
            iconName: "layers"
            label: root.groupTip
            menuWidth: 180
            options: root.groupLabels
            marked: root.view.albumsGroup !== "off"
            currentIndex: Math.max(0, root.groupIds.indexOf(root.view.albumsGroup))
            onSelected: function (i) { root.view.albumsGroup = root.groupIds[i] }
        }
        // Album identity ("Albums by") — what one album IS. A QUERY change
        // (the grouping happens in SQL), so it reloads the set; persisted,
        // shared with the Slint app. Kept as a LABELLED select: it changes
        // what the tab shows, not how it is ordered, and no icon says that.
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
            LocalTip {
                visible: filtArea.containsMouse
                text: root.view.filterCount > 0
                    ? QbzSession.tr("Filters", QbzSession.trRev)
                        + " (" + root.view.filterCount + ")"
                    : QbzSession.tr("Quality, format and source filters", QbzSession.trRev)
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
                    // 9px bold on a 15px accent dot — the smallest on-accent
                    // text in the app, so the floor matters most here. NOT a
                    // departure from locallibrary/LocalLibraryView.slint:887
                    // (`Theme.accent-text`); the twin returns accent-text on
                    // 34 of the 35 palettes and only overrides rose-pine-dawn
                    // (2.56:1 -> 7.38:1).
                    color: theme.accentGlyphColor
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
            anchors.verticalCenter: parent.verticalCenter
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
        LocalIconSelect {
            anchors.verticalCenter: parent.verticalCenter
            iconName: "arrow-up-down"
            label: root.sortTip
            menuWidth: 190
            options: root.tracksSortLabels
            currentIndex: Math.max(0, root.tracksSortIds.indexOf(QbzLocal.localTracksSort))
            onSelected: function (i) { QbzLocal.tracksSetSort(root.tracksSortIds[i]) }
        }
        LocalIconSelect {
            anchors.verticalCenter: parent.verticalCenter
            iconName: "layers"
            label: root.groupTip
            menuWidth: 180
            options: root.tracksGroupLabels
            marked: root.view.tracksGroup !== "off"
            currentIndex: Math.max(0, root.tracksGroupIds.indexOf(root.view.tracksGroup))
            onSelected: function (i) { root.view.tracksGroup = root.tracksGroupIds[i] }
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
                anchors.verticalCenter: parent.verticalCenter
                placeholder: QbzSession.tr("Search", QbzSession.trRev)
                onEdited: function (v) { root.view.foldersSearch = v }
            }
            LocalIconSelect {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "arrow-up-down"
                label: root.sortTip
                menuWidth: 170
                options: root.sortLabels
                currentIndex: Math.max(0, root.sortIds.indexOf(root.view.foldersSort))
                onSelected: function (i) { root.view.foldersSort = root.sortIds[i] }
            }
            LocalIconSelect {
                anchors.verticalCenter: parent.verticalCenter
                iconName: "layers"
                label: root.groupTip
                menuWidth: 180
                options: root.groupLabels
                marked: root.view.foldersGroup !== "off"
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
        anchors.verticalCenter: parent.verticalCenter
        placeholder: QbzSession.tr("Search artists", QbzSession.trRev)
        onEdited: function (v) { root.view.artistsSearch = v }
    }
}
