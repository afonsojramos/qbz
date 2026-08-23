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
    readonly property bool pulled: root.item.qobuzUnavailable === true
    readonly property int cacheStatus: root.item.cacheStatus !== undefined
        ? root.item.cacheStatus : 0
    readonly property bool pulledDead: root.pulled && root.cacheStatus !== 3
    /// `modifiers` rides straight off the mouse event: Shift is what turns
    /// a click into a range (controls/SelectionModel.qml).
    signal toggleSelect(int modifiers)

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
    color: rowHovered && !root.pulledDead ? theme.surfaceHover
         : (rowIndex % 2 === 1 ? theme.alphaTier(4) : "transparent")

    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: root.pulledDead && !root.selectMode
            ? Qt.ArrowCursor : Qt.PointingHandCursor
        onClicked: function (mouse) {
            if (mouse.button === Qt.RightButton) {
                // In select mode the row is a selection target, not a menu
                // host (Tauri/Slint parity: a right-click toggles nothing
                // there either).
                if (!root.selectMode)
                    root.openMenu(rowArea, mouse.x, mouse.y)
                return
            }
            if (root.selectMode) root.toggleSelect(mouse.modifiers)
            else if (!root.pulledDead) QbzAlbum.openAlbum(root.item.id || "")
        }
    }

    function menuEntries() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var m = []
        if (!root.pulledDead) {
            m.push({ "label": t("Open album", r), "icon": "library-big", "action": "open" })
            m.push({ "label": t("Play", r), "icon": "play-fill", "action": "play" })
            m.push({ "label": t("Play next", r), "icon": "list-start", "action": "next" })
            m.push({ "label": t("Play later", r), "icon": "list-plus", "action": "later" })
            m.push({ "label": t("Add to queue", r), "icon": "list-end", "action": "queue" })
        }
        var src = root.item.source || ""
        if (!root.pulledDead && src !== "local" && src !== "plex")
            m.push({ "label": t("Block this album", r), "icon": "blind-eye", "action": "block" })
        return m
    }
    function menuAction(a) {
        var id = root.item.id || ""
        if (id === "") return
        if (root.pulledDead) return
        if (a === "open") QbzAlbum.openAlbum(id)
        else if (a === "play") QbzPlayer.playAlbum(id)
        // `artUrl`, never `artPath`: the store keeps a denormalized cover url
        // and a file:// cache path is dead on any other machine.
        else if (a === "block") QbzBlacklist.blockAlbum(id, root.item.title || "",
            root.item.artist || "", root.item.artUrl || "")
        else QbzPlayer.enqueueAlbum(id, a)
    }
    Loader {
        id: rowMenuLoader
        active: false
        sourceComponent: CardMenu {
            menuWidth: 196
            entries: root.menuEntries()
            onPicked: function (a) { root.menuAction(a) }
        }
    }
    function openMenu(anchor, x, y) {
        if (root.menuEntries().length === 0)
            return
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
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        spacing: root.colGap
        opacity: root.pulledDead ? 0.5 : 1.0

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
                onToggled: function (mods) { root.toggleSelect(mods) }
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
                Rectangle {
                    visible: root.pulledDead
                    anchors.fill: parent
                    radius: 4
                    color: theme.alphaTier(60)
                    QbzIcon {
                        name: "circle-alert"
                        width: 18
                        height: 18
                        anchors.centerIn: parent
                        tintName: "favorite"
                    }
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
                visible: !root.pulledDead
                anchors.verticalCenter: parent.verticalCenter
                tier: root.item.qualityTier || ""
                detail: root.item.qualityDetail || ""
            }
            Text {
                visible: root.pulledDead
                anchors.verticalCenter: parent.verticalCenter
                text: QbzSession.tr("Unavailable", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                font.weight: theme.weightSemibold
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
            visible: !root.pulledDead
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
                    onClicked: function (mouse) { root.openMenu(moreArea, mouse.x, mouse.y) }
                }
            }
        }
    }
}
