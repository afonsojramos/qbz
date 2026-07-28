// Compact album row for the LIST arm of the Albums / Folders-flat / Artists
// surfaces (AlbumCollectionView's list mode, mounted by
// LocalLibraryView.slint:1255 with show-source / show-source-badge on).
//
// 56px: cover, title + artist, year, track count, quality mark and the
// source column. Multi-select puts the shared checkbox in front of the
// cover, as the collection view does.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Rectangle {
    id: root

    property var item: ({})
    property string artSource: ""
    property bool showSource: true
    property bool selectMode: false
    property bool checked: false
    signal opened()
    signal playRequested()
    signal toggleSelect()

    QbzTheme { id: theme }

    height: 56
    radius: 6
    color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.selectMode ? root.toggleSelect() : root.opened()
        onDoubleClicked: if (!root.selectMode) root.playRequested()
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 10
        anchors.rightMargin: 12
        spacing: 12

        Item {
            visible: root.selectMode
            width: visible ? 16 : 0
            height: parent.height
            SelectCheck {
                anchors.centerIn: parent
                on: root.checked
                onToggled: root.toggleSelect()
            }
        }
        Rectangle {
            width: 40
            height: 40
            anchors.verticalCenter: parent.verticalCenter
            radius: 4
            color: theme.surfaceElevated
            clip: true
            RoundedImage {
                anchors.fill: parent
                source: root.artSource
                radius: 4
            }
        }
        Column {
            width: parent.width - 40 - 70 - 90 - 92 - (root.showSource ? 34 : 0)
                - (root.selectMode ? 28 : 0) - 5 * 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                text: root.item.title || ""
                color: theme.textPrimary
                font.pixelSize: 14
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: root.item.artist || ""
                color: theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
            }
        }
        Text {
            width: 70
            anchors.verticalCenter: parent.verticalCenter
            text: root.item.year || ""
            color: theme.textMuted
            font.pixelSize: 12
        }
        Text {
            width: 90
            anchors.verticalCenter: parent.verticalCenter
            text: (root.item.trackCount || 0) + " "
                + QbzSession.tr("tracks", QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: 12
        }
        Item {
            width: 92
            height: parent.height
            QualityMini {
                tier: root.item.qualityTier || ""
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        Item {
            visible: root.showSource
            width: visible ? 34 : 0
            height: parent.height
            QbzIcon {
                visible: root.item.source === "local" || root.item.source === "plex"
                    || root.item.source === "offline"
                name: root.item.source === "offline" ? "cloud-download" : "hard-drive"
                width: 14
                height: 14
                anchors.verticalCenter: parent.verticalCenter
                tintName: root.item.source === "plex" ? "accent" : "muted"
            }
        }
    }
}
