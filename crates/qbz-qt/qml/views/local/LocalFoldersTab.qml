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

    QbzSpinner {
        anchors.centerIn: parent
        size: 36
        visible: QbzLocal.localFoldersLoading && !root.view.ephemeralActive
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
