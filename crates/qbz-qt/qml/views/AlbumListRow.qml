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
// "Block this album" is the sixth entry, live since QbzBlacklist landed
// (primitives/AlbumListRow.slint:434-441). It is pushed conditionally rather
// than sitting in the literal because the .slint gates it on a non-local /
// non-plex source — see the `entries` block.
//
// item contract: home_qt::HomeCard — { id, title, artist, artistId, year,
// qualityTier, qualityDetail, artUrl, artPath }. `artUrl` is the REMOTE cover
// url and `artPath` the local file:// cache path; only the former may be
// persisted (src/home_qt.rs:117-121).

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
    /// Cover override for hosts whose rows carry no `artPath` of their own.
    /// The Library feed is id-keyed (`artKey` -> a decoded file:// path in
    /// LibraryView's artMap), so its rows never gain the key AlbumCollection's
    /// producers bake in; everything else keeps working unchanged because ""
    /// falls straight back to `item.artPath`.
    property string artSource: ""
    /// Multi-select arm (FavoritesView.slint:514 — Library > Albums in LIST
    /// mode only). Default off, so the two catalog call sites are untouched.
    property bool selectMode: false
    property bool checked: false
    signal toggleSelect()

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
            if (mouse.button === Qt.RightButton) {
                // In select mode the row is a selection target, not a menu
                // host (Tauri/Slint parity: a right-click toggles nothing
                // there either).
                if (!root.selectMode)
                    rowMenu.openAtCursor(rowArea, mouse.x, mouse.y)
                return
            }
            if (root.selectMode) root.toggleSelect()
            else QbzAlbum.openAlbum(root.item.id || "")
        }
    }

    CardMenu {
        id: rowMenu
        menuWidth: 196
        // A JS expression, not a literal: the block row is CONDITIONAL. The
        // function form also re-evaluates on `trRev`, which the literal
        // already did through each `tr(…, trRev)` call.
        entries: {
            var t = QbzSession.tr
            var r = QbzSession.trRev
            var m = [
                { "label": t("Open album", r), "icon": "library-big", "action": "open" },
                { "label": t("Play", r), "icon": "play-fill", "action": "play" },
                { "label": t("Play next", r), "icon": "list-start", "action": "next" },
                { "label": t("Play later", r), "icon": "list-plus", "action": "later" },
                { "label": t("Add to queue", r), "icon": "list-end", "action": "queue" },
            ]
            // AlbumListRow.slint:434-441 gates the row on a non-local/plex
            // source. `home_qt::HomeCard` carries NO `source` field, so this
            // reads "" and the row shows — correct for these two catalog-only
            // surfaces (Discover Browse, Label Releases). Written defensively
            // so a producer that starts emitting `source` is honoured without
            // a second edit here.
            var src = root.item.source || ""
            if (src !== "local" && src !== "plex")
                m.push({ "label": t("Block this album", r), "icon": "blind-eye", "action": "block" })
            return m
        }
        onPicked: function (a) {
            var id = root.item.id || ""
            if (id === "") return
            if (a === "open") QbzAlbum.openAlbum(id)
            else if (a === "play") QbzPlayer.playAlbum(id)
            // `artUrl`, never `artPath`: the store keeps a denormalized cover
            // url and a file:// cache path is dead on any other machine.
            else if (a === "block") QbzBlacklist.blockAlbum(id, root.item.title || "",
                root.item.artist || "", root.item.artUrl || "")
            else QbzPlayer.enqueueAlbum(id, a)
        }
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: root.colGap

        // Selection checkbox — takes the leading slot in select mode
        // (MultiSelectBar's companion; the art cell keeps its own width so
        // the columns below the header never shift).
        Item {
            visible: root.selectMode
            width: visible ? 18 : 0
            height: parent.height
            QbzCheckbox {
                anchors.centerIn: parent
                checked: root.checked
                onToggled: root.toggleSelect()
            }
        }

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
                // No clip: RoundedImage confines its own crop on both arms.
                // One batch root per list row, for a scissor that never
                // rounded anything.
                RoundedImage {
                    anchors.fill: parent
                    source: root.artSource !== "" ? root.artSource : (root.item.artPath || "")
                    radius: 4
                }
            }
        }

        // ITEM — title over artist (the artist line links to the artist).
        Column {
            // The select checkbox adds a leading 18px cell AND a gap; without
            // subtracting both, the row overflows its width by 30px the moment
            // multi-select is switched on.
            width: parent.width - root.colArt - root.colQuality - root.colYear
                - root.colOverflow - 4 * root.colGap
                - (root.selectMode ? 18 + root.colGap : 0)
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
