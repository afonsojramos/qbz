// KioskAlbum — the lightweight kiosk album detail (QML port of
// crates/qbz-ui/ui/shell/KioskAlbum.slint, 217 lines). Reads the SAME
// QbzAlbum.albumJson document the desktop AlbumView reads: a header (big
// cover + title + artist + quality + Play/Shuffle) and a simple track list
// (number + title + duration). Album track rows carry no artwork
// (album_qt.rs:90-92 leaves artUrl empty on this view), so the row shows the
// track number instead.
//
// Keyboard/gamepad nav: the track list IS the nav model — a 1-column list
// with no tabs, so the nav index IS the track index. Enter plays the focused
// track. The header buttons (Play / Shuffle / the artist link) stay
// TOUCH-ONLY: no focus ring, no nav index, unreachable by arrows
// (KioskAlbum.slint:7-8).

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    // KioskAlbum.slint:38 sets no background — the kiosk content panel
    // (KioskShell.qml's surfaceMain card) is what shows through.
    color: "transparent"

    QbzTheme { id: theme }

    // KioskAlbum.slint:42.
    readonly property real pad: 22

    // ---------------------------------------------------------------------
    // Data (spec S5 §3.2: AlbumViewData = header + tracks)
    // ---------------------------------------------------------------------
    readonly property var albumDoc: {
        try {
            return JSON.parse(QbzAlbum.albumJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var header: root.albumDoc.header || ({})
    readonly property var tracks: root.albumDoc.tracks || []

    // ---------------------------------------------------------------------
    // Artwork (Qt-only: the Slint model carries a resolved `artwork`, the Qt
    // document carries only the remote `artUrl`)
    // ---------------------------------------------------------------------
    // ONE cover on this page, so this is a single resolved path rather than
    // the url->path map + 16ms coalescing timer the multi-cover views carry
    // (ArtistView.qml:664-681): with a single key there is no per-arrival
    // map copy to coalesce, and a map would be a per-row structure the page
    // has no rows for.
    property string coverPath: ""
    property var dispatchedCovers: ({})
    property string loadedAlbumId: ""

    Connections {
        target: QbzLibrary
        // Key is the REQUESTED URL on the sidebarArtworkWindow path
        // (main.rs:800-823 emits emit_library_artwork(url, path) in both the
        // disk-hit loop and the download tail); the `{kind}:{id}` shape in
        // the signal's doc comment is the FEED dispatch path.
        function onLibraryArtworkReady(key, path) {
            if (key === (root.header.artUrl || ""))
                root.coverPath = path
        }
    }

    // The document is republished as the deferred rails land, and every
    // republish re-fires the dispatch — send only what is new.
    function dispatchCover() {
        var u = root.header.artUrl || ""
        if (u === "" || root.dispatchedCovers[u])
            return
        var seen = root.dispatchedCovers
        seen[u] = true
        root.dispatchedCovers = seen
        QbzShell.sidebarArtworkWindow(JSON.stringify([u]))
    }

    // Per-album state is dropped only when the id actually changes, so a
    // deferred republish of the SAME album never blanks the cover.
    function syncAlbumState() {
        var id = root.header.id || ""
        if (id === root.loadedAlbumId)
            return
        root.loadedAlbumId = id
        root.dispatchedCovers = ({})
        root.coverPath = ""
    }

    // ---------------------------------------------------------------------
    // Nav geometry (KioskAlbum.slint:44-58): no tabs, one index per track,
    // not shelf mode.
    // ---------------------------------------------------------------------
    // The reference clamps `index` right after the publish (:50); the Qt
    // model does that clamp inside publish() itself (kiosk_nav_qt.rs:271-273),
    // so a QML-side write would only fight it.
    function publishNav() {
        QbzKioskNav.publishNav(0, 1, root.tracks.length, false)
    }

    Component.onCompleted: {
        root.syncAlbumState()
        root.dispatchCover()
        root.publishNav()
    }
    onHeaderChanged: {
        root.syncAlbumState()
        root.dispatchCover()
    }
    // The reference re-publishes off a `nav-len-probe` on the track count
    // (:55-58); the parsed list is the same probe here.
    onTracksChanged: root.publishNav()

    // ---------------------------------------------------------------------
    // Actions (spec S5 §3.2.6)
    // ---------------------------------------------------------------------
    // ("track", id, "play") dispatches to play_track_in_context in the
    // reference — "queue the current view's VISIBLE tracklist starting at the
    // clicked track" — which on an album page is playAlbumFrom. NOT
    // playTrack(id), which builds a one-track queue and loses the album.
    function playTrack(trackId) {
        QbzPlayer.playAlbumFrom(root.header.id || "", trackId)
    }

    // ---------------------------------------------------------------------
    // Zone-nav wiring
    // ---------------------------------------------------------------------
    Connections {
        target: QbzKioskNav
        // Enter pulse -> play the focused track (the row's own action). This
        // arm checks navActive explicitly (:63), unlike the shelf views where
        // it rides the shelf-focus predicate.
        function onActivateSeqChanged() {
            if (QbzKioskNav.navActive && QbzKioskNav.zone === "content"
                && QbzKioskNav.index >= 0
                && QbzKioskNav.index < root.tracks.length)
                root.playTrack(root.tracks[QbzKioskNav.index].id || "")
        }
        function onIndexChanged() {
            root.scrollFocusIntoView()
        }
    }

    // Scroll-into-view (:161-170). The page arithmetic is FIXED, exactly as
    // the reference documents at :154-158: 22 pad + 180 header + 22 spacing =
    // 224, then a 54px pitch (52px row + 2px spacing).
    //
    // Written from a handler through Qt.callLater so it lands on a SETTLED
    // layout — the reference warns three times that a viewport write which
    // feeds layout is the "Recursion detected" class (:156-158). contentY is
    // Qt's positive-going twin of Slint's negative viewport-y.
    function scrollFocusIntoView() {
        if (!QbzKioskNav.navActive || QbzKioskNav.zone !== "content")
            return
        var i = QbzKioskNav.index
        if (i < 0 || i >= root.tracks.length)
            return
        Qt.callLater(function () {
            var top = 224 + i * 54
            if (top < pageFlick.contentY)
                pageFlick.contentY = top - 8
            else if (top + 52 > pageFlick.contentY + pageFlick.height)
                pageFlick.contentY = top + 52 - pageFlick.height + 8
        })
    }

    // =====================================================================
    // The page (KioskAlbum.slint:69-216)
    // =====================================================================
    Flickable {
        id: pageFlick
        anchors.fill: parent
        contentWidth: pageFlick.width
        contentHeight: pageColumn.height + 2 * root.pad
        boundsBehavior: Flickable.StopAtBounds
        clip: true

        Column {
            id: pageColumn
            x: root.pad
            y: root.pad
            width: pageFlick.width - 2 * root.pad
            spacing: 22

            // --- Header (:78-144) ------------------------------------
            Row {
                id: headerRow
                width: pageColumn.width
                spacing: 22

                Rectangle {
                    id: coverTile
                    width: 180
                    height: 180
                    radius: theme.radiusMd
                    color: theme.surfaceElevated
                    clip: true

                    RoundedImage {
                        anchors.fill: coverTile
                        source: root.coverPath
                        radius: theme.radiusMd
                        fit: "crop"
                    }
                }

                // A plain cell so the column can vertically centre itself
                // (the reference's `alignment: center`, :94) without
                // anchoring a child of the Row itself.
                Item {
                    id: headerCell
                    width: headerRow.width - coverTile.width - headerRow.spacing
                    height: Math.max(coverTile.height, headerCol.height)

                    Column {
                        id: headerCol
                        width: headerCell.width
                        anchors.verticalCenter: headerCell.verticalCenter
                        spacing: 10

                        Text {
                            width: headerCol.width
                            text: root.header.title || ""
                            color: theme.textPrimary
                            font.pixelSize: 25
                            font.weight: theme.weightBold
                            wrapMode: Text.WordWrap
                        }

                        // Artist link (:103-120). Intrinsic width (the
                        // reference's `horizontal-stretch: 0`), so the tap
                        // target is the name and not the whole row. No
                        // underline, no hover colour, no cursor change — the
                        // reference has none.
                        Rectangle {
                            id: artistRow
                            width: artistText.implicitWidth
                            height: 26
                            color: "transparent"

                            Text {
                                id: artistText
                                anchors.left: artistRow.left
                                anchors.verticalCenter: artistRow.verticalCenter
                                text: root.header.artist || ""
                                color: theme.textSecondary
                                font.pixelSize: 17
                            }

                            MouseArea {
                                anchors.fill: artistRow
                                onClicked: QbzArtist.openArtist(root.header.artistId || "")
                            }
                        }

                        Text {
                            width: headerCol.width
                            text: root.header.qualityDetail || ""
                            color: theme.textMuted
                            font.pixelSize: 13
                        }

                        Row {
                            spacing: 10

                            PillButton {
                                label: QbzSession.tr("Play", QbzSession.trRev)
                                primary: true
                                onClicked: QbzPlayer.playAlbum(root.header.id || "")
                            }
                            PillButton {
                                label: QbzSession.tr("Shuffle", QbzSession.trRev)
                                onClicked: QbzPlayer.playAlbumShuffled(root.header.id || "")
                            }
                        }
                    }
                }
            }

            // --- Track list (:147-214) -------------------------------
            Column {
                id: trackColumn
                width: pageColumn.width
                spacing: 2

                Repeater {
                    model: root.tracks

                    delegate: Rectangle {
                        id: trackRow

                        required property var modelData
                        required property int index

                        width: trackColumn.width
                        height: 52
                        radius: theme.radiusSm

                        readonly property bool current:
                            QbzPlayer.npTrackId === (trackRow.modelData.id || "")
                        readonly property bool rowFocused:
                            QbzKioskNav.navActive
                            && QbzKioskNav.zone === "content"
                            && QbzKioskNav.index === trackRow.index

                        // Focus beats hover beats current-track (:171-175);
                        // note the 0.18 focus tint against the 0.10 current
                        // one.
                        color: trackRow.rowFocused
                            ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.18)
                            : rowArea.containsMouse
                            ? theme.alphaTier(6)
                            : trackRow.current
                            ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.10)
                            : "transparent"
                        border.width: trackRow.rowFocused ? 2 : 0
                        border.color: theme.accent

                        Text {
                            id: numberCell
                            anchors.left: trackRow.left
                            anchors.leftMargin: 12
                            anchors.verticalCenter: trackRow.verticalCenter
                            width: 30
                            text: trackRow.modelData.number || ""
                            color: theme.textMuted
                            font.pixelSize: 14
                            horizontalAlignment: Text.AlignRight
                        }

                        Text {
                            id: durationCell
                            anchors.right: trackRow.right
                            anchors.rightMargin: 12
                            anchors.verticalCenter: trackRow.verticalCenter
                            text: trackRow.modelData.duration || ""
                            color: theme.textMuted
                            font.pixelSize: 13
                        }

                        Text {
                            anchors.left: numberCell.right
                            anchors.leftMargin: 14
                            anchors.right: durationCell.left
                            anchors.rightMargin: 14
                            anchors.verticalCenter: trackRow.verticalCenter
                            text: trackRow.modelData.title || ""
                            color: trackRow.current ? theme.accent : theme.textPrimary
                            font.pixelSize: 15
                            elide: Text.ElideRight
                            wrapMode: Text.NoWrap
                            maximumLineCount: 1
                        }

                        MouseArea {
                            id: rowArea
                            anchors.fill: trackRow
                            hoverEnabled: true
                            onClicked: root.playTrack(trackRow.modelData.id || "")
                        }
                    }
                }
            }
        }
    }

    // The header's two pills (KioskAlbum.slint:13-36). File-local, as in the
    // reference — 132x44 here, not the 150px the artist header uses.
    component PillButton: Rectangle {
        id: pill

        property string label: ""
        property bool primary: false
        signal clicked

        // Its OWN theme instance: an inline component does not carry the
        // enclosing document's ids into qmllint's scope, so tokens read
        // through the outer `theme` would be unverifiable (LESSONS L3).
        QbzTheme { id: pillTheme }

        width: 132
        height: 44
        radius: pillTheme.radiusSm
        color: pill.primary
            ? pillTheme.accent
            : (pillArea.containsMouse ? pillTheme.alphaTier(8) : pillTheme.surfaceElevated)
        border.width: pill.primary ? 0 : 1
        border.color: pillTheme.borderSubtle

        Text {
            anchors.fill: pill
            text: pill.label
            color: pill.primary ? pillTheme.accentText : pillTheme.textPrimary
            font.pixelSize: 15
            font.weight: pillTheme.weightSemibold
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }

        MouseArea {
            id: pillArea
            anchors.fill: pill
            hoverEnabled: true
            onClicked: pill.clicked()
        }
    }
}
