// Tracks tab body (LocalLibraryView.slint:1317) — the SERVER-PAGINATED flat
// list. This is the surface that froze the Slint build at 16k tracks: the
// model is never the whole table (500 rows per page, appended within 600px
// of the tail — the Slint's own trigger distance at :1442) and the ListView
// only ever instantiates its window.
//
// Grouping (off / album / artist / name) inserts 32px header rows, so the
// model is an entry array — { t: 0, label } | { t: 1, row, n } — exactly as
// the collection does it, keeping ONE windowing ListView with variable row
// heights instead of a Repeater per group.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    readonly property int rowH: 50
    readonly property int headerH: 32

    // --------------------------- entry model ------------------------------
    property var entries: []
    property var alphaJumps: []
    /// entry index -> index into `view.tracksVisible`, -1 for a group header.
    /// Group headers inflate the entry indices, so reporting them straight
    /// into the row array asked for the covers of tracks BELOW the ones on
    /// screen, by the number of headers scrolled past.
    property var rowIndex: []
    function rebuild() {
        var rows = view ? view.tracksVisible : []
        var mode = view ? view.tracksGroup : "off"
        var out = []
        var jumps = []
        var idx = []
        var prev = null
        for (var i = 0; i < rows.length; i++) {
            var t = rows[i]
            var key = mode === "album" ? (t.album || "")
                    : mode === "artist" ? (t.artist || "")
                    : mode === "name" ? (t.title || "").slice(0, 1).toUpperCase()
                    : ""
            if (mode !== "off" && key !== "" && key !== prev) {
                if (mode === "name") jumps.push({ "letter": key, "index": out.length })
                out.push({ "t": 0, "label": key })
                idx.push(-1)
                prev = key
            }
            out.push({ "t": 1, "row": t, "n": i + 1 })
            idx.push(i)
        }
        entries = out
        alphaJumps = jumps
        rowIndex = idx
        report()
    }
    Component.onCompleted: { rebuild(); reportSoon() }
    Component.onDestruction: if (view) view.releaseWindow("tracks")
    Connections {
        target: root.view
        function onTracksVisibleChanged() { root.rebuild() }
        function onTracksGroupChanged() { root.rebuild() }
        // The row artwork column is OFF by default; turning it on has to ask
        // for the covers the rows are already bound to.
        function onTrackArtworkChanged() { root.reportSoon() }
        function onArtworkRefresh() { root.reportSoon() }
    }

    // First page = the shape of the 50px track rows (the Slint mounts a bare
    // 36px LoadingSpinner). ONE instance for the whole viewport.
    QbzSkeleton {
        variant: "rowList"
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 12
        visible: QbzLocal.localTracksLoading
        rowH: root.rowH
        rowGap: 0
        rowArt: root.view.trackArtwork
        rowArtSize: 36
        phase: root.view.skelPhase
    }
    LocalNote {
        visible: !QbzLocal.localTracksLoading && root.view.tracks.length === 0
            && root.view.tracksSearch === ""
        text: QbzSession.tr("No tracks in your local library yet.", QbzSession.trRev)
    }
    LocalNote {
        visible: !QbzLocal.localTracksLoading
            && (root.view.tracks.length === 0 || root.view.tracksVisible.length === 0)
            && root.view.tracksSearch !== ""
        text: QbzSession.tr("No tracks match your search.", QbzSession.trRev)
    }

    Column {
        anchors.fill: parent
        anchors.leftMargin: 32
        anchors.rightMargin: 32
        anchors.topMargin: 12
        spacing: 8
        visible: !QbzLocal.localTracksLoading && root.view.tracksVisible.length > 0

        LocalMultiSelectBar {
            visible: root.view.tracksMultiSelect
            width: parent.width
            selectedCount: root.view.tracksSelectedCount
            actions: [
                { "id": "select-all", "label": QbzSession.tr("Select all", QbzSession.trRev), "icon": "square-check-big", "danger": false, "needsSelection": false },
                { "id": "queue", "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "danger": false, "needsSelection": true },
                { "id": "play-later", "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "danger": false, "needsSelection": true },
                { "id": "play-next", "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "danger": false, "needsSelection": true },
                { "id": "remove-favorites", "label": QbzSession.tr("Remove from favorites", QbzSession.trRev), "icon": "heart", "danger": true, "needsSelection": true },
                { "id": "add-to-playlist", "label": QbzSession.tr("Add to playlist", QbzSession.trRev), "icon": "list-music", "danger": false, "needsSelection": true },
                { "id": "add-to-mixtape", "label": QbzSession.tr("Add to Mixtape/Collection", QbzSession.trRev), "icon": "cassette-tape", "danger": false, "needsSelection": true },
                { "id": "clear", "label": QbzSession.tr("Clear", QbzSession.trRev), "icon": "x", "danger": false, "needsSelection": true },
            ]
            onAction: function (id) { root.view.tracksBulkAction(id) }
        }

        Item {
            width: parent.width
            height: parent.height - (root.view.tracksMultiSelect ? 52 : 0)

            ListView {
                id: list
                anchors.fill: parent
                anchors.rightMargin: root.view.tracksGroup === "name" ? 20 : 0
                clip: true
                cacheBuffer: 50 * 12
                boundsBehavior: Flickable.StopAtBounds
                model: root.entries

                onContentYChanged: {
                    root.report()
                    // Infinite scroll: 600px of runway (Slint :1442).
                    if (contentY + height >= contentHeight - 600
                        && QbzLocal.localTracksHasMore
                        && !QbzLocal.localTracksLoadingMore
                        && !QbzLocal.localTracksLoading) {
                        QbzLocal.tracksLoadMore()
                    }
                }
                onModelChanged: root.report()
                onHeightChanged: root.report()
                onVisibleChanged: {
                    if (visible) root.reportSoon()
                    else if (root.view) root.view.releaseWindow("tracks")
                }

                delegate: Loader {
                    required property var modelData
                    width: list.width
                    height: modelData.t === 0 ? root.headerH : root.rowH
                    sourceComponent: modelData.t === 0 ? headerComp : rowComp

                    Component {
                        id: headerComp
                        Item {
                            Text {
                                x: 2
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 4
                                text: modelData.label
                                color: theme.textSecondary
                                font.pixelSize: theme.fontLegal
                                font.weight: theme.weightSemibold
                                elide: Text.ElideRight
                            }
                        }
                    }
                    Component {
                        id: rowComp
                        LocalTrackRow {
                            width: list.width
                            view: root.view
                            item: modelData.row
                            // The row artwork column was rendering a
                            // permanently blank 36px cell: local track rows
                            // ship an EMPTY artPath (local_rows.rs:284) and
                            // nothing ever fed LocalTrackRow.artSource. It is
                            // the same id-keyed artMap every other local
                            // surface reads.
                            artSource: root.view.artMap[modelData.row.artKey] || ""
                            number: modelData.n
                            // Album column hides when the list is already
                            // grouped by album (Slint :1486).
                            showAlbum: root.view.tracksGroup !== "album"
                            showArtwork: root.view.trackArtwork
                            selectMode: root.view.tracksMultiSelect
                            checked: root.view.tracksSelected[modelData.row.id] === true
                            onPlayRequested: QbzLocal.playTrack(modelData.row.id)
                            onEnqueueRequested: function (m) {
                                QbzLocal.enqueue("track", modelData.row.id, m)
                            }
                            onToggleSelect: root.view.toggleTrackSelected(modelData.row.id)
                        }
                    }
                }

                // Infinite scroll: the NEXT page in the shape it will arrive
                // in (3 rows), appended below the last real row. Each
                // placeholder is replaced by a real row as the page lands.
                footer: Item {
                    width: list.width
                    height: QbzLocal.localTracksLoadingMore ? 3 * root.rowH : 0
                    QbzSkeleton {
                        variant: "rowList"
                        anchors.fill: parent
                        visible: QbzLocal.localTracksLoadingMore
                        rowH: root.rowH
                        rowGap: 0
                        rowArt: root.view.trackArtwork
                        rowArtSize: 36
                        phase: root.view.skelPhase
                    }
                }
            }
            AlphaStrip {
                visible: root.view.tracksGroup === "name" && root.alphaJumps.length > 0
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                jumps: root.alphaJumps
                onJump: function (ordinal, index) {
                    list.positionViewAtIndex(index, ListView.Beginning)
                }
            }
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: root.view.tracksGroup === "name" ? 22 : 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: list
                visible: list.contentHeight > list.height
            }
        }
    }

    // TRIGGERS: `contentY` + the model swap alone never fired for a list that
    // was still hidden behind `localTracksLoading` when its first page landed
    // — the covers only started resolving on the first scroll. Mount, the
    // rebuild, becoming visible and a resize report too.
    function report() {
        if (!view || !list) return
        // The row artwork column is gated on the appearance setting (default
        // OFF, for the 16K-track freeze). With it off nothing on this list
        // renders a cover, so nothing is worth decoding.
        if (!list.visible || !view.trackArtwork || root.entries.length === 0) {
            view.releaseWindow("tracks")
            return
        }
        var first = list.indexAt(4, list.contentY + 1)
        var last = list.indexAt(4, list.contentY + Math.max(1, list.height) - 1)
        if (first < 0) first = 0
        if (last < 0) last = Math.min(root.entries.length - 1, first + 12)
        // Entry band -> row band (group headers carry no cover).
        var lo = -1
        var hi = -1
        for (var i = first; i <= last; i++) {
            var n = root.rowIndex[i]
            if (n === undefined || n < 0) continue
            if (lo < 0) lo = n
            hi = n
        }
        if (lo < 0) { lo = 0; hi = Math.min(view.tracksVisible.length - 1, 12) }
        view.queueWindowReport(view.tracksVisible, Math.max(0, lo - 4), hi + 4, "tracks")
    }

    /// Report now, then again once the ListView has laid out (`indexAt`
    /// answers -1 until then — the state a just-shown list is in).
    Timer {
        id: reportSettle
        interval: 50
        repeat: false
        onTriggered: root.report()
    }
    function reportSoon() {
        report()
        reportSettle.restart()
    }
}
