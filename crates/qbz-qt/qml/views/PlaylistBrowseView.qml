// Qobuz Playlists "View all" page — QML port of
// discover/PlaylistBrowseView.slint: the playlist twin of DiscoverBrowseView
// (same fixed 56px header with the shared browse tools, genre overlay,
// infinite scroll and scrollbar), plus a single-select category tag bar
// under the header.
//
// ADR-008: the categories are radio DOTS, never pills (FilterRadio,
// PlaylistBrowseView.slint:24-68 — 22px tall, 14px ring / 1.5px border, 8px
// accent dot, 12px label that goes text-primary on select OR hover).
//
// Filtering, 1:1 with the .slint: the tag and the shared genre selection are
// SERVER-side (re-fetch from offset 0); the search box filters the loaded set
// client-side and disables load-more while active.
//
// Layout notes:
//   :80  chrome above the list = 56px, or 92px when the tag set is non-empty
//   :155 the 36px tag bar mounts ONLY when there are tags
//   :262 grid 200x246, gap 24; :281 list rows, spacing 2
//   :336 the genre popup anchors at y=56 EVEN with the 92px header — the
//        .slint overlaps the tag bar and this reproduces it rather than
//        inventing a different anchor
//
// The .slint declares only `media-action` (no open-album), and AppShell
// forwards only that; the same surface is kept here — playlist rows and
// cards route through QbzBridge.openPlaylist / QbzPlayer.

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz
import "../cards"
import "../controls"
import "../theme"

Rectangle {
    id: root

    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: theme.ambientOn
    radius: 12

    QbzTheme { id: theme }

    readonly property var doc: {
        try {
            return JSON.parse(QbzHome.playlistBrowseJson)
        } catch (e) {
            return {}
        }
    }
    readonly property var items: doc.items || []
    readonly property var tags: doc.tags || []
    readonly property string selectedTag: doc.selectedTag || ""
    readonly property string query: doc.query || ""
    readonly property string viewMode: doc.viewMode || "grid"
    readonly property int headerH: tags.length > 0 ? 92 : 56

    readonly property string genreSig: {
        try {
            var d = JSON.parse(QbzBridge.genreFilterJson)
            return JSON.stringify((d.names || {})["discover"] || [])
        } catch (e) {
            return ""
        }
    }
    property string lastGenreSig: ""
    onGenreSigChanged: {
        if (root.lastGenreSig !== "" && root.lastGenreSig !== root.genreSig)
            QbzHome.playlistBrowseGenreChanged()
        root.lastGenreSig = root.genreSig
    }
    Component.onCompleted: root.lastGenreSig = root.genreSig

    // Single-select category option (FilterRadio).
    component FilterRadio: Item {
        id: fr
        property string label: ""
        property bool selected: false
        signal clicked()

        width: frRow.width
        height: 22

        Row {
            id: frRow
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6
            Rectangle {
                width: 14
                height: 14
                radius: 7
                anchors.verticalCenter: parent.verticalCenter
                color: "transparent"
                border.width: 1.5
                border.color: fr.selected ? theme.accent : theme.textMuted
                Rectangle {
                    width: 8
                    height: 8
                    radius: 4
                    anchors.centerIn: parent
                    color: fr.selected ? theme.accent : "transparent"
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: fr.label
                color: (fr.selected || frArea.containsMouse)
                    ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 12
            }
        }
        MouseArea {
            id: frArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: fr.clicked()
        }
    }

    QbzOfflinePlaceholder {
        visible: QbzSession.offline
        anchors.centerIn: parent
        showSettingsAction: true
        onSettingsClicked: QbzShell.navigateTo("settings")
    }

    Column {
        anchors.fill: parent
        spacing: 0
        visible: !QbzSession.offline

        // --- Fixed 56px header -------------------------------------------
        Item {
            width: parent.width
            height: 56

            Rectangle {
                anchors.fill: parent
                // Rounded at the TOP because this bar is full-bleed at y=0 of
                // the content pane, and under the dynamic background the pane's
                // own rounding cannot reach it: Qt's `clip` is a rectangular
                // scissor that ignores `radius`, and AppShell hides its bezel
                // nubs while the field is meant to show through the corners.
                // So a full-bleed pane child rounds ITSELF — AppShell.qml says
                // exactly this ("there is no mask that can do it for it here")
                // and this bar was the counterexample the owner spotted in
                // Discover. Invisible with the background off: the view root
                // paints the same colour underneath.
                topLeftRadius: theme.radiusMd
                topRightRadius: theme.radiusMd
                color: root.ambientOn ? theme.surfaceMainA30 : theme.surfaceMain
            }

            Text {
                x: 48
                y: 25 - height / 2
                width: Math.max(0, plTools.x - 48 - 16)
                text: root.doc.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontSection
                font.weight: theme.weightBold
                elide: Text.ElideRight
            }

            Row {
                id: plTools
                x: parent.width - width - 32
                y: 25 - height / 2
                spacing: 8

                QbzLineEdit {
                    searchMode: true
                    width: 200
                    placeholder: QbzSession.tr("Search…", QbzSession.trRev)
                    text: root.query
                    onEdited: function (v) { QbzHome.playlistBrowseSearch(v) }
                }
                BrowseGenreButton {
                    context: "discover"
                    onClicked: genrePopup.toggle()
                }
                ViewModeToggle {
                    mode: root.viewMode
                    onSetMode: function (m) { QbzHome.playlistBrowseSetViewMode(m) }
                }
            }
        }

        // --- Category tag bar (only with tags) ---------------------------
        Item {
            visible: root.tags.length > 0
            width: parent.width
            height: visible ? 36 : 0

            Rectangle {
                anchors.fill: parent
                color: root.ambientOn ? theme.surfaceMainA30 : theme.surfaceMain
            }
            Flickable {
                x: 32
                width: parent.width - 64
                height: parent.height
                contentWidth: tagRow.width
                contentHeight: height
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Row {
                    id: tagRow
                    y: 7
                    spacing: 16
                    FilterRadio {
                        label: QbzSession.tr("All", QbzSession.trRev)
                        selected: root.selectedTag === ""
                        onClicked: QbzHome.playlistBrowseSelectTag("")
                    }
                    Repeater {
                        model: root.tags
                        delegate: FilterRadio {
                            required property var modelData
                            // Tag names arrive pre-localized from the API.
                            label: modelData.name
                            selected: modelData.selected === true
                            onClicked: QbzHome.playlistBrowseSelectTag(modelData.slug)
                        }
                    }
                }
            }
        }

        // --- Scrolling list ----------------------------------------------
        Item {
            width: parent.width
            height: parent.height - root.headerH

            Flickable {
                id: flick
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: page.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                onContentYChanged: {
                    if (contentY + height >= contentHeight - 600
                        && root.doc.hasMore === true
                        && !QbzHome.playlistBrowseLoadingMore
                        && !QbzHome.playlistBrowseLoading
                        && root.query === "")
                        QbzHome.playlistBrowseLoadMore()
                }

                Column {
                    id: page
                    width: parent.width
                    leftPadding: 32
                    rightPadding: 32
                    topPadding: 8
                    bottomPadding: 100
                    spacing: 0

                    QbzSpinner {
                        visible: QbzHome.playlistBrowseLoading
                        size: 36
                        anchors.horizontalCenter: parent.horizontalCenter
                    }

                    // Two empty states, split on whether a search is active.
                    Text {
                        visible: !QbzHome.playlistBrowseLoading
                            && root.items.length === 0 && root.query !== ""
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: QbzSession.tr("No results found.", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 14
                    }
                    Text {
                        visible: !QbzHome.playlistBrowseLoading
                            && root.items.length === 0 && root.query === ""
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: QbzSession.tr("No playlists match the selected categories.", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 14
                    }

                    // --- Grid ----------------------------------------------
                    Item {
                        id: plGrid
                        // --- Windowing + windowed artwork -----------------
                        //
                        // This grid used to mount EVERY card, and a View-all of
                        // playlists runs to hundreds — each one a PlaylistCard
                        // carrying a four-tile collage. AlbumCollection had
                        // solved this for the album pages and this one never
                        // got it.
                        //
                        // The band is SAMPLED on a timer rather than bound to
                        // `flick.contentY`: a direct binding re-evaluates every
                        // delegate on every scroll frame, which is the
                        // O(n)-per-frame cost the sampler exists to avoid (same
                        // reasoning, same 80 ms, as AlbumCollection).
                        //
                        // Cards outside the band keep their FOOTPRINT as bare
                        // Items, so the grid height — and with it the scroll
                        // geometry and the infinite-scroll trigger above —
                        // never changes.
                        property int bandFirst: 0
                        property int bandLast: 9999
                        function sampleBand() {
                            if (plGrid.columns <= 0)
                                return
                            var pitch = plGrid.cardH + plGrid.gap
                            var top = flick.contentY - plGrid.y
                            var h = flick.height
                            plGrid.bandFirst = Math.max(0, Math.floor((top - h) / pitch))
                            plGrid.bandLast = Math.max(0, Math.ceil((top + 2 * h) / pitch))
                        }

                        // Covers, one key at a time and only near the viewport
                        // — the Library > All shape. Rust used to download
                        // every missing cover of the page in one batch and then
                        // republish the whole document to attach the paths, so
                        // nothing had artwork until everything did and the
                        // republish rebuilt every delegate on arrival.
                        property var artMap: ({})
                        readonly property var _artAsked: ({ seen: ({}) })
                        function artOf(m) {
                            if (!m)
                                return ""
                            var u = m.artUrl || ""
                            if (u !== "" && plGrid.artMap[u])
                                return plGrid.artMap[u]
                            return m.artPath || ""
                        }
                        function reportArtWindow() {
                            if (plGrid.columns <= 0)
                                return
                            var lo = Math.max(0, plGrid.bandFirst * plGrid.columns)
                            var hi = Math.min(root.items.length - 1,
                                              (plGrid.bandLast + 1) * plGrid.columns - 1)
                            var pending = []
                            var asked = plGrid._artAsked.seen
                            for (var i = lo; i <= hi; i++) {
                                var it = root.items[i]
                                if (!it)
                                    continue
                                var u = it.artUrl || ""
                                if (u === "" || (it.artPath || "") !== ""
                                    || plGrid.artMap[u] || asked[u] === true)
                                    continue
                                asked[u] = true
                                pending.push(u)
                            }
                            if (pending.length > 0)
                                QbzShell.sidebarArtworkWindow(JSON.stringify(pending))
                        }
                        Timer {
                            interval: 80
                            repeat: true
                            running: plGrid.visible
                            onTriggered: {
                                plGrid.sampleBand()
                                plGrid.reportArtWindow()
                            }
                        }
                        Component.onCompleted: {
                            plGrid.sampleBand()
                            plGrid.reportArtWindow()
                        }
                        Connections {
                            target: QbzLibrary
                            function onLibraryArtworkReady(key, path) {
                                if (plGrid.artMap[key] === path
                                    || plGrid._artAsked.seen[key] !== true)
                                    return
                                var m = plGrid.artMap
                                m[key] = path
                                plGrid.artMap = Object.assign({}, m)
                            }
                        }

                        visible: !QbzHome.playlistBrowseLoading && root.viewMode !== "list"
                        width: parent.width - 64
                        readonly property int cardW: 200
                        readonly property int cardH: 246
                        readonly property int gap: 24
                        readonly property int columns: Math.max(
                            1, Math.floor((width + gap) / (cardW + gap)))
                        readonly property int rows: Math.ceil(root.items.length / columns)
                        height: rows > 0 ? rows * cardH + (rows - 1) * gap : 0

                        Repeater {
                            model: (root.viewMode !== "list") ? root.items : []
                            delegate: Item {
                                id: plCell
                                required property var modelData
                                required property int index
                                x: (plCell.index % plGrid.columns) * (plGrid.cardW + plGrid.gap)
                                y: Math.floor(plCell.index / plGrid.columns) * (plGrid.cardH + plGrid.gap)
                                width: plGrid.cardW
                                height: plGrid.cardH
                                readonly property int rowIndex:
                                    Math.floor(plCell.index / plGrid.columns)

                                // Component in the DELEGATE scope so
                                // `modelData` resolves (the AlbumCollection
                                // pattern).
                                Component {
                                    id: plCardComp
                                PlaylistCard {
                                    // `body-opens: true` in the .slint — the
                                    // card's body click opens the playlist and
                                    // the overlay button plays it.
                                    item: plCell.modelData
                                    artSource: plGrid.artOf(plCell.modelData)
                                    // The row carries the pin state
                                    // (home_qt `map_playlist`, which this page
                                    // reuses); the card defaults to false, so
                                    // without the hand-over the glyph read
                                    // "unpinned" on an already-pinned playlist
                                    // and the first click un-pinned it.
                                    // `artworkUrl` needs no hand-over: the
                                    // card defaults it to `item.artUrl`.
                                    isPinned: plCell.modelData.isPinned === true
                                }
                                }
                                Loader {
                                    anchors.fill: parent
                                    active: plCell.rowIndex >= plGrid.bandFirst
                                            && plCell.rowIndex <= plGrid.bandLast
                                    sourceComponent: plCardComp
                                }
                            }
                        }
                    }

                    // --- List ----------------------------------------------
                    Column {
                        visible: !QbzHome.playlistBrowseLoading && root.viewMode === "list"
                        width: parent.width - 64
                        spacing: 2
                        Repeater {
                            model: (root.viewMode === "list") ? root.items : []
                            delegate: PlaylistListRow {
                                required property var modelData
                                required property int index
                                item: modelData
                                rowIndex: index
                            }
                        }
                    }

                    Item { visible: QbzHome.playlistBrowseLoadingMore; width: 1; height: 24 }
                    QbzSpinner {
                        visible: QbzHome.playlistBrowseLoadingMore
                        size: 28
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
            }

            // Back/forward scroll memory (controls/ScrollMemory.qml): reports
            // this container's offset while it is the live page, and restores it
            // when a back/forward step arms this route.
            ScrollMemory { target: flick; scope: "playlistbrowse" }
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: flick
            }
        }
    }

    GenreFilterPopup {
        id: genrePopup
        anchors.fill: parent
        context: "discover"
        anchorTop: 56
        anchorRight: 32
    }
}
