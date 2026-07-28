// Subfolder cover card for the tree-mode folder-detail pane
// (LocalLibraryView.slint:57 FolderSubcard). Bordered card (radius 6, accent
// border on hover), square cover = card width - 16 with a folder placeholder,
// name 13px/medium, recursive track count 11px/muted. Click drills in.

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Rectangle {
    id: root

    property var item: ({})
    /// Resolved cover for item.artKey (the host's artMap lookup).
    property string artSource: ""
    signal opened()

    QbzTheme { id: theme }

    radius: 6
    border.width: 1
    border.color: cardArea.containsMouse ? theme.accent : theme.borderSubtle
    color: cardArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated

    Column {
        anchors.fill: parent
        anchors.margins: 8
        spacing: 6
        Rectangle {
            width: parent.width
            height: parent.width
            radius: 4
            color: theme.surfaceMain
            clip: true
            QbzIcon {
                name: "folder"
                width: 32
                height: 32
                anchors.centerIn: parent
                tintName: "muted"
            }
            RoundedImage {
                anchors.fill: parent
                source: root.artSource
                radius: 4
            }
        }
        Text {
            width: parent.width
            text: root.item.name || ""
            color: theme.textPrimary
            font.pixelSize: 13
            font.weight: theme.weightMedium
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            text: (root.item.trackCount || 0) + " "
                + QbzSession.tr("tracks", QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: 11
            elide: Text.ElideRight
        }
    }
    MouseArea {
        id: cardArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.opened()
    }
}
