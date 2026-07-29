// Folders tab body (LocalLibraryView.slint:1531) — three mutually exclusive
// arms: the ephemeral pane (an ad-hoc folder is open), FLAT mode (the album
// collection grouped by directory) and TREE mode (the two-pane filesystem
// browser).
//
// The tree divider is also the drag handle that resizes the rail: min 136px
// (half the 272px default), max half the content area — the Slint's own
// clamp at :1907.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    // ---- Ephemeral ----
    LocalEphemeralPane {
        anchors.fill: parent
        visible: root.view.ephemeralActive
        view: root.view
    }

    // Loading = the shape of the arm that is coming. FLAT mode is the same
    // collection the Albums tab renders; TREE mode is the rail + detail pair
    // (the rail rows are 34px folder rows with a leading glyph, so the
    // placeholder uses the small square art cell).
    QbzSkeleton {
        visible: QbzLocal.localFoldersLoading && !root.view.ephemeralActive
            && root.view.foldersMode === "flat"
        variant: root.view.foldersGridView === "grid" ? "cardGrid" : "rowList"
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 16
        cellW: 220
        cellH: 266
        rowH: 56
        rowGap: 0
        rowArtSize: 40
        phase: root.view.skelPhase
    }
    Row {
        visible: QbzLocal.localFoldersLoading && !root.view.ephemeralActive
            && root.view.foldersMode !== "flat"
        anchors.fill: parent
        anchors.topMargin: 12
        spacing: 0
        Item { width: 12; height: 1 }
        QbzSkeleton {
            variant: "rowList"
            width: root.view.treeRailWidth - 24
            height: parent.height
            // TreeRow is 26px with a small leading glyph (TreeRow.qml:34).
            rowH: 26
            rowGap: 0
            rowArtSize: 14
            phase: root.view.skelPhase
        }
        Item { width: 19; height: 1 }
        QbzSkeleton {
            variant: "cardGrid"
            width: parent.width - root.view.treeRailWidth - 7
            height: parent.height
            cellW: 220
            cellH: 266
            phase: root.view.skelPhase
        }
    }
    LocalNote {
        visible: !QbzLocal.localFoldersLoading && !root.view.ephemeralActive
            && root.view.folders.length === 0 && root.view.foldersSearch === ""
        text: QbzSession.tr("No folders in your local library yet.", QbzSession.trRev)
    }

    // ---------------------------- FLAT MODE ------------------------------
    Item {
        anchors.fill: parent
        visible: !QbzLocal.localFoldersLoading && !root.view.ephemeralActive
            && root.view.folders.length > 0 && root.view.foldersMode === "flat"

        LocalNote {
            visible: root.view.foldersVisible.length === 0
            text: QbzSession.tr("No folders match your search.", QbzSession.trRev)
        }
        LocalAlbumCollection {
            anchors.fill: parent
            anchors.leftMargin: 32
            anchors.rightMargin: 32
            anchors.topMargin: 16
            visible: root.view.foldersVisible.length > 0
            view: root.view
            rows: root.view.foldersVisible
            groups: root.view.foldersGrouped
            grouped: root.view.foldersGroup !== "off"
            viewMode: root.view.foldersGridView
            // Slint mounts the flat folders collection with show-source
            // false but show-source-badge true (:1606) — the badge on the
            // card, no source COLUMN in the list arm.
            showSource: true
            onOpenRequested: function (id) { root.view.openAlbum(id) }
            onPlayRequested: function (id) { QbzLocal.playAlbum(id, false) }
            onEnqueueRequested: function (id, m) { QbzLocal.enqueue("album", id, m) }
        }
    }

    // ---------------------------- TREE MODE ------------------------------
    Item {
        anchors.fill: parent
        visible: !QbzLocal.localFoldersLoading && !root.view.ephemeralActive
            && root.view.folders.length > 0 && root.view.foldersMode === "tree"

        LocalTreeRail {
            id: rail
            x: 0
            width: root.view.treeRailWidth
            height: parent.height
            view: root.view
        }

        // Divider + drag handle (7px grab area, 1px visible line).
        Item {
            id: handle
            x: rail.width
            width: 7
            height: parent.height
            Rectangle {
                x: 3
                width: 1
                height: parent.height
                color: dragArea.containsMouse || dragArea.pressed
                    ? theme.accent : theme.borderSubtle
            }
            MouseArea {
                id: dragArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.SizeHorCursor
                property real startW: 272
                property real startX: 0
                onPressed: function (mouse) {
                    startW = root.view.treeRailWidth
                    startX = mapToItem(root, mouse.x, mouse.y).x
                }
                onPositionChanged: function (mouse) {
                    if (!pressed) return
                    var dx = mapToItem(root, mouse.x, mouse.y).x - startX
                    root.view.treeRailWidth = Math.max(136,
                        Math.min(root.width / 2, startW + dx))
                }
            }
        }

        LocalFolderDetail {
            x: rail.width + 7
            width: parent.width - rail.width - 7
            height: parent.height
            view: root.view
        }
    }
}
