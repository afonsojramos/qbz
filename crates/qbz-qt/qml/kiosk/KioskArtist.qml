// KioskArtist — lightweight kiosk artist detail (shell/KioskArtist.slint).
// Reads the same document as the desktop ArtistView (`QbzArtist.artistJson`):
// a header (round portrait + name + Play/Radio) and the discography as simple
// KioskCard shelves per release section (the junk "other" bucket is skipped).
//
// Keyboard/gamepad nav: SHELF mode, same model as KioskDiscover — one nav
// index per shelf (columns == 1, no tabs), Left/Right pulses `hmove` consumed
// by the focused shelf's local cursor, skipped shelves ("other" bucket / empty)
// are stepped over in the last travel direction. The header buttons (Play /
// Radio) stay touch-only: they carry no nav index and no focus ring.
//
// `count` counts EVERY section, including the ones this view refuses to
// render; the per-shelf skip cascade is what walks past them.
//
// Slint's `viewport-y` is negative as you scroll down; Qt's `contentY` is its
// positive twin — every occurrence is already translated. Both scroll writes
// run through Qt.callLater so they land on a settled layout, never inside a
// binding evaluation ("Recursion detected" class).
//
// Cover paths: the Slint's AlbumCardItem carries a resolved `artwork` that the
// artwork pipeline pushes into the shared model in place. The Qt documents
// carry only the remote `artUrl`, so this view dispatches the urls and resolves
// them out of `coverMap` when it hands each card down to KioskCard.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Flickable {
    id: root

    property real pad: 22

    readonly property var artist: JSON.parse(QbzArtist.artistJson)
    readonly property var sections: root.artist && root.artist.releaseSections
        ? root.artist.releaseSections
        : []

    contentWidth: root.width
    contentHeight: page.implicitHeight + 2 * root.pad
    clip: true
    boundsBehavior: Flickable.StopAtBounds

    QbzTheme { id: theme }

    // ---- Nav geometry publish: no tab strip, one index per shelf ---------
    function publishNav() {
        QbzKioskNav.publishNav(0, 1, root.sections.length, true)
    }

    readonly property int navCountProbe: root.sections.length
    onNavCountProbeChanged: root.publishNav()

    // ---- Artwork (Qt-only): dispatch + coalesce -------------------------
    // Covers arrive ONE AT A TIME, and on a warm cache the whole disk-hit set
    // lands in a single synchronous loop. Rebinding `coverMap` per arrival is
    // quadratic in the page, so arrivals are coalesced into ONE rebind per
    // frame (16ms granularity is invisible) — the ArtistView.qml fix.
    property var coverMap: ({})
    property var _coverInbox: ({})
    property var dispatchedCovers: ({})

    Timer {
        id: coverFlush
        interval: 16
        repeat: false
        onTriggered: {
            var m = Object.assign({}, root.coverMap, root._coverInbox)
            root._coverInbox = ({})
            // A rebind needs a NEW object reference (same-ref assignment is
            // not a change in QML).
            root.coverMap = m
        }
    }

    Connections {
        target: QbzLibrary
        // `sidebarArtworkWindow` keys its replies by the REQUESTED URL.
        function onLibraryArtworkReady(key, path) {
            root._coverInbox[key] = path
            if (!coverFlush.running)
                coverFlush.start()
        }
    }

    function dispatchArtwork(urls) {
        var fresh = []
        for (var i = 0; i < urls.length; i++) {
            var u = urls[i]
            if (!u || root.dispatchedCovers[u])
                continue
            root.dispatchedCovers[u] = true
            fresh.push(u)
        }
        if (fresh.length > 0)
            QbzShell.sidebarArtworkWindow(JSON.stringify(fresh))
    }

    // The portrait plus every section's cards — INCLUDING the sections this
    // view does not render, because a later publish pass can fill one and make
    // it renderable, and nothing downstream reports a url that was never sent.
    function collectArtwork() {
        var urls = []
        if (root.artist && root.artist.artUrl)
            urls.push(root.artist.artUrl)
        for (var s = 0; s < root.sections.length; s++) {
            var cards = root.sections[s].cards || []
            for (var c = 0; c < cards.length; c++)
                if (cards[c].artUrl)
                    urls.push(cards[c].artUrl)
        }
        root.dispatchArtwork(urls)
    }

    onArtistChanged: root.collectArtwork()

    Component.onCompleted: {
        root.publishNav()
        root.collectArtwork()
    }

    Column {
        id: page
        x: root.pad
        y: root.pad
        width: root.width - 2 * root.pad
        spacing: 22

        // ---- Header ------------------------------------------------------
        Item {
            id: headerRow
            width: page.width
            height: Math.max(140, headerInfo.implicitHeight)

            Rectangle {
                id: portrait
                anchors.left: headerRow.left
                anchors.verticalCenter: headerRow.verticalCenter
                width: 140
                height: 140
                radius: 70
                color: theme.surfaceElevated
                clip: true

                RoundedImage {
                    anchors.fill: portrait
                    source: root.artist && root.artist.artUrl
                        ? (root.coverMap[root.artist.artUrl] || "")
                        : ""
                    radius: 70
                    fit: "crop"
                }
            }

            Column {
                id: headerInfo
                anchors.left: portrait.right
                anchors.leftMargin: 22
                anchors.right: headerRow.right
                anchors.verticalCenter: headerRow.verticalCenter
                spacing: 12

                Text {
                    width: headerInfo.width
                    text: root.artist && root.artist.name ? root.artist.name : ""
                    color: theme.textPrimary
                    font.pixelSize: 25
                    font.weight: theme.weightBold
                    wrapMode: Text.WordWrap
                }

                Row {
                    spacing: 10

                    // TOUCH-ONLY. No nav index, no focus ring — the only state
                    // either button has is mouse hover.
                    Rectangle {
                        id: playBtn
                        width: 150
                        height: 44
                        radius: theme.radiusSm
                        color: playArea.containsMouse ? theme.accentHover : theme.accent

                        Text {
                            anchors.fill: playBtn
                            text: QbzSession.tr("Play", QbzSession.trRev)
                            color: theme.accentText
                            font.pixelSize: 15
                            font.weight: theme.weightSemibold
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }

                        MouseArea {
                            id: playArea
                            anchors.fill: playBtn
                            hoverEnabled: true
                            onClicked: {
                                if (root.artist && root.artist.id)
                                    QbzHome.playArtistTopTracks(root.artist.id)
                            }
                        }
                    }

                    Rectangle {
                        id: radioBtn
                        width: 130
                        height: 44
                        radius: theme.radiusSm
                        color: radioArea.containsMouse
                            ? theme.alphaTier(8)
                            : theme.surfaceElevated
                        border.width: 1
                        border.color: theme.borderSubtle

                        Text {
                            anchors.fill: radioBtn
                            text: QbzSession.tr("Radio", QbzSession.trRev)
                            color: theme.textPrimary
                            font.pixelSize: 15
                            font.weight: theme.weightSemibold
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }

                        MouseArea {
                            id: radioArea
                            anchors.fill: radioBtn
                            hoverEnabled: true
                            onClicked: {
                                if (root.artist && root.artist.id)
                                    QbzHome.startArtistRadio(root.artist.id)
                            }
                        }
                    }
                }
            }
        }

        // ---- Discography shelves ----------------------------------------
        // One delegate per section, INCLUDING the ones that render nothing:
        // their nav handlers are what step the focus over them. A non-rendering
        // delegate is invisible, so the Column gives it neither height nor a
        // spacing slot — exactly what the Slint's `if` does.
        Repeater {
            model: root.sections

            delegate: Item {
                id: shelfSlot

                required property var modelData
                required property int index

                readonly property var cards: shelfSlot.modelData && shelfSlot.modelData.cards
                    ? shelfSlot.modelData.cards
                    : []
                readonly property string releaseType: shelfSlot.modelData && shelfSlot.modelData.releaseType
                    ? shelfSlot.modelData.releaseType
                    : ""
                readonly property bool renders: shelfSlot.releaseType !== "other"
                    && shelfSlot.cards.length > 0

                property int cursor: 0
                readonly property bool shelfFocused: QbzKioskNav.navActive
                    && QbzKioskNav.zone === "content"
                    && (QbzKioskNav.index - QbzKioskNav.tabs) === shelfSlot.index

                visible: shelfSlot.renders
                width: page.width
                // 24px title + 12px spacing + 182px strip — the constant the
                // 218/240 page arithmetic below depends on.
                height: shelfSlot.renders ? 218 : 0

                // Step over the shelves this view does not render, in the last
                // travel direction. NOT a write to `QbzKioskNav.index`: the
                // qproperties are a mirror of the Rust NavModel (the single
                // source of truth — kiosk_nav_bridge.rs:109-110), so a QML
                // property write desyncs them and the next arrow key computes
                // from the stale model. Instead the skip re-runs the model's
                // own mover one more step; the boundary guards reproduce the
                // Slint clamp `max(tabs, min(count-1, index+dir))` — at either
                // edge the Slint write is a no-op, so no call is made (which
                // also avoids down()'s player escape and up()'s D5 rail wrap).
                // Each step re-fires every shelf's handler, so this cascades
                // until a renderable shelf or a boundary is hit. `dir === 0`
                // (no vertical move yet) deliberately does NOT skip.
                function skipIfUnrendered() {
                    if (QbzKioskNav.zone === "content" && QbzKioskNav.navActive
                        && (QbzKioskNav.index - QbzKioskNav.tabs) === shelfSlot.index
                        && (shelfSlot.releaseType === "other" || shelfSlot.cards.length === 0)
                        && QbzKioskNav.dir !== 0) {
                        if (QbzKioskNav.dir > 0
                            && QbzKioskNav.index < QbzKioskNav.count - 1)
                            QbzKioskNav.navDown()
                        else if (QbzKioskNav.dir < 0
                                 && QbzKioskNav.index > QbzKioskNav.tabs)
                            QbzKioskNav.navUp()
                    }
                }

                // Keep the focused shelf's strip inside the page viewport.
                // Fixed page arithmetic: 22px pad + 140px header + 22px
                // spacing = 184, then a 240px pitch per shelf block (24 title
                // + 12 spacing + 182 strip + 22 spacing) over a 218px block.
                function scrollShelfIntoView() {
                    if (!shelfSlot.shelfFocused || shelfSlot.cards.length === 0)
                        return
                    var top = 184 + shelfSlot.index * 240
                    if (top < root.contentY)
                        root.contentY = top - 8
                    else if (top + 218 > root.contentY + root.height)
                        root.contentY = top + 218 - root.height + 8
                }

                // Keep the cursor card inside the strip's horizontal viewport
                // (card pitch = 128px art + 14px spacing).
                function scrollCardIntoView() {
                    if (!shelfSlot.shelfFocused)
                        return
                    var cx = shelfSlot.cursor * 142
                    if (cx < stripFlick.contentX)
                        stripFlick.contentX = cx - 8
                    else if (cx + 128 > stripFlick.contentX + stripFlick.width)
                        stripFlick.contentX = cx + 128 - stripFlick.width + 8
                }

                onCursorChanged: Qt.callLater(shelfSlot.scrollCardIntoView)

                Connections {
                    target: QbzKioskNav

                    function onIndexChanged() {
                        shelfSlot.skipIfUnrendered()
                        Qt.callLater(shelfSlot.scrollShelfIntoView)
                    }

                    // Left/Right: consume the shell's pulse. Reading it through
                    // takeHmove() clears it in the same step, so a pulse can
                    // never be applied by two shelves.
                    function onHmoveChanged() {
                        if (shelfSlot.shelfFocused && QbzKioskNav.hmove !== 0
                            && shelfSlot.cards.length > 0) {
                            var d = QbzKioskNav.takeHmove()
                            shelfSlot.cursor = Math.max(0,
                                Math.min(shelfSlot.cursor + d, shelfSlot.cards.length - 1))
                        }
                    }

                    // Enter pulse → open the cursor album.
                    function onActivateSeqChanged() {
                        if (shelfSlot.shelfFocused && shelfSlot.cards.length > 0
                            && shelfSlot.cursor < shelfSlot.cards.length)
                            QbzAlbum.openAlbum(shelfSlot.cards[shelfSlot.cursor].id)
                    }
                }

                Column {
                    id: shelfCol
                    width: shelfSlot.width
                    spacing: 12

                    // Fixed-height title wrapper — keeps the shelf block a
                    // constant 218px for the scroll arithmetic above.
                    Item {
                        id: titleBox
                        width: shelfCol.width
                        height: 24

                        Text {
                            anchors.fill: titleBox
                            text: shelfSlot.modelData && shelfSlot.modelData.title
                                ? shelfSlot.modelData.title
                                : ""
                            color: theme.textPrimary
                            font.pixelSize: 18
                            font.weight: theme.weightSemibold
                            verticalAlignment: Text.AlignVCenter
                        }
                    }

                    Flickable {
                        id: stripFlick
                        width: shelfSlot.width
                        height: 182
                        contentWidth: strip.implicitWidth
                        contentHeight: stripFlick.height
                        clip: true
                        flickableDirection: Flickable.HorizontalFlick
                        boundsBehavior: Flickable.StopAtBounds

                        Row {
                            id: strip
                            spacing: 14

                            Repeater {
                                // Gated on `renders`, so an "other" bucket or an
                                // empty section never mounts a card.
                                model: shelfSlot.renders ? shelfSlot.cards : []

                                delegate: KioskCard {
                                    id: card

                                    required property var modelData
                                    required property int index

                                    album: ({
                                        "id": card.modelData.id,
                                        "title": card.modelData.title,
                                        "artist": card.modelData.artist,
                                        "artwork": root.coverMap[card.modelData.artUrl] || "",
                                        "qualityTier": card.modelData.qualityTier
                                    })
                                    navFocused: shelfSlot.shelfFocused
                                        && card.index === shelfSlot.cursor
                                    onClicked: function (id) {
                                        QbzAlbum.openAlbum(id)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
