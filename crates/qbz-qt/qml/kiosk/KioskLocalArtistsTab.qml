// Local Library > Artists, kiosk body — reported DEAD in the release audit.
//
// Two causes, both fixed here:
//   1. the old kiosk read `QbzLocal.localArtistsJson`, which the LEGACY reader
//      publishes and which stops being republished the moment the paged
//      `QbzLocalArtists` surface goes active (the production default). A live
//      library rendered an empty avatar wall;
//   2. it mounted a `Repeater` over EVERY artist with a per-slot Loader —
//      20 487 items and 10 075 Loaders at the 10 000-artist fixture, 1.87 s to
//      idle (K0 runtime baseline). Nothing about that is virtualised.
//
// Now: ONE windowing ListView over whichever reader is authoritative, with a
// 72px touch row per artist. A LIST rather than a grid because the native
// model's rows ARE one artist each (plus A-Z letter bands) — chunking a paged
// model into grid rows would mean materialising it, which is the whole thing
// the model exists to avoid.
//
// SELECTION IS A DRILL-DOWN, not a split. The desktop pane is a 280px rail
// beside an album grid; at 800×480 that leaves 520px for the grid, i.e. two
// cards. Tapping an artist replaces the list with that artist's albums and a
// back chip. Selecting an artist records NO history entry — same as Full UI,
// where `selectedArtist` is opaque view state restored with the entry, never a
// destination of its own.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    function t(s) { return QbzSession.tr(s, QbzSession.trRev) }

    readonly property string nativeError: root.nativeActive ? QbzLocal.localArtistsNativeError : ""
    readonly property bool nativeActive: QbzLocal.localArtistsNativeActive
    readonly property var nativeModel: QbzLocalArtists
    readonly property var nativeAlbumModel: QbzLocalArtistAlbums
    readonly property int artistTotal: root.nativeActive
        ? (QbzLocal.localArtistsNativeTotal || 0)
        : (root.view ? root.view.artists.length : 0)

    readonly property string selectedArtist: root.view ? root.view.selectedArtist : ""
    readonly property bool drilled: root.selectedArtist !== ""

    readonly property real rowH: 72
    readonly property real headerH: 26

    // ---------------------------------------------------------------------
    // Readers
    // ---------------------------------------------------------------------
    // Legacy entries mirror the native row shape ({t, item}) so ONE delegate
    // serves both and the two paths cannot drift apart visually.
    readonly property var legacyEntries: {
        if (root.nativeActive || !root.view)
            return []
        var rows = root.view.artists
        var out = []
        for (var i = 0; i < rows.length; i++)
            out.push({ "t": 1, "loading": false, "item": rows[i] })
        return out
    }
    readonly property int entryCount: root.nativeActive
        ? (root.nativeModel.totalCount || 0) : root.legacyEntries.length

    /// The selected artist's albums on the LEGACY path. The match itself lives
    /// in Rust (`artistAlbumIds` -> local_artist_match), because it is the same
    /// normalised-name rule the Artists merge uses and there must be one of it.
    readonly property var legacyArtistAlbums: {
        if (root.nativeActive || !root.drilled || !root.view)
            return []
        var ids = ({})
        try {
            var parsed = JSON.parse(QbzLocal.artistAlbumIds(root.selectedArtist))
            for (var j = 0; j < parsed.length; j++)
                ids[parsed[j]] = true
        } catch (e) {
            return []
        }
        var rows = root.view.albums
        var out = []
        for (var i = 0; i < rows.length; i++)
            if (ids[rows[i].id])
                out.push(rows[i])
        return out
    }
    readonly property int artistAlbumTotal: root.nativeActive
        ? (QbzLocal.localArtistAlbumsNativeTotal || 0) : root.legacyArtistAlbums.length

    // ---------------------------------------------------------------------
    // Native queries
    // ---------------------------------------------------------------------
    // `artists_native_reset` REFUSES a non-default sort or funnel, so the
    // kiosk — which exposes neither control — passes the neutral descriptor.
    Timer {
        id: railQuery
        interval: 0
        repeat: false
        onTriggered: QbzLocal.artistsNativeReset("", "name-asc", "{}")
    }
    Timer {
        id: detailQuery
        interval: 0
        repeat: false
        onTriggered: {
            if (!root.nativeActive || !root.drilled || albumGrid.columns <= 0)
                return
            QbzLocal.artistsNativeSelect(root.selectedArtist, albumGrid.columns)
        }
    }
    Component.onCompleted: {
        railQuery.restart()
        if (root.drilled)
            detailQuery.restart()
        root.publishNav()
    }
    onSelectedArtistChanged: {
        if (root.drilled)
            detailQuery.restart()
        root.publishNav()
        Qt.callLater(root.reportSoon)
    }
    Connections {
        target: albumGrid
        function onColumnsChanged() {
            if (root.drilled)
                detailQuery.restart()
            root.publishNav()
        }
    }

    // The focus geometry follows the drill-down: the list is one column over
    // the model's rows, the detail is the album grid's own pitch.
    function publishNav() {
        if (!root.view)
            return
        if (root.drilled)
            root.view.publishNav(Math.max(1, albumGrid.columns), root.artistAlbumTotal)
        else
            root.view.publishNav(1, root.entryCount)
    }
    onEntryCountChanged: root.publishNav()
    onArtistAlbumTotalChanged: root.publishNav()
    Connections {
        target: QbzLocal
        function onLocalArtistsNativeActiveChanged() {
            root.reportSoon()
            if (root.drilled)
                detailQuery.restart()
        }
        function onLocalArtistsLoadingChanged() {
            if (!QbzLocal.localArtistsLoading) {
                root.reportSoon()
                if (root.drilled)
                    detailQuery.restart()
            }
        }
        // Resolved covers have to reach the paged rows, which carry their own
        // `artPath`; the id-keyed map alone only serves the legacy reader.
        function onLocalArtworkReady(key, path) {
            root.nativeModel.setArtwork(key, path)
            root.nativeAlbumModel.setArtwork(key, path)
        }
    }
    // Page requests are only emitted while this signal has a receiver, so both
    // subscriptions belong to the mounted tab.
    Connections {
        target: root.nativeModel
        function onDataChanged() { root.reportSoon() }
        function onModelReset() { root.reportSoon() }
        function onPageMiss(page, generation) {
            QbzLocal.artistsNativePageMiss(page, generation)
        }
    }
    Connections {
        target: root.nativeAlbumModel
        function onPageMiss(page, generation) {
            QbzLocal.artistAlbumsNativePageMiss(page, generation)
        }
    }

    // ---------------------------------------------------------------------
    // Focus. The index space is the model's ROWS, letter bands included: a
    // paged model cannot answer "the Nth artist" without materialising the
    // pages in between. A band simply does nothing on Enter.
    // ---------------------------------------------------------------------
    readonly property int focusedItem: QbzKioskNav.index - QbzKioskNav.tabs
    readonly property bool itemFocused: QbzKioskNav.navActive
        && QbzKioskNav.zone === "content"
        && !root.drilled
        && root.focusedItem >= 0
        && root.focusedItem < root.entryCount

    function entryAt(index) {
        if (root.nativeActive)
            return root.nativeModel.rowAt(index)
        return root.legacyEntries[index] || null
    }

    Connections {
        target: QbzKioskNav
        function onIndexChanged() {
            if (root.itemFocused)
                Qt.callLater(function () {
                    rail.positionViewAtIndex(root.focusedItem, ListView.Contain)
                })
        }
        function onActivateSeqChanged() {
            if (!root.itemFocused)
                return
            var entry = root.entryAt(root.focusedItem)
            if (entry && !entry.loading && entry.t === 1 && entry.item)
                root.select(entry.item.name)
        }
    }

    function select(name) {
        if (root.view)
            root.view.selectArtist(name || "")
    }

    // ---------------------------------------------------------------------
    // Artwork window — the rail's mounted band. The drill-down grid reports
    // its own window through its own surface slot.
    // ---------------------------------------------------------------------
    function report() {
        if (!root.view)
            return
        if (!rail.visible || root.entryCount === 0) {
            root.view.releaseWindow("artists")
            return
        }
        var first = rail.indexAt(4, rail.contentY + 1)
        var last = rail.indexAt(4, rail.contentY + Math.max(1, rail.height) - 1)
        if (first < 0)
            first = Math.max(0, Math.floor((rail.contentY - rail.originY) / root.rowH))
        if (last < 0)
            last = Math.min(root.entryCount - 1, first + Math.ceil(rail.height / root.rowH))
        first = Math.max(0, first - 1)
        last = Math.min(root.entryCount - 1, last + 1)

        var resident = []
        for (var i = first; i <= last; i++) {
            var entry = root.entryAt(i)
            if (!entry || entry.loading || entry.t !== 1 || !entry.item)
                continue
            resident.push(entry.item)
        }
        if (resident.length === 0)
            root.view.releaseWindow("artists")
        else
            root.view.queueWindowReport(resident, 0, resident.length - 1, "artists")
    }
    Timer {
        id: reportSettle
        interval: 50
        repeat: false
        onTriggered: root.report()
    }
    function reportSoon() {
        root.report()
        reportSettle.restart()
    }
    Component.onDestruction: if (root.view) root.view.releaseWindow("artists")
    Connections {
        target: root.view
        function onArtworkRefresh() { root.reportSoon() }
    }

    // =====================================================================
    // The artist list
    // =====================================================================
    ListView {
        id: rail
        anchors.fill: parent
        visible: !root.drilled && !QbzLocal.localArtistsLoading
            && root.nativeError === ""
            && root.artistTotal > 0
        clip: true
        topMargin: 8
        bottomMargin: 8
        cacheBuffer: Math.min(root.rowH, Math.max(0, rail.height / 2))
        boundsBehavior: Flickable.StopAtBounds
        reuseItems: true
        model: rail.visible ? (root.nativeActive ? root.nativeModel : root.legacyEntries) : []

        onContentYChanged: root.report()
        onModelChanged: root.report()
        onHeightChanged: root.report()
        onVisibleChanged: {
            if (visible)
                root.reportSoon()
            else if (root.view)
                root.view.releaseWindow("artists")
        }

        delegate: Item {
            id: entrySlot
            required property var modelData
            required property int index

            readonly property bool entryLoading: entrySlot.modelData
                && entrySlot.modelData.loading === true
            readonly property bool band: !entrySlot.entryLoading
                && entrySlot.modelData && entrySlot.modelData.t === 0
            readonly property var item: entrySlot.modelData
                ? entrySlot.modelData.item : null

            width: rail.width
            height: entrySlot.band ? root.headerH : root.rowH

            // A-Z band.
            Text {
                anchors.fill: parent
                anchors.leftMargin: root.view ? root.view.pad : 16
                visible: entrySlot.band
                verticalAlignment: Text.AlignVCenter
                text: entrySlot.band ? entrySlot.modelData.label : ""
                color: theme.textSecondary
                font.pixelSize: theme.fontLegal
                font.weight: theme.weightSemibold
            }

            // Page not resident: a static tile, no avatar request.
            Rectangle {
                anchors.fill: parent
                anchors.margins: 6
                visible: entrySlot.entryLoading
                radius: theme.radiusSm
                color: theme.surfaceElevated
                opacity: 0.55
            }

            // The artist row. 72px, the whole row is the tap target.
            Rectangle {
                id: artistRow
                anchors.fill: parent
                anchors.leftMargin: root.view ? root.view.pad : 16
                anchors.rightMargin: root.view ? root.view.pad : 16
                visible: !entrySlot.band && !entrySlot.entryLoading
                radius: theme.radiusSm
                color: root.itemFocused && root.focusedItem === entrySlot.index
                    ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.18)
                    : "transparent"
                border.width: root.itemFocused && root.focusedItem === entrySlot.index ? 2 : 0
                border.color: theme.accent

                Row {
                    anchors.left: parent.left
                    anchors.leftMargin: 8
                    anchors.right: parent.right
                    anchors.rightMargin: 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 14

                    Rectangle {
                        id: avatar
                        width: 52
                        height: 52
                        radius: 26
                        color: theme.surfaceElevated
                        clip: true

                        KioskArtwork {
                            anchors.fill: parent
                            radius: 26
                            fit: "crop"
                            source: entrySlot.item
                                ? (entrySlot.item.artPath
                                   || (root.view ? root.view.artPathOf(entrySlot.item.artKey) : ""))
                                : ""
                        }
                    }

                    Column {
                        width: parent.width - 52 - 14
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 3

                        Text {
                            width: parent.width
                            text: entrySlot.item ? (entrySlot.item.name || "") : ""
                            color: theme.textPrimary
                            font.pixelSize: 16
                            font.weight: theme.weightSemibold
                            elide: Text.ElideRight
                            maximumLineCount: 1
                        }
                        Text {
                            width: parent.width
                            text: entrySlot.item
                                ? ((entrySlot.item.albumCount || 0) + " "
                                   + root.t("albums"))
                                : ""
                            color: theme.textMuted
                            font.pixelSize: 13
                            elide: Text.ElideRight
                            maximumLineCount: 1
                        }
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: {
                        if (entrySlot.item)
                            root.select(entrySlot.item.name)
                    }
                }
            }
        }
    }

    ScrollMemory { target: rail; scope: "local:artists"; relativeToOrigin: true }

    // =====================================================================
    // The drill-down: one artist's albums
    // =====================================================================
    Item {
        anchors.fill: parent
        visible: root.drilled

        Rectangle {
            id: detailBar
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 64
            color: "transparent"

            Row {
                anchors.left: parent.left
                anchors.leftMargin: root.view ? root.view.pad : 16
                anchors.right: parent.right
                anchors.rightMargin: root.view ? root.view.pad : 16
                anchors.verticalCenter: parent.verticalCenter
                spacing: 12

                // 64px primary touch target — leaving the drill-down is the
                // one action this bar has to make impossible to miss.
                Rectangle {
                    id: backChip
                    width: 64
                    height: 48
                    radius: theme.radiusSm
                    color: theme.surfaceElevated
                    border.width: 1
                    border.color: theme.borderSubtle

                    QbzIcon {
                        anchors.centerIn: parent
                        name: "chevron-left"
                        width: 22
                        height: 22
                        tintName: "textPrimary"
                    }
                    MouseArea {
                        anchors.fill: parent
                        // A generous margin turns the 64x48 chip into a
                        // >64px effective target without moving the visual.
                        anchors.margins: -8
                        onClicked: root.select("")
                    }
                }

                Column {
                    width: parent.width - 64 - 12
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2

                    Text {
                        width: parent.width
                        text: root.selectedArtist
                        color: theme.textPrimary
                        font.pixelSize: 18
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                        maximumLineCount: 1
                    }
                    Text {
                        width: parent.width
                        text: root.artistAlbumTotal + " " + root.t("albums")
                        color: theme.textMuted
                        font.pixelSize: 13
                        elide: Text.ElideRight
                    }
                }
            }
        }

        KioskLocalAlbumGrid {
            id: albumGrid
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: detailBar.bottom
            anchors.bottom: parent.bottom
            visible: root.drilled && !QbzLocal.localArtistAlbumsLoading
                && root.artistAlbumTotal > 0
            view: root.view
            surface: "artist-albums"
            scrollScope: "local:artists:albums"
            pad: root.view ? root.view.pad : 16
            nativeActive: root.nativeActive
            nativeModel: root.nativeAlbumModel
            rows: root.nativeActive ? [] : root.legacyArtistAlbums
            onOpen: function (id) {
                if (root.view)
                    root.view.openAlbum(id)
            }
        }

        KioskSkeleton {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: detailBar.bottom
            anchors.bottom: parent.bottom
            kind: "grid"
            columns: Math.max(2, albumGrid.columns)
            pad: albumGrid.pad
            gap: albumGrid.gap
            loading: root.drilled && QbzLocal.localArtistAlbumsLoading
            empty: root.drilled && !QbzLocal.localArtistAlbumsLoading
                && root.artistAlbumTotal === 0
            emptyText: root.t("No albums found")
        }
    }

    // Rail-level loading / error / empty. Static, mounts no artwork.
    KioskSkeleton {
        anchors.fill: parent
        kind: "list"
        rowHeight: root.rowH
        rowArtSize: 52
        pad: root.view ? root.view.pad : 16
        loading: !root.drilled && QbzLocal.localArtistsLoading
        error: root.drilled ? "" : root.nativeError
        empty: !root.drilled && !QbzLocal.localArtistsLoading
            && root.nativeError === ""
            && root.artistTotal === 0
        emptyText: root.t("No artists in your local library yet.")
    }
}
