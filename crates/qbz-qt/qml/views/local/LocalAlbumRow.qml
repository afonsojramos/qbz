// Compact album row for the LIST arm of the Albums / Folders-flat / Artists
// surfaces (AlbumCollectionView's list mode, mounted by
// LocalLibraryView.slint:1255 with show-source / show-source-badge on).
//
// 56px: cover, title + artist, year, track count, quality mark, the source
// column and the trailing ⋯ overflow. Multi-select puts the shared checkbox
// in front of the cover, as the collection view does.
//
// The ⋯ menu is AlbumListRow.slint:381 minus its two Qobuz-only entries: the
// favourite heart (local albums are not catalog favourites) and "Block this
// album" (which the Slint itself hides for source local/plex, :435). What is
// left is the five entries Slint always shows — Open album, Play, Play next,
// Play later, Add to queue — and all five are wired. Right-clicking the row
// opens the same menu at the pointer.

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
    signal enqueueRequested(string mode)
    signal toggleSelect()

    QbzTheme { id: theme }

    height: 56
    radius: 6
    color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) {
            if (mouse.button === Qt.RightButton) {
                rowMenu.openAtCursor(rowArea, mouse.x, mouse.y)
                return
            }
            if (root.selectMode) root.toggleSelect()
            else root.opened()
        }
        onDoubleClicked: if (!root.selectMode) root.playRequested()
    }

    CardMenu {
        id: rowMenu
        menuWidth: 196
        entries: [
            { "label": QbzSession.tr("Open album", QbzSession.trRev), "icon": "library-big", "action": "open" },
            { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
            { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
            { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
            { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
        ]
        onPicked: function (a) {
            if (a === "open") root.opened()
            else if (a === "play") root.playRequested()
            else root.enqueueRequested(a)
        }
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
            width: parent.width - 40 - 70 - 90 - 92 - 32
                - (root.showSource ? 34 : 0)
                - (root.selectMode ? 28 : 0) - 6 * 12
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
        // Trailing ⋯ overflow (AlbumListRow.slint:360).
        Rectangle {
            width: 32
            height: 32
            radius: 6
            anchors.verticalCenter: parent.verticalCenter
            color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
            QbzIcon {
                name: "ellipsis"
                width: 18
                height: 18
                anchors.centerIn: parent
                tintName: moreArea.containsMouse ? "primary" : "muted"
            }
            MouseArea {
                id: moreArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: function (mouse) { rowMenu.openAtCursor(moreArea, mouse.x, mouse.y) }
            }
        }
    }
}
