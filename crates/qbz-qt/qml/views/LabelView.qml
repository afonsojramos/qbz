// Label landing page — QML port of label/LabelPageView.slint.
//
// A circular-portrait header with description + Follow, a JUMP TO strip, a
// Popular Tracks list with a 5 -> 20 -> 50 reveal, and the Releases /
// Critics' Picks / Playlists / Artists / More Labels carousels.
//
// SCROLLING-PAGE CONVENTION (AlbumView / ArtistView / PlaylistView): the
// whole page scrolls in one Flickable whose Column pads 32 / 32 / top 11 /
// bottom 100. The .slint's NavButtons row is dropped (back/forward live in
// the shell header, HeaderBar.qml:235) but its 22px spacer is KEPT, so the
// header sits where every other detail page's does.
//
// POC-NOTEs, each deliberate:
// - JUMP TO is NOT sticky. ArtistView.qml:1204 documents the same call for
//   the .slint's sticky clamp; matching it keeps the two detail pages
//   consistent. This is the one accepted visual delta.
// - NO multi-select. qml/rows/TrackRow.qml has no multi-select arm, and two
//   of the .slint MultiSelectBar's seven actions (add-to-playlist,
//   add-to-mixtape) have no bridge seam at all (TrackRow.qml:108-112), so a
//   full port would ship two dead entries. The select disc renders DIMMED
//   and inert (the ArtistView "Radio" precedent) and the bar is absent.
// - NO "In library" tab. The Qt library feed carries no label id, so
//   `libraryCount` is always 0 and the SegmentedTabBar never mounts (see
//   src/label_qt.rs). A toggle that switches to an empty tab is worse.
// - Share copies the Qobuz link with no toast: there is no Rust clipboard
//   bridge and no toast seam (TrackRow.qml:538-558 documents both).
// - No scroll restore (the port has no NavState.restore-scope counterpart).
//
// LOGO FIT — a CONSCIOUS deviation: both .slint label views draw the logo
// `image-fit: cover` (LabelPageView.slint:116). This port draws it CONTAIN,
// the way qml/cards/LabelCard.qml already does, because label logos are
// wordmarks on transparent backgrounds and cropping one to a circle cuts the
// name off. The Slint behaviour is cover; this is not it, on purpose.

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../rows"
import "../theme"

Rectangle {
    id: root

    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    radius: 12

    QbzTheme { id: theme }

    readonly property var doc: {
        try {
            return JSON.parse(QbzHome.labelJson)
        } catch (e) {
            return {}
        }
    }
    readonly property var topTracks: doc.topTracks || []
    readonly property var releases: doc.releases || []
    readonly property var critics: doc.critics || []
    readonly property var playlists: doc.playlists || []
    readonly property var artists: doc.artists || []
    readonly property var moreLabels: doc.moreLabels || []
    readonly property var jumpTabs: doc.jumpTabs || []

    property string activeJumpTab: "about"
    /// Popular Tracks progressive reveal: 5 -> 20 -> 50 (the Tauri steps).
    property int previewCount: 5
    property string loadedLabelId: ""

    // Reset the per-label view state when the id actually changes (the
    // document is republished on the artwork pass too — ArtistView's
    // syncArtistState rule).
    onDocChanged: {
        var id = doc.id || ""
        if (id === root.loadedLabelId)
            return
        root.loadedLabelId = id
        root.previewCount = 5
        root.activeJumpTab = "about"
        flick.contentY = 0
    }

    function scrollToSection(id) {
        root.activeJumpTab = id
        if (id === "about") {
            flick.contentY = 0
            return
        }
        for (var i = 0; i < catalog.children.length; i++) {
            var c = catalog.children[i]
            if (c.anchorId !== undefined && c.anchorId === id) {
                var p = c.mapToItem(flick.contentItem, 0, 0)
                flick.contentY = Math.max(0, p.y - 48)
                return
            }
        }
    }

    // --- Share (TrackRow.qml's Loader/TextEdit pattern — QtQuick exposes no
    // Clipboard type, and there is no toast seam, so the copy is silent).
    Loader {
        id: clipLoader
        active: false
        sourceComponent: TextEdit { visible: false }
    }
    function copyToClipboard(text) {
        if (!text || text === "")
            return
        clipLoader.active = true
        clipLoader.item.text = text
        clipLoader.item.selectAll()
        clipLoader.item.copy()
        clipLoader.active = false
    }

    // ============================ shared rail =============================
    // ONE horizontal rail for all five carousels; `kind` picks the card, the
    // way the .slint mounts Carousel / PlaylistCarousel / ArtistCarousel.
    component CardRail: Column {
        id: cr
        property string title: ""
        property var items: []
        /// "album" | "playlist" | "artist" | "label"
        property string kind: "album"
        property bool showViewAll: false
        signal viewAllClicked()

        width: parent ? parent.width : 0
        spacing: 12

        readonly property int perPage: Math.max(1, Math.floor((crList.width + 32) / 232))
        readonly property int step: perPage * 232
        readonly property real maxScroll: Math.max(0, crList.contentWidth - crList.width)

        QbzSectionHeader {
            title: cr.title
            leftEnabled: crList.contentX > 1
            rightEnabled: crList.contentX < cr.maxScroll - 1
            showViewAll: cr.showViewAll
            onViewAllClicked: cr.viewAllClicked()
            onPageLeft: crList.contentX = Math.max(0, crList.contentX - cr.step)
            onPageRight: crList.contentX = Math.min(cr.maxScroll, crList.contentX + cr.step)
        }
        Item {
            width: parent.width
            height: 246
            ListView {
                id: crList
                anchors.fill: parent
                orientation: ListView.Horizontal
                spacing: 32
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: cr.items
                delegate: Item {
                    id: crCell
                    required property var modelData
                    width: 200
                    height: 246

                    Component {
                        id: crAlbum
                        AlbumCard {
                            albumId: crCell.modelData.id
                            title: crCell.modelData.title
                            artist: crCell.modelData.artist
                            artistId: crCell.modelData.artistId
                            genre: crCell.modelData.genre
                            year: crCell.modelData.year
                            qualityTier: crCell.modelData.qualityTier
                            ribbon: crCell.modelData.ribbon || ""
                            ribbonKind: crCell.modelData.ribbonKind || ""
                            artSource: crCell.modelData.artPath || ""
                            isPinned: crCell.modelData.isPinned === true
                            isFavorite: false
                        }
                    }
                    Component {
                        id: crPlaylist
                        PlaylistCard {
                            item: crCell.modelData
                            artSource: crCell.modelData.artPath || ""
                        }
                    }
                    Component {
                        id: crArtist
                        ArtistCard {
                            item: crCell.modelData
                            artSource: crCell.modelData.artPath || ""
                            // The .slint passes card-follow-mode "none" here;
                            // ArtistCard's hasFollowSeam already gates the chip
                            // off, so this is belt and braces.
                            followMode: "none"
                        }
                    }
                    Component {
                        id: crLabel
                        LabelCard {
                            item: crCell.modelData
                            artSource: crCell.modelData.artPath || ""
                        }
                    }
                    Loader {
                        anchors.fill: parent
                        sourceComponent: cr.kind === "playlist" ? crPlaylist
                            : cr.kind === "artist" ? crArtist
                            : cr.kind === "label" ? crLabel : crAlbum
                    }
                }
            }
        }
    }

    // ============================ offline gate ============================
    QbzOfflinePlaceholder {
        visible: QbzSession.offline
        anchors.centerIn: parent
        showSettingsAction: true
        onSettingsClicked: QbzShell.navigateTo("settings")
    }

    // ============================ the page ================================
    Flickable {
        id: flick
        anchors.fill: parent
        visible: !QbzSession.offline
        clip: true
        contentWidth: width
        contentHeight: page.implicitHeight
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: page
            width: parent.width
            leftPadding: 32
            rightPadding: 32
            topPadding: 11
            bottomPadding: 100
            spacing: 0

            // The .slint's NavButtons row is not drawn (shell header owns
            // back/forward) but its 22px spacer is kept.
            Item { width: 1; height: 22 }

            // --- Header ---------------------------------------------------
            Row {
                width: parent.width - 64
                spacing: 32

                Rectangle {
                    width: 180
                    height: 180
                    radius: 90
                    color: theme.surfaceElevated
                    clip: true

                    QbzIcon {
                        visible: (root.doc.imageUrl || "") === ""
                        name: "disc-3"
                        width: 60
                        height: 60
                        anchors.centerIn: parent
                        tintName: "muted"
                    }
                    // Contain-fit inside the square inscribed in the circle
                    // (180 / sqrt(2) ~= 127 -> margin 26) — see the header
                    // note on the logo-fit deviation.
                    RoundedImage {
                        anchors.fill: parent
                        anchors.margins: 26
                        source: root.doc.artPath || ""
                        radius: 0
                        fit: "contain"
                    }
                }

                Column {
                    width: parent.width - 180 - 32
                    anchors.top: parent.top
                    anchors.topMargin: 8
                    spacing: 0

                    Text {
                        text: QbzSession.tr("LABEL", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 10
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 1.5
                    }
                    Item { width: 1; height: 6 }
                    Text {
                        width: parent.width
                        text: root.doc.name || ""
                        color: theme.textPrimary
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }

                    Item { visible: (root.doc.description || "") !== ""; width: 1; height: 12 }
                    Text {
                        visible: (root.doc.description || "") !== ""
                        // .slint: `width: root.width - 296px`.
                        width: Math.max(0, root.width - 296)
                        text: root.doc.descriptionShort || ""
                        color: theme.textSecondary
                        font.pixelSize: theme.fontLegal
                        wrapMode: Text.WordWrap
                    }
                    Item { visible: root.doc.descriptionTruncated === true; width: 1; height: 4 }
                    Text {
                        visible: root.doc.descriptionTruncated === true
                        text: QbzSession.tr("Read more", QbzSession.trRev)
                        color: readMoreArea.containsMouse ? theme.accentHover : theme.accent
                        font.pixelSize: theme.fontLegal
                        MouseArea {
                            id: readMoreArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                var shell = root.parent
                                while (shell && shell.openTextModal === undefined)
                                    shell = shell.parent
                                if (!shell) return
                                // The .slint modal heading is @tr("About"),
                                // not the label name.
                                shell.openTextModal(
                                    QbzSession.tr("About", QbzSession.trRev),
                                    root.doc.description || "")
                            }
                        }
                    }

                    Item { width: 1; height: 18 }

                    // Action row — NO Play button (owner call, .slint:171-179:
                    // Popular Tracks already carries one and it does the same).
                    Row {
                        width: parent.width
                        spacing: 12
                        QbzCircleAction {
                            id: followBtn
                            readonly property bool following: root.doc.isFollowing === true
                            name: following ? "check" : "user-plus"
                            active: following
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzHome.labelFollow()
                        }
                        QbzCircleAction {
                            name: "shuffle"
                            btnEnabled: root.topTracks.length > 0
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzHome.labelShuffle()
                        }
                        QbzCircleAction {
                            id: headerMoreBtn
                            name: "ellipsis"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: function (mouse) {
                                headerMenu.openAtCursor(headerMoreBtn, mouse.x, mouse.y)
                            }
                        }
                        // Clamped stretch (ArtistView.qml:1161 — an unclamped
                        // negative width silently reflows the whole row).
                        Item {
                            width: Math.max(0, parent.width - 3 * 32 - 3 * 12)
                            height: 1
                        }
                    }
                }
            }

            CardMenu {
                id: headerMenu
                menuWidth: 224
                entries: [
                    { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next",
                      "enabled": root.topTracks.length > 0 },
                    { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later",
                      "enabled": root.topTracks.length > 0 },
                    { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue",
                      "enabled": root.topTracks.length > 0 },
                    { "label": QbzSession.tr("Share", QbzSession.trRev), "icon": "link", "action": "share" },
                ]
                onPicked: function (a) {
                    if (a === "share")
                        root.copyToClipboard("https://open.qobuz.com/label/" + (root.doc.id || ""))
                    else
                        QbzHome.labelTopAction(a)
                }
            }

            // --- JUMP TO ---------------------------------------------------
            // The .slint reserves 52px here for its sticky overlay; this port
            // puts the bar itself in the flow (see the header POC-NOTE).
            Item { visible: root.jumpTabs.length > 1; width: 1; height: visible ? 8 : 0 }
            QbzJumpNavBar {
                visible: root.jumpTabs.length > 1
                width: parent.width - 64
                padH: 0
                showSearch: false
                barBg: root.ambientOn ? "transparent" : theme.surfaceMain
                tabs: root.jumpTabs
                activeTabId: root.activeJumpTab
                onTabClicked: function (id) { root.scrollToSection(id) }
            }

            // --- Catalog ---------------------------------------------------
            Column {
                id: catalog
                width: parent.width - 64
                spacing: 0

                // First-paint block.
                Item {
                    visible: QbzHome.labelLoading && root.topTracks.length === 0
                    width: parent.width
                    height: visible ? 280 : 0
                    Column {
                        anchors.centerIn: parent
                        spacing: 18
                        QbzSpinner {
                            size: 36
                            anchors.horizontalCenter: parent.horizontalCenter
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: QbzSession.tr("Loading…", QbzSession.trRev)
                            color: theme.textMuted
                            font.pixelSize: 13
                        }
                    }
                }

                // --- Popular Tracks -------------------------------------
                // The WHOLE block is gated on a non-empty list (.slint:313).
                Column {
                    property string anchorId: "popular"
                    visible: root.topTracks.length > 0
                    width: parent.width
                    spacing: 0

                    Row {
                        width: parent.width
                        spacing: 12
                        Text {
                            width: Math.max(0, parent.width - 3 * 12 - 44 - 32 - 32)
                            anchors.verticalCenter: parent.verticalCenter
                            text: QbzSession.tr("Popular Tracks", QbzSession.trRev)
                            color: theme.textPrimary
                            font.pixelSize: theme.fontHeading
                            font.weight: theme.weightSemibold
                            elide: Text.ElideRight
                        }
                        QbzCircleAction {
                            primary: true
                            name: "play-fill"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzHome.labelPlayTop()
                        }
                        // Multi-select: DIMMED and inert (see the header note).
                        QbzCircleAction {
                            name: "square-check-big"
                            btnEnabled: false
                            anchors.verticalCenter: parent.verticalCenter
                        }
                        QbzCircleAction {
                            id: topMoreBtn
                            name: "ellipsis"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: function (mouse) {
                                topMenu.openAtCursor(topMoreBtn, mouse.x, mouse.y)
                            }
                        }
                    }
                    CardMenu {
                        id: topMenu
                        menuWidth: 224
                        entries: [
                            { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
                            { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
                            { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
                        ]
                        onPicked: function (a) { QbzHome.labelTopAction(a) }
                    }

                    Item { width: 1; height: 10 }

                    Repeater {
                        model: root.topTracks
                        delegate: TrackRow {
                            required property var modelData
                            required property int index
                            width: parent ? parent.width : 0
                            visible: index < root.previewCount
                            height: visible ? 50 : 0
                            item: modelData
                            number: index + 1
                            showArtwork: true
                            showAlbum: true
                            onPlayRequested: QbzHome.labelPlayTrack(modelData.id)
                            onEnqueueRequested: function (mode) {
                                QbzHome.labelEnqueueTrack(modelData.id, mode)
                            }
                        }
                    }

                    // Reveal control: 5 -> 20 -> 50, then back to 5.
                    Item { visible: root.topTracks.length > 5; width: 1; height: visible ? 4 : 0 }
                    Item {
                        visible: root.topTracks.length > 5
                        width: parent.width
                        height: visible ? 28 : 0
                        Rectangle {
                            anchors.centerIn: parent
                            width: revealText.implicitWidth + 24
                            height: 28
                            color: "transparent"
                            Text {
                                id: revealText
                                anchors.centerIn: parent
                                text: (root.previewCount < 50
                                       && root.topTracks.length > root.previewCount)
                                    ? QbzSession.tr("Load more", QbzSession.trRev)
                                    : QbzSession.tr("View less", QbzSession.trRev)
                                color: revealArea.containsMouse ? theme.textPrimary : theme.textSecondary
                                font.pixelSize: 13
                            }
                            MouseArea {
                                id: revealArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    if (root.previewCount < 50
                                        && root.topTracks.length > root.previewCount)
                                        root.previewCount = root.previewCount === 5 ? 20 : 50
                                    else
                                        root.previewCount = 5
                                }
                            }
                        }
                    }
                }

                // --- Releases ---------------------------------------------
                Column {
                    property string anchorId: "releases"
                    visible: root.releases.length > 0
                    width: parent.width
                    spacing: 0
                    Item { width: 1; height: 32 }
                    CardRail {
                        title: QbzSession.tr("Releases", QbzSession.trRev)
                        items: root.releases
                        kind: "album"
                        showViewAll: true
                        onViewAllClicked: QbzHome.labelOpenReleases()
                    }
                }

                // --- Critics' Picks ---------------------------------------
                Column {
                    property string anchorId: "critics"
                    visible: root.critics.length > 0
                    width: parent.width
                    spacing: 0
                    Item { width: 1; height: 32 }
                    CardRail {
                        title: QbzSession.tr("Critics' Picks", QbzSession.trRev)
                        items: root.critics
                        kind: "album"
                    }
                }

                // --- Playlists ---------------------------------------------
                Column {
                    property string anchorId: "playlists"
                    visible: root.playlists.length > 0
                    width: parent.width
                    spacing: 0
                    Item { width: 1; height: 32 }
                    CardRail {
                        title: QbzSession.tr("Playlists", QbzSession.trRev)
                        items: root.playlists
                        kind: "playlist"
                    }
                }

                // --- Artists ------------------------------------------------
                Column {
                    property string anchorId: "artists"
                    visible: root.artists.length > 0
                    width: parent.width
                    spacing: 0
                    Item { width: 1; height: 32 }
                    CardRail {
                        title: QbzSession.tr("Artists", QbzSession.trRev)
                        items: root.artists
                        kind: "artist"
                    }
                }

                // --- More Labels --------------------------------------------
                // NOTE the msgid: the carousel is "More labels" (lowercase l)
                // and the JUMP TO tab is "More Labels" — two distinct msgids
                // in the .slint, kept distinct here.
                Column {
                    property string anchorId: "labels"
                    visible: root.moreLabels.length > 0
                    width: parent.width
                    spacing: 0
                    Item { width: 1; height: 32 }
                    CardRail {
                        title: QbzSession.tr("More labels", QbzSession.trRev)
                        items: root.moreLabels
                        kind: "label"
                    }
                }
            }
        }
    }

    QbzScrollBar {
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        target: flick
    }
}
