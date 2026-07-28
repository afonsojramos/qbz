// One row of the Artists master rail (LocalLibraryView.slint:609
// LocalArtistRow). 60px tall, radius 8, 48px ROUND avatar (cached photo or a
// user glyph), display name + "N albums · M tracks". Selected = accent fill
// with accent-text labels (that is the Slint, :615-660 — the accent fill is
// the master-list selection, not a pill).

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Rectangle {
    id: root

    property var item: ({})
    property bool selected: false
    property string artSource: ""
    signal picked(string name)

    QbzTheme { id: theme }

    height: 60
    radius: 8
    color: selected ? theme.accent
         : rowArea.containsMouse ? theme.surfaceHover : "transparent"

    Row {
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        spacing: 12
        Rectangle {
            width: 48
            height: 48
            radius: 24
            anchors.verticalCenter: parent.verticalCenter
            color: theme.surfaceElevated
            clip: true
            QbzIcon {
                visible: root.artSource === ""
                name: "user"
                width: 22
                height: 22
                anchors.centerIn: parent
                tintName: "muted"
            }
            RoundedImage {
                anchors.fill: parent
                source: root.artSource
                radius: 24
            }
        }
        Column {
            width: parent.width - 60
            anchors.verticalCenter: parent.verticalCenter
            spacing: 3
            Text {
                width: parent.width
                text: root.item.displayName || root.item.name || ""
                color: root.selected ? theme.accentText : theme.textPrimary
                font.pixelSize: theme.fontBody
                font.weight: root.selected ? theme.weightSemibold : theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: (root.item.albumCount || 0) + " "
                    + QbzSession.tr("albums", QbzSession.trRev) + " · "
                    + (root.item.trackCount || 0) + " "
                    + QbzSession.tr("tracks", QbzSession.trRev)
                color: root.selected ? theme.accentText : theme.textMuted
                font.pixelSize: theme.fontLegal
                elide: Text.ElideRight
            }
        }
    }
    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.picked(root.item.name)
    }
}
