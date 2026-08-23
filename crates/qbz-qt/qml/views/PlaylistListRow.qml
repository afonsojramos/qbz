// PlaylistListRow — 1:1 port of primitives/PlaylistListRow.slint: the LIST
// arm of the Qobuz Playlists browse page. 60px, radius 8, odd-row zebra,
// 44px thumb, name over "owner   •   N tracks", a play affordance and the ⋯
// menu. The whole row body opens the playlist.
//
// item contract: home_qt::HomeCard as the playlist pages publish it —
// { id, title, subtitle, artPath }.
//
// Menu inventory vs the .slint's five rows: Play / Play next / Play later /
// Add to queue are wired through QbzPlayer; "Follow on Qobuz" is NOT — the
// follow invokable takes the playlist that is currently OPEN
// (`playlistToggleFollow`), so a row cannot call it, exactly as
// PlaylistCard.qml documents for "Copy to your library". The entry stays out
// of the menu rather than rendering and doing nothing.

import QtQuick
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../theme"

Rectangle {
    id: root

    property var item: ({})
    /// Cover override for hosts whose rows carry no `artPath` of their own.
    /// The Library feed is id-keyed (`artKey` -> a decoded file:// path in
    /// LibraryView's artMap); "" falls straight back to `item.artPath`, so the
    /// browse pages are untouched.
    property string artSource: ""
    /// Up to four member-track covers — the 2x2 mosaic a USER playlist shows
    /// when it has no artwork of its own (`library_qt::FeedItem.covers`). The
    /// browse pages publish none, so they keep the designed glyph placeholder.
    property var covers: []
    readonly property string artResolved:
        root.artSource !== "" ? root.artSource : (root.item.artPath || "")
    /// Row index — drives the even/odd zebra (coherent with TrackRow).
    /// NOT named `index`: a Repeater injects one into the delegate context
    /// and a same-named property here would shadow it.
    property int rowIndex: 0

    QbzTheme { id: theme }

    readonly property bool rowHovered: rowArea.containsMouse || playArea.containsMouse
        || moreArea.containsMouse

    width: parent ? parent.width : 0
    height: 60
    radius: theme.radiusSm
    color: rowHovered ? theme.surfaceHover
         : (rowIndex % 2 === 1 ? theme.alphaTier(4) : "transparent")

    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) {
            if (mouse.button === Qt.RightButton)
                root.openMenu(rowArea, mouse.x, mouse.y)
            else
                QbzBridge.openPlaylist(root.item.id || "")
        }
    }

    Loader {
        id: rowMenuLoader
        active: false
        sourceComponent: CardMenu {
            menuWidth: 196
            entries: [
                { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
                { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
                { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
            ]
            onPicked: function (a) {
                var id = root.item.id || ""
                if (id === "") return
                if (a === "play") QbzPlayer.playPlaylistById(id)
                else QbzPlayer.enqueuePlaylistById(id, a)
            }
        }
    }
    function openMenu(anchor, x, y) {
        rowMenuLoader.active = true
        rowMenuLoader.item.openAtCursor(anchor, x, y)
    }
    function releaseForReuse() {
        if (rowMenuLoader.item)
            rowMenuLoader.item.close()
        rowMenuLoader.active = false
    }
    ListView.onPooled: root.releaseForReuse()

    Row {
        anchors.fill: parent
        anchors.leftMargin: 10
        anchors.rightMargin: 10
        spacing: 12

        Rectangle {
            width: 44
            height: 44
            anchors.verticalCenter: parent.verticalCenter
            radius: 6
            color: theme.surfaceElevated
            clip: true
            // These rows come from `home_qt::map_playlist` (browse_qt.rs:612),
            // whose art is the playlist's own `image.rectangle` — 800x380.
            // `auto` pads that instead of cropping to the middle 47% of it,
            // and still crops the square `image.covers[0]` fallback.
            RoundedImage {
                anchors.fill: parent
                visible: !rowCollage.visible
                source: root.artResolved
                radius: 6
                fit: "auto"
            }
            PlaylistCollage {
                id: rowCollage
                anchors.fill: parent
                visible: root.artResolved === "" && (root.covers || []).length > 0
                urls: root.covers || []
                radius: 6
            }
            QbzIcon {
                visible: root.artResolved === "" && !rowCollage.visible
                name: "list-music"
                width: 20
                height: 20
                anchors.centerIn: parent
                tintName: "muted"
            }
        }

        Column {
            width: parent.width - 44 - 2 * 30 - 4 * 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                text: root.item.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontBody
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: root.item.subtitle || ""
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                elide: Text.ElideRight
            }
        }

        Item {
            width: 30
            height: parent.height
            QbzIcon {
                name: "play-fill"
                width: 16
                height: 16
                anchors.centerIn: parent
                tintName: root.rowHovered ? "textPrimary" : "muted"
            }
            MouseArea {
                id: playArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: QbzPlayer.playPlaylistById(root.item.id || "")
            }
        }
        Item {
            width: 30
            height: parent.height
            QbzIcon {
                name: "ellipsis"
                width: 16
                height: 16
                anchors.centerIn: parent
                tintName: moreArea.containsMouse ? "textPrimary" : "secondary"
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
