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
    // Transparent while the ambient background is active (phase 14 —
    // HomeView.slint:163: the frosted content panel shows through).
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzBridge.ambientMode > 0 && QbzBridge.npHasTrack
       
    // Round to the AppShell content-frame bezel (Radius.md): QML clips
    // are rectangular, so the frame's own rounding never reaches the
    // view — the view's own fill must round instead.
    radius: 12

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
        function onPinChanged(key, value) {
            var f = root.feed
            for (var i = 0; i < f.length; i++) {
                if (f[i].artKey === key) { f[i].isPinned = value; break }
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

    // ===================== card families (Slint homologation) =============
    // Phase 13: every Library surface mounts the SAME card family the Slint
    // mounts (FavoritesView.slint): track -> discover/TrackCard, album ->
    // discover/AlbumCard (the shared qml/AlbumCard.qml), artist ->
    // discover/ArtistGridCard, playlist -> discover/PlaylistCard, label ->
    // discover/LabelCard. The old one-size FeedGridCard is gone.

    // Overlay action button (AlbumCard's OverlayButton: 36px ghost /
    // 44px primary white disc).
    component LibOverlayBtn: Rectangle {
        property string name: ""
        property bool primary: false
        property bool active: false
        signal clicked()
        width: primary ? 44 : 36
        height: primary ? 44 : 36
        radius: width / 2
        color: primary ? (obArea.containsMouse ? "#d6ffffff" : "#ffffff")
             : (obArea.containsMouse ? "#3dffffff" : "#24ffffff")
        border.width: primary ? 0 : 1.5
        border.color: "#ccffffff"
        QbzIcon {
            name: parent.name
            width: primary ? 18 : 16
            height: primary ? 18 : 16
            anchors.centerIn: parent
            tintName: parent.active ? "accent" : (parent.primary ? "black" : "primary")
        }
        property alias hovered: obArea.containsMouse
        MouseArea {
            id: obArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }

    // Shared ⋯ menu surface (33px rows, icon 15 + label 13).
    component CardMenu: QbzContextMenu {
        id: cmRoot
        property var entries: []
        signal picked(string action)
        Repeater {
            model: cmRoot.entries
            delegate: Rectangle {
                required property var modelData
                width: parent ? parent.width : 0
                height: 33
                radius: 5
                color: cmiArea.containsMouse ? theme.surfaceHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    spacing: 8
                    QbzIcon { name: modelData.icon; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                    Text {
                        height: parent.height
                        width: parent.width - 23
                        text: modelData.label
                        color: theme.textSecondary
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
                MouseArea {
                    id: cmiArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: { cmRoot.close(); cmRoot.picked(modelData.action) }
                }
            }
        }
    }

    // Track context-menu model (TrackCard.slint track-menu) + dispatch.
    function trackMenuModel(item) {
        var m = [
            { "label": QbzBridge.tr("Play"), "icon": "play-fill", "action": "play" },
            { "label": QbzBridge.tr("Play next"), "icon": "list-start", "action": "next" },
            { "label": QbzBridge.tr("Play later"), "icon": "list-plus", "action": "later" },
            { "label": QbzBridge.tr("Add to queue"), "icon": "list-end", "action": "queue" },
        ]
        if (item.artistId !== "") m.push({ "label": QbzBridge.tr("Go to artist"), "icon": "user", "action": "go-artist" })
        if (item.albumId !== "") m.push({ "label": QbzBridge.tr("Go to album"), "icon": "disc", "action": "go-album" })
        m.push({ "label": item.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"),
                 "icon": item.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
        return m
    }
    function trackAction(item, a) {
        if (a === "play") QbzBridge.playTrack(item.id)
        else if (a === "next") QbzBridge.enqueueTrack(item.id, "next")
        else if (a === "later") QbzBridge.enqueueTrack(item.id, "later")
        else if (a === "queue") QbzBridge.enqueueTrack(item.id, "queue")
        else if (a === "go-artist") QbzBridge.openArtist(item.artistId)
        else if (a === "go-album") QbzBridge.openAlbum(item.albumId)
        else if (a === "favorite") {
            item.isFavorite = !item.isFavorite
            QbzBridge.libraryToggleFavorite("track", item.id)
        }
    }

    // --- TrackCard (discover/TrackCard.slint) -----------------------------
    component LibTrackCard: Rectangle {
        property var item: ({})
        color: "transparent"
        readonly property bool overlayOn: tcArtArea.containsMouse || favBtn.hovered
            || playBtn.hovered || moreBtn.hovered

        Column {
            spacing: 0
            Rectangle {
                width: 200
                height: 200
                radius: theme.radiusSm
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: root.artMap[item.artKey] || ""
                    radius: theme.radiusSm
                }
                Rectangle {
                    anchors.fill: parent
                    radius: theme.radiusSm
                    color: "#000000"
                    opacity: parent.parent.overlayOn ? 0.6 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                // Body click PLAYS the track (TrackCard hover).
                MouseArea {
                    id: tcArtArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.playTrack(item.id)
                }
                // Hover overlay — favorite / play / more (y=120, h=44,
                // centered, spacing 12).
                Row {
                    y: 120
                    width: 200
                    height: 44
                    opacity: parent.parent.overlayOn ? 1.0 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                    Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                    LibOverlayBtn {
                        id: favBtn
                        name: item.isFavorite ? "heart-filled" : "heart"
                        active: item.isFavorite
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: {
                            item.isFavorite = !item.isFavorite
                            QbzBridge.libraryToggleFavorite("track", item.id)
                        }
                    }
                    LibOverlayBtn {
                        id: playBtn
                        name: "play-fill"
                        primary: true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzBridge.playTrack(item.id)
                    }
                    LibOverlayBtn {
                        id: moreBtn
                        name: "ellipsis"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: function (mouse) { trackMenu.openAtCursor(moreBtn, mouse.x, mouse.y) }
                    }
                    Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                }
                // Source badge (All feed, show-local): bottom-right.
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
                CardMenu {
                    id: trackMenu
                    menuWidth: 196
                    entries: root.trackMenuModel(item)
                    onPicked: function (a) { root.trackAction(item, a) }
                }
            }
            Item { width: 1; height: 6 }
            // Title / "Track • Artist" + quality badge.
            Row {
                width: 200
                height: 40
                spacing: theme.spacingSm
                Column {
                    width: parent.width - (tcQ.visible ? tcQ.width + theme.spacingSm : 0)
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Text {
                        width: parent.width
                        height: 20
                        text: item.title
                        color: tcTitleArea.containsMouse ? theme.accent : theme.textPrimary
                        font.pixelSize: theme.fontBody - 2
                        font.weight: theme.weightMedium
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        MouseArea {
                            id: tcTitleArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: QbzBridge.playTrack(item.id)
                        }
                    }
                    Text {
                        width: parent.width
                        height: 18
                        text: QbzBridge.tr("Track") + " • " + item.artist
                        color: item.artistId !== "" && tcArtistArea.containsMouse
                            ? theme.textPrimary : theme.textMuted
                        font.pixelSize: theme.fontLink - 1
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        MouseArea {
                            id: tcArtistArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: item.artistId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: if (item.artistId !== "") QbzBridge.openArtist(item.artistId)
                        }
                    }
                }
                QualityMini { id: tcQ; tier: item.qualityTier; anchors.verticalCenter: parent.verticalCenter }
            }
        }
    }

    // --- ArtistGridCard (discover/ArtistGridCard.slint; the Favorites arm
    // uses follow-mode "none" and wires ONLY open-artist — the hover overlay
    // buttons are dead even in Slint there, so they are omitted here;
    // POC-NOTE) ------------------------------------------------------------
    component LibArtistCard: Rectangle {
        property var item: ({})
        color: "transparent"
        readonly property bool overlayOn: agArea.containsMouse

        Column {
            spacing: 0
            Rectangle {
                width: 200
                height: 200
                radius: theme.radiusSm
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    x: 5
                    y: 5
                    width: 190
                    height: 190
                    source: root.artMap[item.artKey] || ""
                    radius: 95
                }
                Rectangle {
                    anchors.fill: parent
                    radius: theme.radiusSm
                    color: "#000000"
                    opacity: parent.parent.overlayOn ? 0.25 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                MouseArea {
                    id: agArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.openArtist(item.id)
                }
            }
            Item { width: 1; height: 6 }
            Text {
                width: 200
                height: 40
                text: item.title
                color: agNameArea.containsMouse ? theme.accent : theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                MouseArea {
                    id: agNameArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.openArtist(item.id)
                }
            }
        }
    }

    // --- PlaylistCard (discover/PlaylistCard.slint) ------------------------
    component LibPlaylistCard: Rectangle {
        property var item: ({})
        color: "transparent"
        readonly property bool overlayOn: plArtArea.containsMouse || plFav.hovered
            || plPlay.hovered || plMore.hovered

        Column {
            spacing: 0
            Rectangle {
                width: 200
                height: 200
                radius: theme.radiusSm
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: root.artMap[item.artKey] || ""
                    radius: theme.radiusSm
                }
                Rectangle {
                    anchors.fill: parent
                    radius: theme.radiusSm
                    color: "#000000"
                    opacity: parent.parent.overlayOn ? 0.6 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                MouseArea {
                    id: plArtArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // body-opens (Favorites convention): the playlist page is
                    // out of scope (POC-NOTE — inert).
                }
                Row {
                    y: 120
                    width: 200
                    height: 44
                    opacity: parent.parent.overlayOn ? 1.0 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                    Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                    LibOverlayBtn {
                        id: plFav
                        // Owned -> heart (favorite); foreign followed -> check;
                        // foreign -> user-plus (follow inert, POC-NOTE).
                        name: item.playlistOwned ? "heart"
                            : item.playlistFollowing ? "check" : "user-plus"
                        active: item.playlistOwned && item.isFavorite
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: {
                            if (item.playlistOwned) {
                                item.isFavorite = !item.isFavorite
                                QbzBridge.libraryToggleFavorite("playlist", item.id)
                            }
                        }
                    }
                    LibOverlayBtn {
                        id: plPlay
                        name: "play-fill"
                        primary: true
                        anchors.verticalCenter: parent.verticalCenter
                        // POC-NOTE: playlist play (playlist queue builder) out
                        // of scope — inert.
                    }
                    LibOverlayBtn {
                        id: plMore
                        name: "ellipsis"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: function (mouse) { plMenu.openAtCursor(plMore, mouse.x, mouse.y) }
                    }
                    Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                }
                CardMenu {
                    id: plMenu
                    menuWidth: 196
                    entries: [
                        { "label": QbzBridge.tr("Open playlist"), "icon": "list-music", "action": "open" },
                        { "label": QbzBridge.tr("Play"), "icon": "play-fill", "action": "play" },
                        { "label": item.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"),
                          "icon": item.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                    ]
                    onPicked: function (a) {
                        // "open"/"play": no playlist view / playlist queue in
                        // the POC (POC-NOTE — inert).
                        if (a === "favorite") {
                            item.isFavorite = !item.isFavorite
                            QbzBridge.libraryToggleFavorite("playlist", item.id)
                        }
                    }
                }
            }
            Item { width: 1; height: 6 }
            Column {
                width: 200
                height: 40
                spacing: 2
                leftPadding: theme.spacingXs
                Text {
                    width: parent.width
                    height: 20
                    text: item.title
                    color: theme.textPrimary
                    font.pixelSize: theme.fontBody - 2
                    font.weight: theme.weightSemibold
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
                Text {
                    width: parent.width
                    height: 16
                    text: item.subtitle
                    color: theme.textMuted
                    font.pixelSize: theme.fontLink - 1
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
        }
    }

    // --- LabelCard (discover/LabelCard.slint) ------------------------------
    component LibLabelCard: Rectangle {
        property var item: ({})
        color: "transparent"

        Column {
            spacing: 0
            Rectangle {
                width: 200
                height: 200
                radius: theme.radiusSm
                color: theme.surfaceElevated
                clip: true
                // No logo: indigo->violet gradient + disc glyph.
                Rectangle {
                    visible: item.imageUrl === ""
                    anchors.fill: parent
                    radius: theme.radiusSm
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0.0; color: "#6366f1" }
                        GradientStop { position: 1.0; color: "#8b5cf6" }
                    }
                    QbzIcon {
                        name: "disc-3"
                        width: 56
                        height: 56
                        anchors.centerIn: parent
                        tintName: "primary"
                    }
                }
                RoundedImage {
                    visible: item.imageUrl !== ""
                    anchors.fill: parent
                    source: root.artMap[item.artKey] || ""
                    radius: theme.radiusSm
                    // Logos are contain-fit, never cropped (LabelCard).
                    fit: "contain"
                }
                Rectangle {
                    anchors.fill: parent
                    radius: theme.radiusSm
                    color: "#000000"
                    opacity: lblArea.containsMouse ? 0.25 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                MouseArea {
                    id: lblArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    // Label landing page: out of scope (POC-NOTE — inert).
                }
            }
            Item { width: 1; height: 6 }
            Column {
                width: 200
                height: 40
                spacing: 2
                Text {
                    width: parent.width
                    height: 20
                    text: item.title
                    color: lblTitleArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: theme.fontBody - 2
                    font.weight: theme.weightMedium
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: lblTitleArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                    }
                }
                Text {
                    visible: item.subtitle !== ""
                    width: parent.width
                    text: item.subtitle
                    color: theme.textMuted
                    font.pixelSize: theme.fontLink - 1
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
        }
    }

    // --- All-feed LIST row (FavoritesView inline row, homologated) --------
    component FeedListRow: Rectangle {
        property var item: ({})
        height: 44
        radius: 6
        color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

        // Row body — click plays/opens by kind. Declared BEFORE the cells so
        // the ⋯ button and art-play win their clicks.
        MouseArea {
            id: rowArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                if (item.kind === "track") QbzBridge.playTrack(item.id)
                else if (item.kind === "album") QbzBridge.openAlbum(item.id)
                else if (item.kind === "artist") QbzBridge.openArtist(item.id)
                // playlist/label pages: out of scope (POC-NOTE).
            }
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 12
            // Col — artwork (round for artists) + hover play.
            Rectangle {
                width: 44
                height: 44
                anchors.verticalCenter: parent.verticalCenter
                radius: item.kind === "artist" ? 22 : 6
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    anchors.fill: parent
                    source: root.artMap[item.artKey] || ""
                    radius: item.kind === "artist" ? 22 : 6
                    fit: item.kind === "label" ? "contain" : "crop"
                }
                Rectangle {
                    visible: item.kind === "track" || item.kind === "album" || item.kind === "playlist"
                    anchors.fill: parent
                    radius: item.kind === "artist" ? 22 : 6
                    color: "#a6000000"
                    opacity: lrPlayArea.containsMouse ? 1.0 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                    QbzIcon { name: "play-fill"; width: 16; height: 16; anchors.centerIn: parent; tintName: "primary" }
                    MouseArea {
                        id: lrPlayArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (item.kind === "track") QbzBridge.playTrack(item.id)
                            else if (item.kind === "album") QbzBridge.playAlbum(item.id)
                            // playlist play: out of scope (POC-NOTE).
                        }
                    }
                }
            }
            // Col — title + subtitle (artist link for track/album).
            Column {
                width: parent.width - 44 - 116 - (srcCol.visible ? 44 : 0)
                    - 150 - 18 - 30 - 6 * 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    height: 18
                    text: item.title
                    color: lrTitleArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: 14
                    font.weight: theme.weightMedium
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: lrTitleArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (item.kind === "track") QbzBridge.playTrack(item.id)
                            else if (item.kind === "album") QbzBridge.openAlbum(item.id)
                            else if (item.kind === "artist") QbzBridge.openArtist(item.id)
                        }
                    }
                }
                Text {
                    visible: item.subtitle !== ""
                    width: parent.width
                    height: 16
                    text: item.subtitle
                    color: (item.artistId !== "" && (item.kind === "track" || item.kind === "album")
                            && lrSubArea.containsMouse) ? theme.accent : theme.textMuted
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: lrSubArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: (item.artistId !== "" && (item.kind === "track" || item.kind === "album"))
                            ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (item.artistId !== "") QbzBridge.openArtist(item.artistId)
                    }
                }
            }
            // Col — type (icon + caps label).
            Rectangle {
                width: 116
                height: parent.height
                color: "transparent"
                Row {
                    spacing: 6
                    anchors.verticalCenter: parent.verticalCenter
                    QbzIcon {
                        name: item.kind === "track" ? "music"
                            : item.kind === "playlist" ? "list-music"
                            : item.kind === "artist" ? "user"
                            : item.kind === "label" ? "disc-3" : "disc"
                        width: 13
                        height: 13
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: "muted"
                    }
                    Text {
                        text: item.kind === "track" ? QbzBridge.tr("Track")
                            : item.kind === "album" ? QbzBridge.tr("Album")
                            : item.kind === "artist" ? QbzBridge.tr("Artist")
                            : item.kind === "playlist" ? QbzBridge.tr("Playlist")
                            : QbzBridge.tr("Label")
                        color: theme.textMuted
                        font.pixelSize: 10
                        font.weight: theme.weightSemibold
                        font.letterSpacing: 1.2
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
            // Col — source glyph (only when local/Plex can appear).
            Rectangle {
                id: srcCol
                visible: root.showLocal
                width: 44
                height: parent.height
                color: "transparent"
                QbzIcon {
                    visible: item.source === "local" || item.source === "plex"
                    name: "hard-drive"
                    width: 14
                    height: 14
                    anchors.verticalCenter: parent.verticalCenter
                    tintName: item.source === "plex" ? "accent" : "primary"
                }
            }
            // Col — quality (albums + tracks).
            Rectangle {
                width: 150
                height: parent.height
                color: "transparent"
                QualityMini {
                    visible: (item.kind === "album" || item.kind === "track") && item.qualityTier !== ""
                    tier: item.qualityTier
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
            // Col — favorite / follow indicator.
            QbzIcon {
                name: item.kind === "artist" ? "user-plus"
                    : (item.isFavorite ? "heart-filled" : "heart")
                width: 18
                height: 18
                anchors.verticalCenter: parent.verticalCenter
                tintName: "accent"
            }
            // Col — context-menu button.
            Rectangle {
                width: 30
                height: 30
                radius: 6
                anchors.verticalCenter: parent.verticalCenter
                color: lrMenuArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon { name: "ellipsis"; width: 16; height: 16; anchors.centerIn: parent; tintName: "secondary" }
                MouseArea {
                    id: lrMenuArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function (mouse) { lrMenu.openAtCursor(lrMenuArea, mouse.x, mouse.y) }
                }
            }
        }
        CardMenu {
            id: lrMenu
            menuWidth: 196
            entries: item.kind === "track" ? root.trackMenuModel(item)
                : item.kind === "album" ? [
                    { "label": QbzBridge.tr("Open album"), "icon": "library-big", "action": "open" },
                    { "label": QbzBridge.tr("Play"), "icon": "play-fill", "action": "play" },
                    { "label": item.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"),
                      "icon": item.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                ]
                : item.kind === "artist" ? [
                    { "label": QbzBridge.tr("Go to artist"), "icon": "user", "action": "go-artist" },
                ]
                : [
                    { "label": QbzBridge.tr("Open"), "icon": "list-music", "action": "open" },
                    { "label": item.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"),
                      "icon": item.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                ]
            onPicked: function (a) {
                if (item.kind === "track") { root.trackAction(item, a); return }
                if (a === "open") {
                    if (item.kind === "album") QbzBridge.openAlbum(item.id)
                    // playlist/label pages: out of scope (POC-NOTE).
                } else if (a === "play") {
                    if (item.kind === "album") QbzBridge.playAlbum(item.id)
                } else if (a === "go-artist") {
                    QbzBridge.openArtist(item.id)
                } else if (a === "favorite") {
                    item.isFavorite = !item.isFavorite
                    QbzBridge.libraryToggleFavorite(item.kind, item.id)
                }
            }
        }
    }

    // --- Tracks-tab row (primitives/TrackRow.slint, 50px) -----------------
    component TrackListRow: Rectangle {
        property var item: ({})
        property int number: 0
        height: 50
        radius: 6
        color: trRowArea.containsMouse ? theme.surfaceHover : "transparent"

        MouseArea {
            id: trRowArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: QbzBridge.playTrack(item.id)
        }
        Row {
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 14
            // # / hover play (TrackRow number cell, 32px).
            Rectangle {
                width: 32
                height: parent.height
                color: "transparent"
                Text {
                    visible: !trRowArea.containsMouse
                    anchors.centerIn: parent
                    text: number
                    color: theme.textMuted
                    font.pixelSize: 13
                }
                Rectangle {
                    visible: trRowArea.containsMouse
                    anchors.centerIn: parent
                    width: 28
                    height: 28
                    radius: 14
                    color: "#3dffffff"
                    QbzIcon { name: "play-fill"; width: 14; height: 14; anchors.centerIn: parent; tintName: "primary" }
                }
            }
            // Title (+ explicit badge) / artist.
            Column {
                width: parent.width - 32 - 70 - 92 - 28 - 28 - 4 * 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Row {
                    spacing: 6
                    Text {
                        width: Math.min(implicitWidth, parent.parent.width - (item.explicit ? 22 : 0))
                        text: item.title
                        color: theme.textPrimary
                        font.pixelSize: 14
                        font.weight: theme.weightMedium
                        elide: Text.ElideRight
                    }
                    Rectangle {
                        visible: item.explicit
                        width: 16
                        height: 16
                        radius: 3
                        anchors.verticalCenter: parent.verticalCenter
                        color: theme.surfaceElevated
                        Text { anchors.centerIn: parent; text: "E"; color: theme.textMuted; font.pixelSize: 10; font.weight: theme.weightSemibold }
                    }
                }
                Text {
                    width: parent.width
                    text: item.artist
                    color: theme.textMuted
                    font.pixelSize: 12
                    elide: Text.ElideRight
                }
            }
            // Duration.
            Text {
                width: 70
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                text: item.duration
                color: theme.textMuted
                font.pixelSize: 12
            }
            // Quality mini.
            Rectangle {
                width: 92
                height: parent.height
                color: "transparent"
                QualityMini { tier: item.qualityTier; anchors.verticalCenter: parent.verticalCenter }
            }
            // Heart.
            Rectangle {
                width: 28
                height: 28
                radius: 6
                anchors.verticalCenter: parent.verticalCenter
                color: trHeartArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon {
                    name: item.isFavorite ? "heart-filled" : "heart"
                    width: 15
                    height: 15
                    anchors.centerIn: parent
                    tintName: item.isFavorite ? "favorite" : "muted"
                }
                MouseArea {
                    id: trHeartArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        item.isFavorite = !item.isFavorite
                        QbzBridge.libraryToggleFavorite("track", item.id)
                    }
                }
            }
            // ⋯ menu.
            Rectangle {
                width: 28
                height: 28
                radius: 6
                anchors.verticalCenter: parent.verticalCenter
                color: trMenuArea.containsMouse ? theme.surfaceElevated : "transparent"
                QbzIcon { name: "ellipsis"; width: 15; height: 15; anchors.centerIn: parent; tintName: "secondary" }
                MouseArea {
                    id: trMenuArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: function (mouse) { trMenu.openAtCursor(trMenuArea, mouse.x, mouse.y) }
                }
            }
        }
        CardMenu {
            id: trMenu
            menuWidth: 196
            entries: root.trackMenuModel(item)
            onPicked: function (a) { root.trackAction(item, a) }
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

                delegate: Item {
                    required property var modelData
                    width: 200
                    height: 246

                    Component {
                        id: albumCardComp
                        AlbumCard {
                            albumId: modelData.id
                            title: modelData.title
                            artist: modelData.artist
                            artistId: modelData.artistId
                            genre: modelData.genre
                            year: modelData.year
                            qualityTier: modelData.qualityTier
                            artSource: root.artMap[modelData.artKey] || ""
                            isFavorite: modelData.isFavorite
                            isPinned: modelData.isPinned
                            source: modelData.source
                        }
                    }
                    Component {
                        id: trackCardComp
                        LibTrackCard { item: modelData }
                    }
                    Component {
                        id: artistCardComp
                        LibArtistCard { item: modelData }
                    }
                    Component {
                        id: playlistCardComp
                        LibPlaylistCard { item: modelData }
                    }
                    Component {
                        id: labelCardComp
                        LibLabelCard { item: modelData }
                    }
                    Loader {
                        anchors.fill: parent
                        sourceComponent: modelData.kind === "album" ? albumCardComp
                            : modelData.kind === "track" ? trackCardComp
                            : modelData.kind === "artist" ? artistCardComp
                            : modelData.kind === "playlist" ? playlistCardComp
                            : labelCardComp
                    }
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

                delegate: Loader {
                    required property var modelData
                    required property int index
                    width: list.width
                    height: root.activeTab === "tracks" ? 50 : 44
                    Component {
                        id: trackRowComp
                        TrackListRow { item: modelData; number: index + 1 }
                    }
                    Component {
                        id: feedRowComp
                        FeedListRow { item: modelData }
                    }
                    sourceComponent: root.activeTab === "tracks" ? trackRowComp : feedRowComp
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
