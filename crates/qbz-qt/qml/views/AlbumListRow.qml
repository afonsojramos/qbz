// AlbumListRow — primitives/AlbumListRow.slint:129+, the LIST arm of a
// CATALOG album collection (Discover Browse, Label Releases). 64px
// (`list-row-h`, DiscoverBrowseView.slint:29), radius 8, odd-row zebra,
// hover surface-hover — coherent with TrackRow.
//
// The local twin is views/local/LocalAlbumRow.qml (56px, local actions, a
// host-owned skeleton). This one is Qobuz-wired: the body opens the catalog
// album, and the ⋯ menu carries the five entries the Slint always shows
// (Open album / Play / Play next / Play later / Add to queue). Right-clicking
// the row opens the same menu at the pointer, like every other ⋯ site.
//
// "Block this album" is absent for the same reason AlbumCard.qml gates it
// off: the Qt bridge exposes the blacklist COUNTERS but no block-album
// invokable, and a row that renders and does nothing is a defect.
//
// item contract: home_qt::HomeCard — { id, title, artist, artistId, year,
// qualityTier, qualityDetail, artPath }.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    property var item: ({})
    /// Row index — drives the even/odd zebra (coherent with TrackRow).
    /// NOT named `index`: a Repeater injects one into the delegate context
    /// and a same-named property here would shadow it.
    property int rowIndex: 0

    QbzTheme { id: theme }

    // Same columns (and widths) as AlbumListHeader.qml.
    readonly property int colArt: 52
    readonly property int colQuality: 150
    readonly property int colYear: 64
    readonly property int colOverflow: 36
    readonly property int colGap: 12

    readonly property bool rowHovered: rowArea.containsMouse || moreArea.containsMouse
        || artistArea.containsMouse

    width: parent ? parent.width : 0
    height: 64
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
                rowMenu.openAtCursor(rowArea, mouse.x, mouse.y)
            else
                QbzAlbum.openAlbum(root.item.id || "")
        }
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
            var id = root.item.id || ""
            if (id === "") return
            if (a === "open") QbzAlbum.openAlbum(id)
            else if (a === "play") QbzPlayer.playAlbum(id)
            else QbzPlayer.enqueueAlbum(id, a)
        }
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: root.colGap

        // Art cell (52px column, 44px thumb centred).
        Item {
            width: root.colArt
            height: parent.height
            Rectangle {
                width: 44
                height: 44
                anchors.centerIn: parent
                radius: 4
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: root.item.artPath || ""
                    radius: 4
                }
            }
        }

        // ITEM — title over artist (the artist line links to the artist).
        Column {
            width: parent.width - root.colArt - root.colQuality - root.colYear
                - root.colOverflow - 4 * root.colGap
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                text: root.item.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontLink
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                id: artistText
                width: parent.width
                text: root.item.artist || ""
                color: artistArea.containsMouse ? theme.textPrimary : theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
                MouseArea {
                    id: artistArea
                    anchors.fill: parent
                    // Only a real artist id is clickable — otherwise the
                    // pointer promises a page that cannot open.
                    enabled: (root.item.artistId || "") !== ""
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzArtist.openArtist(root.item.artistId)
                }
            }
        }

        // QUALITY — the SAME badge the album page shows (tier label over the
        // exact bit-depth / sample-rate line).
        Item {
            width: root.colQuality
            height: parent.height
            QualityBadgeFull {
                anchors.verticalCenter: parent.verticalCenter
                tier: root.item.qualityTier || ""
                detail: root.item.qualityDetail || ""
            }
        }

        Text {
            width: root.colYear
            height: parent.height
            text: root.item.year || ""
            color: theme.textMuted
            font.pixelSize: 12
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }

        Item {
            width: root.colOverflow
            height: parent.height
            Rectangle {
                width: 32
                height: 32
                radius: 6
                anchors.centerIn: parent
                color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    name: "ellipsis"
                    width: 18
                    height: 18
                    anchors.centerIn: parent
                    tintName: moreArea.containsMouse ? "textPrimary" : "muted"
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
}
