// Library view — QML port of crates/qbz-ui/ui/favorites/FavoritesView.slint
// + the Library "All" mixed feed (library_all.rs semantics).
//
// Data: QbzBridge.libraryJson (ONE JSON document — the full merged feed;
// tabs/search/sort/source-filters derive HERE in JS, measured per the
// phase brief) + libraryCountsJson (tab badges). Artwork is id-keyed
// through the libraryArtworkReady signal into `artMap` (never a
// wrong-cover race); windows are reported as artKeys via
// libraryArtworkWindow, and artMap entries far outside the viewport are
// pruned (the Slint eviction policy, QML-side).
//
// POC-NOTEs:
// - Genre filter, multi-select, group modes, alpha jumps, play-all /
//  shuffle bulk actions, playlists-random, artists sidepanel, albums LIST
//  mode, per-tab persist of view/sort state: out of scope (stubs or
//  omitted; the All grid+list is the measured 1:1 focus).
// - Offline: the generic OfflinePlaceholder replica mounts (the Slint
//  offline RAIL of playable cached favorites needs the offline cache —
//  not wired).
// - Sort/search/source state is session-local (no ui_prefs persistence).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz

Rectangle {
    id: root
    color: theme.surfaceMain

    QbzTheme { id: theme }

    // ============================ state ==================================
    property string activeTab: "all"

    readonly property var counts: JSON.parse(QbzBridge.libraryCountsJson)
    // Full merged feed (parsed once per publish — timed for the report).
    readonly property var feed: parseFeed(QbzBridge.libraryJson)
    function parseFeed(json) {
        var t = Date.now()
        var f = JSON.parse(json)
        console.log("[qbz-qt][perf] QML JSON.parse feed: " + (Date.now() - t)
                    + "ms (" + f.length + " items, " + json.length + " bytes)")
        return f
    }

    // Decoded-cover map {artKey: file://path}, fed by the signal.
    property var artMap: ({})

    // All-tab derive state (LibraryAllState semantics).
    property string search: ""
    property string sortBy: "date"      // "date" | "title" | "artist"
    property bool sortAsc: false        // date: false = newest first
    property bool showPurchases: true
    property bool showFavorites: true
    property bool showFollowing: true
    property bool showLocal: true
    property string viewMode: "grid"    // "grid" | "list"

    // Other-tab state.
    property string tabSearch: ""
    property string albumsSort: "default" // default|title-asc|title-desc|artist-asc
    property string playlistsSubTab: "favorites"

    // ------------------------- derive (JS) ------------------------------
    function visibleItems() {
        var items = []
        var i
        if (activeTab === "all") {
            var needle = search.toLowerCase()
            var anyGroup = showPurchases || showFavorites || showFollowing
            for (i = 0; i < feed.length; i++) {
                var it = feed[i]
                var isLocal = it.source === "local" || it.source === "plex"
                if (isLocal) {
                    if (!showLocal) continue
                } else if (anyGroup) {
                    var g = it.group
                    var ok = (g === "purchases" && showPurchases)
                        || (g === "favorites" && showFavorites)
                        || (g === "following" && showFollowing)
                    if (!ok) continue
                }
                if (needle !== ""
                    && it.title.toLowerCase().indexOf(needle) < 0
                    && it.artist.toLowerCase().indexOf(needle) < 0) continue
                items.push(it)
            }
            // Canonical ascending order per field, then reverse for the
            // other direction; "date" keeps model order (newest-first),
            // reversed for oldest (library_all.rs derive).
            if (sortBy === "title") {
                items.sort(function (a, b) { return a.title.toLowerCase() < b.title.toLowerCase() ? -1 : 1 })
                if (!sortAsc) items.reverse()
            } else if (sortBy === "artist") {
                items.sort(function (a, b) { return a.artist.toLowerCase() < b.artist.toLowerCase() ? -1 : 1 })
                if (!sortAsc) items.reverse()
            } else if (sortAsc) {
                items = items.slice().reverse()
            }
            return items
        }
        var tabNeedle = tabSearch.toLowerCase()
        function hit(it) {
            return tabNeedle === ""
                || it.title.toLowerCase().indexOf(tabNeedle) >= 0
                || it.artist.toLowerCase().indexOf(tabNeedle) >= 0
        }
        for (i = 0; i < feed.length; i++) {
            var x = feed[i]
            var keep = false
            if (activeTab === "tracks") keep = x.kind === "track" && x.group === "favorites"
            else if (activeTab === "albums") keep = x.kind === "album" && x.group === "favorites"
            else if (activeTab === "artists") keep = x.kind === "artist"
            else if (activeTab === "labels") keep = x.kind === "label"
            else if (activeTab === "playlists") keep = x.kind === "playlist"
                && (playlistsSubTab === "following" ? x.group === "following" : x.group === "favorites")
            if (keep && hit(x)) items.push(x)
        }
        if (activeTab === "albums" && albumsSort !== "default") {
            var field = albumsSort.indexOf("artist") === 0 ? "artist" : "title"
            items.sort(function (a, b) {
                var av = field === "artist" ? a.artist.toLowerCase() : a.title.toLowerCase()
                var bv = field === "artist" ? b.artist.toLowerCase() : b.title.toLowerCase()
                return av < bv ? -1 : 1
            })
            if (albumsSort.slice(-4) === "desc") items.reverse()
        }
        return items
    }

    // --------------------- windowed artwork -----------------------------
    // Report the mounted window as artKeys; prune far-away covers.
    function reportWindow(visibleArray, first, last) {
        if (visibleArray.length === 0) return
        last = Math.min(last, visibleArray.length - 1)
        if (first > last) return
        var keys = []
        var keep = {}
        var span = last - first + 1
        var keepLo = Math.max(0, first - span)
        var keepHi = Math.min(visibleArray.length - 1, last + span)
        for (var i = keepLo; i <= keepHi; i++) keep[visibleArray[i].artKey] = true
        for (i = first; i <= last; i++) {
            var k = visibleArray[i].artKey
            if (visibleArray[i].imageUrl !== "") keys.push(k)
        }
        var m = artMap
        var changed = false
        for (var key in m) {
            if (!keep[key]) { delete m[key]; changed = true }
        }
        if (changed) artMap = Object.assign({}, m)
        QbzBridge.libraryArtworkWindow(JSON.stringify(keys))
    }

    Connections {
        target: QbzBridge
        function onLibraryArtworkReady(key, path) {
            var m = root.artMap
            m[key] = path
            // Rebind requires a NEW object reference (same-ref assignment
            // is not a change in QML).
            root.artMap = Object.assign({}, m)
        }
        function onLibraryFavoriteChanged(key, value) {
            var f = root.feed
            for (var i = 0; i < f.length; i++) {
                if (f[i].artKey === key) { f[i].isFavorite = value; break }
            }
            root.feedChanged()
        }
    }

    // Debounced window reporting (180ms, library_all.rs throttle).
    Timer {
        id: windowDebounce
        interval: 180
        onTriggered: root.reportWindow(root.visibleItems(), pendingFirst, pendingLast)
        property int pendingFirst: 0
        property int pendingLast: 0
    }
    function queueWindowReport(first, last) {
        windowDebounce.pendingFirst = first
        windowDebounce.pendingLast = last
        windowDebounce.restart()
    }

    // ============================ components =============================

    // Segmented tab (SegmentedTabBar's Segment) with count badge.
    component SegTab: Rectangle {
        id: segTabRoot
        property string label: ""
        property int count: 0
        property bool active: false
        signal clicked()

        width: segRow.implicitWidth
        height: segRow.implicitHeight
        radius: 4
        color: active ? theme.surfaceMain
             : segArea.containsMouse ? theme.surfaceHover : "transparent"

        Row {
            id: segRow
            leftPadding: 12
            rightPadding: parent && parent.count > 0 ? 8 : 12
            topPadding: 6
            bottomPadding: 6
            spacing: 7
            Text {
                text: parent.parent.label
                color: parent.parent.active ? theme.textPrimary : theme.textMuted
                font.pixelSize: theme.fontLegal
                font.weight: theme.weightMedium
                anchors.verticalCenter: parent.verticalCenter
            }
            Rectangle {
                visible: parent.parent.count > 0
                width: Math.max(18, countText.implicitWidth + 10)
                height: 16
                radius: 8
                anchors.verticalCenter: parent.verticalCenter
                color: parent.parent.active ? "#26ffffff" : "#14ffffff"
                Text {
                    id: countText
                    anchors.centerIn: parent
                    text: segTabRoot.count
                    color: segTabRoot.active ? theme.textPrimary : theme.textSecondary
                    font.pixelSize: 11
                    font.weight: theme.weightMedium
                }
            }
        }
        // Active underline (redundant shape cue).
        Rectangle {
            visible: active
            x: 4
            width: parent.width - 8
            height: 2
            y: parent.height - 2
            radius: 1
            color: theme.accent
        }
        MouseArea {
            id: segArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // Small toolbar toggle button (ToggleButton sm): 30px, active = accent.
    component ToolToggle: Rectangle {
        property string name: ""
        property bool active: false
        signal clicked()
        width: 30
        height: 30
        radius: 6
        color: active ? theme.surfaceElevated
             : ttArea.containsMouse ? theme.surfaceHover : "transparent"
        QbzIcon {
            name: parent.name
            width: 16
            height: 16
            anchors.centerIn: parent
            tintName: parent.active ? "accent"
                   : ttArea.containsMouse ? "primary" : "secondary"
        }
        MouseArea {
            id: ttArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // Toolbar search box (ExpandableSearch sm replica, fixed width).
    component SearchBox: Rectangle {
        property string placeholder: ""
        property alias text: edit.text
        signal searchEdited()
        width: 220
        height: 30
        radius: 6
        color: theme.surfaceElevated
        border.width: 1
        border.color: theme.borderSubtle
        Row {
            anchors.fill: parent
            anchors.leftMargin: 9
            anchors.rightMargin: 9
            spacing: 6
            QbzIcon {
                name: "search"
                width: 13
                height: 13
                anchors.verticalCenter: parent.verticalCenter
                tintName: "muted"
            }
            TextInput {
                id: edit
                width: parent.width - 19
                height: parent.height
                color: theme.textPrimary
                font.pixelSize: 13
                verticalAlignment: Text.AlignVCenter
                clip: true
                onTextEdited: searchEdited()
                Text {
                    visible: edit.text === ""
                    anchors.fill: parent
                    text: placeholder
                    color: theme.textMuted
                    font.pixelSize: 13
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }
    }

    // Card heart (live favorite toggle).
    component HeartBtn: Rectangle {
        property bool fav: false
        property string itemKind: ""
        property string itemId: ""
        width: 28
        height: 28
        radius: 14
        color: hbArea.containsMouse ? "#3dffffff" : "#24ffffff"
        border.width: 1.5
        border.color: "#ccffffff"
        QbzIcon {
            name: parent.fav ? "heart-filled" : "heart"
            width: 14
            height: 14
            anchors.centerIn: parent
            tintName: parent.fav ? "favorite" : "primary"
        }
        MouseArea {
            id: hbArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                // Optimistic flip; the signal confirms or rolls back.
                parent.fav = !parent.fav
                QbzBridge.libraryToggleFavorite(itemKind, itemId)
            }
        }
    }

    // Quality mini-badge (hi-res image / cd box).
    component QualityMini: Item {
        property string tier: ""
        visible: tier !== ""
        width: tier === "hires" ? 42 : 30
        height: 30
        Image {
            visible: tier === "hires"
            source: "assets/hi-res.svg"
            width: 42
            height: 28
            anchors.centerIn: parent
            sourceSize: Qt.size(84, 56)
            fillMode: Image.PreserveAspectFit
        }
        Rectangle {
            visible: tier === "cd"
            width: 30
            height: 30
            radius: 3
            color: theme.surfaceElevated
            border.width: 1
            border.color: theme.borderSubtle
            QbzIcon { name: "cd"; width: 16; height: 16; anchors.centerIn: parent; tintName: "muted" }
        }
    }

    // ===================== feed cards (inline components) ================

    // GRID card — one per kind, 200x246 (AlbumCard metrics).
    component FeedGridCard: Rectangle {
        property var item: ({})
        color: "transparent"

        readonly property bool overlayOn: artArea.containsMouse

        Column {
            spacing: 0
            // --- Artwork -------------------------------------------------
            Rectangle {
                width: 200
                height: 200
                radius: theme.radiusSm
                color: theme.surfaceElevated
                clip: true

                Image {
                    anchors.fill: parent
                    source: root.artMap[item.artKey] || ""
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
                // Artist cards are circular in Slint (ArtistGridCard).
                Rectangle {
                    visible: item.kind === "artist"
                    anchors.fill: parent
                    radius: 100
                    color: "transparent"
                    border.width: 0
                }
                // Hover scrim + play overlay (albums + tracks).
                Rectangle {
                    anchors.fill: parent
                    color: "#000000"
                    opacity: overlayOn && (item.kind === "album" || item.kind === "track") ? 0.5 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                Rectangle {
                    visible: item.kind === "album" || item.kind === "track"
                    anchors.centerIn: parent
                    width: 44
                    height: 44
                    radius: 22
                    opacity: overlayOn ? 1.0 : 0.0
                    color: playArea.containsMouse ? "#d6ffffff" : "#ffffff"
                    QbzIcon { name: "play-fill"; width: 18; height: 18; anchors.centerIn: parent; tintName: "black" }
                    MouseArea {
                        id: playArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: item.kind === "album" ? QbzBridge.playAlbum(item.id) : QbzBridge.playTrack(item.id)
                    }
                }
                // Heart (live).
                HeartBtn {
                    x: parent.width - width - 8
                    y: 8
                    fav: item.isFavorite
                    itemKind: item.kind
                    itemId: item.id
                    opacity: (overlayOn || item.isFavorite) ? 1.0 : 0.0
                }
                // Source badge (show-local): bottom-right hard-drive glyph.
                Rectangle {
                    visible: root.showLocal && (item.source === "local" || item.source === "plex")
                    x: parent.width - width - 6
                    y: parent.height - height - 6
                    width: 24
                    height: 24
                    radius: 4
                    color: "#b3000000"
                    QbzIcon {
                        name: "hard-drive"
                        width: 14
                        height: 14
                        anchors.centerIn: parent
                        tintName: item.source === "plex" ? "accent" : "primary"
                    }
                }
                MouseArea {
                    id: artArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (item.kind === "album") QbzBridge.playAlbum(item.id)
                        else if (item.kind === "track") QbzBridge.playTrack(item.id)
                        // artist/playlist/label pages: out of scope (POC-NOTE).
                    }
                }
            }
            Item { width: 1; height: 6 }
            // --- Meta ----------------------------------------------------
            Row {
                width: 200
                spacing: theme.spacingSm
                Column {
                    width: parent.width - (qBadge.visible ? qBadge.width + theme.spacingSm : 0)
                    spacing: 2
                    Text {
                        width: parent.width
                        height: 20
                        text: item.title
                        color: theme.textPrimary
                        font.pixelSize: theme.fontBody - 2
                        font.weight: theme.weightMedium
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                    Text {
                        width: parent.width
                        height: 18
                        text: item.kind === "track" ? item.artist
                            : item.subtitle !== "" ? item.subtitle : item.artist
                        color: theme.textMuted
                        font.pixelSize: theme.fontLink - 1
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
                QualityMini { tier: item.qualityTier; id: qBadge; anchors.verticalCenter: parent.verticalCenter }
            }
        }
    }

    // LIST row — 44px (QueueRow-like; FeedListRow approximation).
    component FeedListRow: Rectangle {
        property var item: ({})
        height: 44
        radius: theme.radiusSm
        color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

        // Declared BEFORE the content so the heart button keeps its clicks.
        MouseArea {
            id: rowArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                if (item.kind === "album") QbzBridge.playAlbum(item.id)
                else if (item.kind === "track") QbzBridge.playTrack(item.id)
            }
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: theme.spacingSm
            anchors.rightMargin: theme.spacingXs
            anchors.topMargin: 4
            anchors.bottomMargin: 4
            spacing: 9
            Rectangle {
                width: 34
                height: 34
                radius: 4
                anchors.verticalCenter: parent.verticalCenter
                color: theme.surfaceElevated
                clip: true
                Image {
                    anchors.fill: parent
                    source: root.artMap[item.artKey] || ""
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
            }
            Column {
                width: parent.width - 34 - heartBtn.width
                    - (qMini.visible ? qMini.width + 9 : 0)
                    - (durText.visible ? durText.implicitWidth + 9 : 0) - 9 * 2
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    text: item.title
                    color: theme.textPrimary
                    font.pixelSize: 12
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    text: item.subtitle !== "" ? item.subtitle : item.artist
                    color: theme.textMuted
                    font.pixelSize: 10
                    elide: Text.ElideRight
                }
            }
            QualityMini { id: qMini; tier: item.qualityTier; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: durText
                visible: item.duration !== ""
                anchors.verticalCenter: parent.verticalCenter
                text: item.duration
                color: theme.textMuted
                font.pixelSize: 11
            }
            Rectangle {
                id: heartBtn
                width: 28
                height: 28
                radius: 6
                anchors.verticalCenter: parent.verticalCenter
                color: lhArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    name: item.isFavorite ? "heart-filled" : "heart"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: item.isFavorite ? "favorite" : "muted"
                }
                MouseArea {
                    id: lhArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.libraryToggleFavorite(item.kind, item.id)
                }
            }
        }
    }

    // ============================ view ===================================

    // Offline gate (OfflinePlaceholder replica; the Slint offline RAIL is
    // out of scope — see header note).
    Column {
        visible: QbzBridge.offline
        anchors.centerIn: parent
        spacing: 0
        QbzIcon {
            name: "cloud-off"
            width: 56
            height: 56
            anchors.horizontalCenter: parent.horizontalCenter
            tintName: "muted"
        }
        Item { width: 1; height: 18 }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: QbzBridge.tr("You're offline")
            color: theme.textPrimary
            font.pixelSize: theme.fontHeading
            font.weight: theme.weightSemibold
        }
        Item { width: 1; height: 8 }
        Text {
            width: 420
            text: QbzBridge.offlineMode === 2
                ? QbzBridge.tr("Offline mode is enabled. Disable it in Settings to use Qobuz.")
                : QbzBridge.tr("No internet connection. Your local library and downloads keep working.")
            color: theme.textSecondary
            font.pixelSize: theme.fontBody
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.WordWrap
        }
    }

    Column {
        anchors.fill: parent
        spacing: 0
        visible: !QbzBridge.offline

        // --- Toolbar: tab menu (left) + per-tab controls (right) ---------
        Item {
            width: parent.width
            height: 56

            // FavTabMenu (SegmentedTabBar with count badges).
            Rectangle {
                x: 32
                y: 25 - height / 2
                width: tabRow.width
                height: tabRow.height
                color: theme.surfaceElevated
                radius: 6
                Row {
                    id: tabRow
                    padding: 3
                    spacing: 4
                    SegTab { label: QbzBridge.tr("All"); count: counts.all || 0; active: root.activeTab === "all"; onClicked: root.activeTab = "all" }
                    SegTab { label: QbzBridge.tr("Tracks"); count: counts.tracks || 0; active: root.activeTab === "tracks"; onClicked: root.activeTab = "tracks" }
                    SegTab { label: QbzBridge.tr("Albums"); count: counts.albums || 0; active: root.activeTab === "albums"; onClicked: root.activeTab = "albums" }
                    SegTab { label: QbzBridge.tr("Artists"); count: counts.artists || 0; active: root.activeTab === "artists"; onClicked: root.activeTab = "artists" }
                    SegTab { label: QbzBridge.tr("Playlists"); count: counts.playlists || 0; active: root.activeTab === "playlists"; onClicked: root.activeTab = "playlists" }
                    SegTab { label: QbzBridge.tr("Labels"); count: counts.labels || 0; active: root.activeTab === "labels"; onClicked: root.activeTab = "labels" }
                }
            }

            // Per-tab controls.
            Row {
                x: parent.width - width - 32
                y: 25 - height / 2
                height: 30
                spacing: 8

                // ===== All toolbar =====
                Row {
                    visible: root.activeTab === "all"
                    spacing: 8
                    height: parent.height
                    SearchBox {
                        placeholder: QbzBridge.tr("Search your library")
                        onSearchEdited: {
                            root.search = text
                            allSearchDebounce.restart()
                        }
                        Timer {
                            id: allSearchDebounce
                            interval: 250
                            onTriggered: root.queueWindowReport(0, 59)
                        }
                    }
                    // Source switches (tooltips are the Slint copies).
                    ToolToggle { name: "shopping-bag"; active: root.showPurchases; onClicked: root.showPurchases = !root.showPurchases }
                    ToolToggle { name: "heart"; active: root.showFavorites; onClicked: root.showFavorites = !root.showFavorites }
                    ToolToggle { name: "user-plus"; active: root.showFollowing; onClicked: root.showFollowing = !root.showFollowing }
                    ToolToggle { name: "hard-drive"; active: root.showLocal; onClicked: root.showLocal = !root.showLocal }
                    // Genre filter — INERT stub (out of scope).
                    ToolToggle { name: "list-filter"; active: false }
                    // Grid / list toggle.
                    ToolToggle {
                        name: root.viewMode === "list" ? "layout-grid" : "list"
                        active: false
                        onClicked: root.viewMode = root.viewMode === "list" ? "grid" : "list"
                    }
                    // Sort menu (PlaylistView-style: field + direction caret).
                    Rectangle {
                        width: allSortRow.width
                        height: 30
                        radius: 6
                        color: allSortArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                        Row {
                            id: allSortRow
                            height: parent.height
                            leftPadding: 10
                            rightPadding: 10
                            spacing: 6
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: QbzBridge.tr("Sort") + ": " + (
                                    root.sortBy === "title" ? QbzBridge.tr("Title")
                                    : root.sortBy === "artist" ? QbzBridge.tr("Artist")
                                    : QbzBridge.tr("Date added"))
                                color: theme.textSecondary
                                font.pixelSize: 12
                            }
                            QbzIcon {
                                name: root.sortAsc ? "chevron-up" : "chevron-down"
                                width: 12
                                height: 12
                                anchors.verticalCenter: parent.verticalCenter
                                tintName: "secondary"
                            }
                        }
                        MouseArea {
                            id: allSortArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: allSortMenu.open()
                        }
                        Popup {
                            id: allSortMenu
                            x: parent.width - 172
                            y: parent.height + 4
                            width: 172
                            padding: 5
                            closePolicy: Popup.CloseOnPressOutside
                            background: Rectangle {
                                color: theme.surfaceMain
                                radius: theme.radiusSm
                                border.width: 1
                                border.color: theme.borderMuted
                            }
                            contentItem: Column {
                                Repeater {
                                    model: [
                                        { "field": "date", "label": QbzBridge.tr("Date added") },
                                        { "field": "title", "label": QbzBridge.tr("Title") },
                                        { "field": "artist", "label": QbzBridge.tr("Artist") },
                                    ]
                                    delegate: Rectangle {
                                        required property var modelData
                                        width: parent ? parent.width : 0
                                        height: 33
                                        radius: 5
                                        color: sortOptArea.containsMouse ? theme.surfaceHover : "transparent"
                                        Row {
                                            anchors.fill: parent
                                            anchors.leftMargin: 8
                                            spacing: 6
                                            Text {
                                                width: parent.width - 26
                                                height: parent.height
                                                text: modelData.label
                                                color: theme.textSecondary
                                                font.pixelSize: 13
                                                font.weight: root.sortBy === modelData.field ? theme.weightSemibold : theme.weightRegular
                                                verticalAlignment: Text.AlignVCenter
                                            }
                                            QbzIcon {
                                                visible: root.sortBy === modelData.field
                                                name: root.sortAsc ? "chevron-up" : "chevron-down"
                                                width: 12
                                                height: 12
                                                anchors.verticalCenter: parent.verticalCenter
                                                tintName: "accent"
                                            }
                                        }
                                        MouseArea {
                                            id: sortOptArea
                                            anchors.fill: parent
                                            hoverEnabled: true
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: {
                                                // Re-pick flips direction; a
                                                // new field resets to its
                                                // natural default.
                                                if (root.sortBy === modelData.field) {
                                                    root.sortAsc = !root.sortAsc
                                                } else {
                                                    root.sortBy = modelData.field
                                                    root.sortAsc = modelData.field !== "date"
                                                }
                                                allSortMenu.close()
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ===== Other-tab toolbars =====
                SearchBox {
                    visible: root.activeTab !== "all"
                    placeholder: QbzBridge.tr("Search")
                    onSearchEdited: root.tabSearch = text
                }
                // Albums sort popup (real, JS-side).
                Rectangle {
                    visible: root.activeTab === "albums"
                    width: albumsSortRow.width
                    height: 30
                    radius: 6
                    color: albumsSortArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                    Row {
                        id: albumsSortRow
                        height: parent.height
                        leftPadding: 10
                        rightPadding: 10
                        spacing: 6
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: QbzBridge.tr("Sort") + ": " + root.albumsSort
                            color: theme.textSecondary
                            font.pixelSize: 12
                        }
                        QbzIcon { name: "chevron-down"; width: 12; height: 12; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                    }
                    MouseArea {
                        id: albumsSortArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: albumsSortMenu.open()
                    }
                    Popup {
                        id: albumsSortMenu
                        x: parent.width - 172
                        y: parent.height + 4
                        width: 172
                        padding: 5
                        closePolicy: Popup.CloseOnPressOutside
                        background: Rectangle {
                            color: theme.surfaceMain
                            radius: theme.radiusSm
                            border.width: 1
                            border.color: theme.borderMuted
                        }
                        contentItem: Column {
                            Repeater {
                                model: ["default", "title-asc", "title-desc", "artist-asc"]
                                delegate: Rectangle {
                                    required property string modelData
                                    width: parent ? parent.width : 0
                                    height: 33
                                    radius: 5
                                    color: abSortOptArea.containsMouse ? theme.surfaceHover : "transparent"
                                    Text {
                                        anchors.fill: parent
                                        anchors.leftMargin: 8
                                        text: modelData
                                        color: theme.textSecondary
                                        font.pixelSize: 13
                                        font.weight: root.albumsSort === modelData ? theme.weightSemibold : theme.weightRegular
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                    MouseArea {
                                        id: abSortOptArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: { root.albumsSort = modelData; albumsSortMenu.close() }
                                    }
                                }
                            }
                        }
                    }
                }
                // Playlists sub-tab (Library / Following).
                Rectangle {
                    visible: root.activeTab === "playlists"
                    width: plSubRow.width
                    height: 30
                    radius: 6
                    color: theme.surfaceElevated
                    Row {
                        id: plSubRow
                        padding: 3
                        spacing: 4
                        Repeater {
                            model: [
                                { "id": "favorites", "label": QbzBridge.tr("Library") },
                                { "id": "following", "label": QbzBridge.tr("Following") },
                            ]
                            delegate: Rectangle {
                                required property var modelData
                                property bool active: root.playlistsSubTab === modelData.id
                                width: plSubText.implicitWidth + 20
                                height: 24
                                radius: 4
                                color: active ? theme.surfaceMain : "transparent"
                                Text {
                                    id: plSubText
                                    anchors.centerIn: parent
                                    text: modelData.label
                                    color: parent.active ? theme.textPrimary : theme.textMuted
                                    font.pixelSize: 12
                                    font.weight: theme.weightMedium
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.playlistsSubTab = modelData.id
                                }
                            }
                        }
                    }
                }
            }
        }
        Rectangle {
            width: parent.width
            height: 1
            color: theme.borderSubtle
        }

        // ===================== SCROLLING CONTENT =========================
        Item {
            width: parent.width
            height: parent.height - 57
            clip: true

            // Loading (LoadingSpinner.slint: accent arc, 1s spin).
            QbzSpinner {
                anchors.centerIn: parent
                size: 36
                visible: QbzBridge.libraryLoading
            }

            // Error + retry.
            Column {
                visible: !QbzBridge.libraryLoading && QbzBridge.libraryError !== ""
                anchors.centerIn: parent
                spacing: 10
                Text {
                    text: QbzBridge.tr("Couldn't load your Library.")
                    color: theme.textPrimary
                    font.pixelSize: theme.fontBody
                    font.weight: theme.weightSemibold
                }
                Text {
                    text: QbzBridge.libraryError
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                }
                Rectangle {
                    width: retryText.implicitWidth + 28
                    height: 32
                    radius: 6
                    anchors.horizontalCenter: parent.horizontalCenter
                    color: retryArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                    border.width: 1
                    border.color: theme.borderSubtle
                    Text {
                        id: retryText
                        anchors.centerIn: parent
                        text: QbzBridge.tr("Retry")
                        color: theme.textPrimary
                        font.pixelSize: theme.fontLegal
                    }
                    MouseArea {
                        id: retryArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.reloadLibrary()
                    }
                }
            }

            // ============ GRID (all tabs; All in grid mode) ==============
            GridView {
                id: grid
                anchors.fill: parent
                anchors.leftMargin: 32
                anchors.rightMargin: 32
                anchors.topMargin: 16
                visible: !QbzBridge.libraryLoading && QbzBridge.libraryError === ""
                    && (root.activeTab !== "all" || root.viewMode === "grid")
                    && root.activeTab !== "tracks"
                cellWidth: 220
                cellHeight: 266
                cacheBuffer: 266 * 2
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.visibleItems()

                onContentYChanged: root.gridWindowReport()
                onModelChanged: root.gridWindowReport()
                onWidthChanged: root.gridWindowReport()
                Component.onCompleted: root.gridWindowReport()

                delegate: FeedGridCard {
                    width: 200
                    height: 246
                    item: modelData
                }
            }

            // ============ LIST (All list mode + Tracks tab) ==============
            ListView {
                id: list
                anchors.fill: parent
                anchors.leftMargin: 32
                anchors.rightMargin: 32
                anchors.topMargin: 10
                visible: !QbzBridge.libraryLoading && QbzBridge.libraryError === ""
                    && ((root.activeTab === "all" && root.viewMode === "list")
                        || root.activeTab === "tracks")
                cacheBuffer: 44 * 10
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: root.visibleItems()
                onContentYChanged: root.listWindowReport()
                onModelChanged: root.listWindowReport()
                Component.onCompleted: root.listWindowReport()

                delegate: FeedListRow {
                    width: list.width
                    item: modelData
                }
            }

            // Thin auto-hiding scrollbars (ListScrollbar replica).
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: grid
                visible: grid.visible && grid.contentHeight > grid.height
            }
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: list
                visible: list.visible && list.contentHeight > list.height
            }
        }
    }

    function gridWindowReport() {
        var cols = Math.max(1, Math.floor(grid.width / grid.cellWidth))
        var firstRow = Math.max(0, Math.floor(grid.contentY / grid.cellHeight) - 1)
        var lastRow = Math.ceil((grid.contentY + grid.height) / grid.cellHeight) + 1
        var m = grid.model
        queueWindowReport(firstRow * cols, Math.min(m.length - 1, lastRow * cols - 1))
    }
    function listWindowReport() {
        var first = Math.max(0, Math.floor(list.contentY / 44) - 4)
        var last = Math.ceil((list.contentY + list.height) / 44) + 4
        queueWindowReport(first, Math.min(list.model.length - 1, last))
    }
}
