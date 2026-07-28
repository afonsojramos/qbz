// One row of the FLATTENED folder tree (LocalLibraryView.slint:444 TreeRow).
//
// 26px, radius 6, depth indent 8 + depth*16, select-mode checkbox (folder =
// tri-state from node.selectState 0/1/2, track = node.selected), an expand
// chevron with its OWN hit target (so toggling never selects), the folder /
// music glyph and the segment name. The selection cue is the 3px accent bar
// on the left edge — ADR-008: never a pill.

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Rectangle {
    id: root

    property var node: ({})
    property bool selected: false
    property bool selectMode: false
    signal toggled()
    signal activated()
    signal toggleSelect()

    QbzTheme { id: theme }

    height: 26
    radius: 6
    color: selected ? theme.surfaceElevated
         : rowArea.containsMouse ? theme.surfaceHover : "transparent"

    // Row body — declared FIRST so the chevron / checkbox win their clicks.
    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.node.isFolder ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (root.node.isFolder) root.activated()
    }

    Rectangle {
        visible: root.selected
        x: 0
        y: 4
        width: 3
        height: parent.height - 8
        radius: 1.5
        color: theme.accent
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 8 + (root.node.depth || 0) * 16
        anchors.rightMargin: 10
        spacing: 6

        // Select-mode checkbox (folder tri-state / track boolean).
        Item {
            visible: root.selectMode
            width: visible ? 13 : 0
            height: parent.height
            SelectCheck {
                anchors.verticalCenter: parent.verticalCenter
                on: root.node.isFolder ? root.node.selectState === 2
                                       : root.node.selected === true
                partial: root.node.isFolder === true && root.node.selectState === 1
                onToggled: root.toggleSelect()
            }
        }

        // Expand chevron — its own hit target.
        Item {
            width: 18
            height: parent.height
            QbzIcon {
                visible: root.node.canExpand === true
                name: root.node.expanded ? "chevron-down" : "chevron-right"
                width: 11
                height: 11
                anchors.centerIn: parent
                tintName: "muted"
            }
            MouseArea {
                anchors.fill: parent
                enabled: root.node.canExpand === true
                cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
                onClicked: root.toggled()
            }
        }

        QbzIcon {
            name: root.node.isFolder
                ? (root.node.expanded ? "folder-open" : "folder") : "music"
            width: 14
            height: 14
            anchors.verticalCenter: parent.verticalCenter
            tintName: root.node.isFolder ? "accent" : "muted"
        }
        Text {
            width: Math.max(0, parent.width - 50 - (root.node.depth || 0) * 16
                            - (root.selectMode ? 19 : 0))
            height: parent.height
            text: root.node.segment || ""
            color: root.selected ? theme.textPrimary : theme.textSecondary
            font.pixelSize: 13
            font.weight: root.selected ? theme.weightSemibold : theme.weightRegular
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
    }
}
