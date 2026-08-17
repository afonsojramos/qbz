// Album detail page — QML port of album/AlbumPageView.slint.
//
// Header (224px cover, title, credited-artist line, meta with label link,
// description + Read more, CircleAction row), divider, toolbar (quality
// badge + track search), column header, track list (Disc/work headers,
// TrackRow replica with the playing-row pill + number↔play cell + live
// heart), label/awards sidebar, and the two bottom carousels ("From the
// same artist", "Listening suggestions").
//
// POC-NOTEs: the offline download column (TrackRow status glyphs + ⋯ menu
// rows) and multi-select + bulk bar are LIVE. Still out: DiscHeaderMenu
// per-disc actions.
//
// Header atmosphere (AlbumPageView.slint:161-189, 221-257): the
// artwork-tinted band IS wired now, through the shared
// controls/HeaderGradient.qml (route B — see that file for why the blurred
// route is not available on this path). It brings the .slint's header-colour
// rules with it: with the band on, the header sits on a DARK backdrop, so
// the text goes light regardless of theme (`hdrStrong` / `hdrBody`,
// .slint:169-172) and the CircleActions switch to their overlay palette
// (`hdrOverlay` = the .slint's inverted `hdr-on-surface`, :179).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../rows"
import "../shell"
import "../theme"

Rectangle {
    id: root
    // Transparent while the ambient background is active (phase 14 —
    // HomeView.slint:163: the frosted content panel shows through).
    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: theme.ambientOn

    // Round to the AppShell content-frame bezel (Radius.md): QML clips
    // are rectangular, so the frame's own rounding never reaches the
    // view — the view's own fill must round instead.
    radius: 12

    QbzTheme { id: theme }

    // The view's album + url-keyed cover map (artwork pipeline).
    readonly property var album: JSON.parse(QbzAlbum.albumJson)
    readonly property var header: album.header || ({})
    readonly property var tracks: album.tracks || []
    // `header.awards`, NOT `album.awards`. It is a field of AlbumHeader
    // (src/album_qt.rs:171), not of AlbumViewData — read one level too high it
    // is `undefined`, `|| []` swallows that, and the AWARDS block simply never
    // renders. Which is what it did, for every album, since the block was
    // written: an empty Repeater draws nothing and reports nothing, and
    // `hasSidebar` counted the same empty list, so an album whose ONLY sidebar
    // content was its awards showed no sidebar at all.
    readonly property var awards: header.awards || []
    property var coverMap: ({})
    // Client-side track search (AlbumActions.search equivalent).
    property string trackQuery: ""

    // ---- Header atmosphere (AlbumPageView.slint:161-189) -----------------
    // The pref, LIVE where possible: the settings snapshot is only published
    // on settings-view open / mutation, so on a cold start it is empty and
    // the document's own copy (album_qt.rs `headerGradient`) answers instead.
    readonly property bool headerGradientPref: {
        var raw = QbzBridge.settingsJson
        if (raw && raw.length > 2) {
            try {
                var d = JSON.parse(raw)
                if (d.albumHeaderGradient !== undefined)
                    return d.albumHeaderGradient === true
            } catch (e) { /* fall through to the document copy */ }
        }
        return album.headerGradient !== false
    }
    // .slint:168 — the album's own atmosphere is SUPPRESSED under the
    // app-wide dynamic background (they clash); the dynamic background then
    // provides the dark backdrop instead.
    readonly property bool headerAtmoOn: headerGradientPref && !ambientOn
    // .slint:167 — dark backdrop from EITHER source means light header text.
    readonly property bool headerLight: headerGradientPref || ambientOn
    readonly property color hdrStrong: headerLight ? "#ffffff" : theme.textPrimary
    readonly property color hdrBody: headerLight ? "#e0ffffff" : theme.textSecondary
    // (the .slint declares an `hdr-muted` tier too, :173, but never binds it
    //  to anything — not ported rather than ported dead)
    // .slint:179 — with no dark backdrop the circles use the on-surface arm.
    readonly property bool hdrOverlay: headerLight

    readonly property var visibleTracks: {
        if (trackQuery === "") return tracks
        var q = trackQuery.toLowerCase()
        return tracks.filter(function (t) {
            return t.title.toLowerCase().indexOf(q) >= 0
        })
    }

    // ---- Loading staging (album_qt.rs publishes in passes) ---------------
    // The PRIMARY document (header + tracks) lands as soon as /album/get
    // answers; each bottom rail arrives later carrying its own flag, so the
    // page is usable while Qobuz suggestions and the Last.fm row resolve.
    //
    // A rail is shown when it HAS cards, its placeholder when its flag is
    // still up. Both false = the section is ABSENT: `moreLoading` is seeded
    // false when the album has no artist id and `similarLoading` false when
    // Last.fm is not connected, so nothing ever spins forever on a row that
    // will never arrive.
    readonly property bool primaryLoading: QbzAlbum.albumLoading && tracks.length === 0
    readonly property bool moreLoading: album.moreLoading === true
                                        && (album.moreFromArtist || []).length === 0
    readonly property bool suggestionsLoading: album.suggestionsLoading === true
                                               && (album.suggestions || []).length === 0
    readonly property bool similarLoading: album.similarLoading === true
                                           && (album.similarAlbums || []).length === 0

    // ONE 900ms phase for every placeholder on the page (QbzSkeleton's COST
    // note: N placeholders, 1 timer). Stops dead when nothing is pending.
    Timer {
        id: skeletonPhase
        property bool on: false
        interval: 900
        repeat: true
        running: root.visible && (root.primaryLoading || root.moreLoading
                                  || root.suggestionsLoading || root.similarLoading)
        onTriggered: on = !on
    }

    // Placeholder cards that fill one rail (SectionRail's 232px pitch).
    readonly property int railSkeletonCount:
        Math.max(1, Math.min(8, Math.ceil((root.width - 64) / 232)))

    // Disc headers + work headers precede their first row (computed once
    // per track list change, mirroring AlbumState's disc-header-number /
    // work-header model fields).
    function headerFor(i) {
        var t = visibleTracks[i]
        if (!t) return null
        if (i === 0) {
            return { disc: t.disc > 1 || (visibleTracks.length > 0 && visibleTracks[visibleTracks.length - 1].disc > 1) ? t.disc : 0,
                     work: t.workHeader }
        }
        var prev = visibleTracks[i - 1]
        var multi = visibleTracks[visibleTracks.length - 1].disc > 1
        return { disc: (multi && t.disc !== prev.disc) ? t.disc : 0,
                 work: t.workHeader !== prev.workHeader ? t.workHeader : "" }
    }

    Connections {
        target: QbzLibrary
        function onLibraryArtworkReady(key, path) {
            var m = root.coverMap
            m[key] = path
            root.coverMap = Object.assign({}, m)
        }
        // The SETTLED heart from Rust: the flipped value when the write
        // landed, the UNCHANGED one when it failed. The header click writes
        // its optimistic flip into localToggles and nothing used to correct
        // it, so a 404'd un-favorite stayed visibly un-favorited until the
        // user navigated away — LibraryView.qml was this signal's only
        // listener. Key shape is `library_qt::feed_key` (`{kind}:{id}`).
        function onLibraryFavoriteChanged(key, value) {
            var id = (header && header.id) ? header.id : ""
            if (id !== "" && key === "album:" + id)
                root.setToggleState("album", value)
        }
    }
    // Album-blacklist settle / rollback. `blacklistChanged` carries the state
    // the write actually produced — flipped on success, UNCHANGED on failure
    // (blacklist_qt.rs `album_toggle`), so this is both the cross-surface walk
    // (the manager's row `x`, a card's "Block this album") and the rollback for
    // the header menu's optimistic flip below. Same two-arg `{kind}:{id}` shape
    // as pinChanged / libraryFavoriteChanged above; a separate Connections
    // block only because the signal lives on a different singleton.
    Connections {
        target: QbzBlacklist
        function onBlacklistChanged(key, value) {
            var id = (header && header.id) ? header.id : ""
            if (id !== "" && key === "album:" + id)
                root.setToggleState("blocked", value)
        }
    }
    Component.onCompleted: { syncAlbumState(); dispatchCovers() }
    onAlbumChanged: { syncAlbumState(); dispatchCovers() }
    // The derived binding settles AFTER onAlbumChanged fires (stale race) —
    // redispatch when the header itself updates.
    onHeaderChanged: { syncAlbumState(); dispatchCovers() }

    // Optimistic heart / pin state. The document is republished once per
    // deferred rail now, and every republish re-parses `album` — a toggle
    // written straight onto the parsed object would silently pop back a
    // second later. Overrides live here and win until the album changes.
    // (Same pattern, same reason, as ArtistView.localToggles.)
    property var localToggles: ({})
    function toggleState(key, fallback) {
        return localToggles[key] !== undefined ? localToggles[key] : fallback === true
    }
    function setToggleState(key, value) {
        var m = localToggles
        m[key] = value
        localToggles = Object.assign({}, m)
    }

    // Per-album view state is reset ONLY when the id actually changes, or a
    // deferred rail landing would yank the user's toggles back mid-read.
    property string loadedAlbumId: ""
    function syncAlbumState() {
        var id = (header && header.id) ? header.id : ""
        if (id === loadedAlbumId)
            return
        loadedAlbumId = id
        localToggles = ({})
        dispatchedCovers = ({})
        // A new album carries no selection over (Slint resets multi-select
        // on every album load/reset, album.rs:692-694).
        setMultiSelect(false)
        // Re-seed the live offline map from the new document (track rows
        // carry their own seeded cacheStatus; this map tracks the LIVE one).
        var seed = ({})
        var ts = album.tracks || []
        for (var i = 0; i < ts.length; i++)
            seed[ts[i].id] = ts[i].cacheStatus || 0
        cacheStates = seed
    }

    // --- Multi-select (AlbumPageView.slint:736-807) -------------------------
    // The selection lives here in QML (LibraryView precedent): select-all and
    // clear never reach Rust, everything else goes down as a JSON id array
    // through QbzAlbum.albumBulkAction.
    property bool multiSelect: false
    property var selected: ({})
    readonly property int selectedCount: Object.keys(root.selected).length
    readonly property bool multiSelectOn: root.multiSelect
    function setMultiSelect(on) {
        root.multiSelect = on
        if (!on) root.selected = ({})
    }
    function toggleSelected(id) {
        var m = root.selected
        if (m[id] === true) delete m[id]
        else m[id] = true
        // A rebind needs a NEW object reference — mutating in place notifies
        // nothing.
        root.selected = Object.assign({}, m)
    }
    /// Selected ids in VISIBLE order — never Object.keys (integer-like keys
    /// iterate in NUMERIC order, not the user's).
    function selectedIdsInOrder() {
        var rows = root.visibleTracks
        var out = []
        for (var i = 0; i < rows.length; i++)
            if (root.selected[rows[i].id] === true) out.push(rows[i].id)
        return out
    }
    function bulkAction(action) {
        if (action === "select-all") {
            var m = {}
            var rows = root.visibleTracks
            for (var i = 0; i < rows.length; i++) m[rows[i].id] = true
            root.selected = m
            return
        }
        if (action === "clear") { root.selected = ({}); return }
        var ids = root.selectedIdsInOrder()
        if (ids.length === 0) return
        QbzAlbum.albumBulkAction(header.id, JSON.stringify(ids), action)
        // The Slint keeps the selection while a picker is still open (a
        // failed write is retried from the same modal) and clears after
        // everything else.
        if (action !== "add-to-playlist" && action !== "add-to-mixtape")
            root.selected = ({})
    }
    // Ctrl+A / Escape hotkey router seam (AppShell duck-types these).
    function selectAll() {
        if (!root.multiSelect) root.setMultiSelect(true)
        root.bulkAction("select-all")
    }
    function exitMultiSelectMode() {
        if (root.multiSelect) root.setMultiSelect(false)
    }

    // --- Offline cache live state ------------------------------------------
    // Row glyphs own their live status (TrackRow's Connections). This map
    // exists for ONE consumer: the ⋯ menu's "Make available offline" /
    // "Refresh offline copy" swap, which needs the all-rows-ready aggregate
    // (Slint recomputes album-fully-cached on every row flip,
    // main.rs:2426-2438).
    property var cacheStates: ({})
    readonly property bool fullyCachedLive: {
        var ts = album.tracks || []
        if (ts.length === 0)
            return false
        var m = cacheStates
        for (var i = 0; i < ts.length; i++) {
            var st = m[ts[i].id]
            if (st === undefined) st = ts[i].cacheStatus || 0
            if (st !== 3)
                return false
        }
        return true
    }
    Connections {
        target: QbzShell
        function onTrackCacheStatusChanged(trackId, status, progress) {
            if (!(trackId in root.cacheStates))
                return
            var m = root.cacheStates
            m[trackId] = status
            root.cacheStates = Object.assign({}, m)
        }
    }

    // Already-requested artwork keys. The document is now published in FOUR
    // passes (primary, then each rail), and every pass re-fires this — resending
    // the whole list each time is pure waste, so only what is new goes out.
    property var dispatchedCovers: ({})
    function dispatchCovers() {
        var urls = []
        if (header && header.artUrl) urls.push(header.artUrl)
        var more = album.moreFromArtist || []
        for (var i = 0; i < more.length; i++) if (more[i].artUrl) urls.push(more[i].artUrl)
        var sug = album.suggestions || []
        for (i = 0; i < sug.length; i++) if (sug[i].artUrl) urls.push(sug[i].artUrl)
        // The Last.fm row's covers ride the same dispatch — without this its
        // cards would render as empty frames.
        var sim = album.similarAlbums || []
        for (i = 0; i < sim.length; i++) if (sim[i].artUrl) urls.push(sim[i].artUrl)

        var seen = dispatchedCovers
        var fresh = []
        for (i = 0; i < urls.length; i++) {
            if (!seen[urls[i]]) {
                seen[urls[i]] = true
                fresh.push(urls[i])
            }
        }
        if (fresh.length > 0) {
            dispatchedCovers = seen
            QbzShell.sidebarArtworkWindow(JSON.stringify(fresh))
        }
    }

    // Ghost CircleAction (secondary, on-surface variant): elevated disc,
    // strong ring, text-primary icon (accent when active).


    // Track list row (TrackRow.slint replica: number cell, no artwork,
    // Sidebar label/award card (SidebarCard).
    component SidebarCard: Rectangle {
        property string name: ""
        property color gradA: "#6366f1"
        property color gradB: "#8b5cf6"
        property string iconName: "disc"
        signal clicked()
        width: parent ? parent.width : 0
        height: 48
        radius: theme.radiusSm
        color: scArea.containsMouse ? theme.surfaceHover : "transparent"
        Row {
            anchors.fill: parent
            anchors.margins: 6
            spacing: 10
            Rectangle {
                width: 28
                height: 28
                radius: 14
                anchors.verticalCenter: parent.verticalCenter
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: gradA }
                    GradientStop { position: 1.0; color: gradB }
                }
                QbzIcon {
                    name: iconName
                    width: 13
                    height: 13
                    anchors.centerIn: parent
                    // On the card's fixed brand gradient disc (indigo/violet
                    // or amber), never on a theme surface.
                    tintName: "white"
                }
            }
            Text {
                width: parent.width - 38
                anchors.verticalCenter: parent.verticalCenter
                text: name
                color: theme.textSecondary
                font.pixelSize: 12
                font.weight: theme.weightMedium
                wrapMode: Text.WordWrap
            }
        }
        MouseArea {
            id: scArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: parent.clicked()
        }
    }
    component SidebarHeading: Text {
        color: theme.textMuted
        font.pixelSize: 10
        font.weight: theme.weightSemibold
        font.letterSpacing: 1
    }

    // Placeholder for a bottom rail that has not resolved yet: the SAME 28px
    // header band, 232px pitch and 246px card band SectionRail uses, so the
    // page does not jump when the real cards replace it. Built out of the
    // shared QbzSkeleton — no local skeleton primitive.
    //
    // Everything it needs is a property, not a file-scope id: an inline
    // `component` does not see the enclosing document's ids (the gotcha
    // QbzSkeleton.qml documents), so `phase` is passed in by the host.
    component RailSkeleton: Column {
        id: railSk
        property bool phase: false
        property int cardCount: 4
        width: parent ? parent.width : 0
        spacing: 12

        Item {
            width: parent.width
            height: 28
            QbzSkeleton {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                variant: "block"
                width: 180
                height: 20
                phase: railSk.phase
            }
        }
        Item {
            width: parent.width
            height: 246
            clip: true
            Row {
                spacing: 32
                Repeater {
                    model: railSk.cardCount
                    // "card" carries its own 200 x (200+42) footprint.
                    delegate: QbzSkeleton {
                        required property int index
                        variant: "card"
                        cellIndex: index
                        phase: railSk.phase
                    }
                }
            }
        }
    }

    // One EXTERNAL LINKS brand icon (AlbumPageView.slint BrandLink): the bare
    // brand SVG in its NATIVE colors — no tint pass, no visible label, the
    // name lives in the hover tooltip (Feishin-style inline links).
    component BrandLink: Rectangle {
        property string iconSource: ""
        property string name: ""
        property string url: ""
        width: 30
        height: 30
        radius: 6
        color: brandArea.containsMouse ? theme.surfaceHover : "transparent"
        Image {
            anchors.centerIn: parent
            source: iconSource
            width: 18
            height: 18
            sourceSize: Qt.size(36, 36)
            fillMode: Image.PreserveAspectFit
            opacity: brandArea.containsMouse ? 1.0 : 0.85
            Behavior on opacity { NumberAnimation { duration: 120 } }
        }
        MouseArea {
            id: brandArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            // Deep link only — the browser does the work, nothing is fetched
            // here and no integration has to be connected.
            onClicked: if (url !== "") Qt.openUrlExternally(url)
            // The Slint BrandLink carries the name in the shared tooltip
            // bubble; the Qt port rides Qt's own ToolTip (LocalMultiSelectBar
            // precedent).
            ToolTip.visible: containsMouse && name !== ""
            ToolTip.text: name
            ToolTip.delay: 350
        }
    }

    // Absolute qrc prefix for the brand SVGs — same rule as QbzIcon: a
    // relative URL resolves against the CONSUMER's document depth.
    readonly property string brandDir: "qrc:/qt/qml/com/blitzfc/qbz/qml/assets/brand/"

    // Whether the right-hand album sidebar has anything to show at all.
    readonly property bool hasSidebar: (header.label || "") !== ""
                                       || awards.length > 0
                                       || header.showExternalLinks === true

    // ============================ the page ================================
    Flickable {
        id: pageFlick
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: page.implicitHeight
        boundsBehavior: Flickable.StopAtBounds

        // Artwork-tinted header band. FIRST child so it paints under the
        // page, and inside the Flickable so it scrolls with the content
        // (AlbumPageView.slint:221 — the atmosphere lives in the Flickable).
        // Full-bleed on purpose: the page's 32px padding must NOT clip it or
        // a dark gutter strip appears on the right (.slint:199-202).
        HeaderGradient {
            x: 0
            y: 0
            width: pageFlick.width
            // .slint:189 `atmo-height: page.y + header-divider.y` — the band
            // ends EXACTLY on the header/track-list divider, whatever height
            // a long editorial description gave the header.
            height: page.y + headerDivider.y
            tint: album.headerColor || ""
            // Route A: the blurred field. Empty until the cover resolves, and the
            // flat tint stands in meanwhile (HeaderGradient handles the swap).
            atmosphere: album.headerAtmosphere || ""
            active: root.headerAtmoOn
        }

        Column {
            id: page
            width: parent.width
            leftPadding: 32
            rightPadding: 32
            topPadding: 11
            bottomPadding: 100
            spacing: 0

            // NavButtons is a 0px placeholder in the Slint source.
            Item { width: 1; height: 22 }

            // --- Album header skeleton ----------------------------------
            // Mounted on the primary flag, and the real header is hidden by
            // the same flag: opening album B never renders a half-empty
            // header frame while B's document is in flight.
            Row {
                visible: root.primaryLoading
                width: parent.width - 64
                spacing: 32

                QbzSkeleton {
                    variant: "block"
                    width: 224
                    height: 224
                    blockRadius: 12
                    phase: skeletonPhase.on
                }
                Column {
                    width: parent.width - 224 - 32
                    spacing: 12
                    Item { width: 1; height: 6 }
                    QbzSkeleton { variant: "block"; width: Math.min(420, parent.width); height: 30; cellIndex: 0; phase: skeletonPhase.on }
                    QbzSkeleton { variant: "block"; width: Math.min(260, parent.width); height: 18; cellIndex: 1; phase: skeletonPhase.on }
                    QbzSkeleton { variant: "block"; width: Math.min(340, parent.width); height: 14; cellIndex: 2; phase: skeletonPhase.on }
                    Item { width: 1; height: 14 }
                    Row {
                        spacing: 12
                        Repeater {
                            model: 4
                            delegate: QbzSkeleton {
                                required property int index
                                variant: "circle"
                                width: 44
                                height: 44
                                cellIndex: index
                                phase: skeletonPhase.on
                            }
                        }
                    }
                }
            }

            // --- Album header -------------------------------------------
            Row {
                visible: !root.primaryLoading
                width: parent.width - 64
                spacing: 32

                Rectangle {
                    width: 224
                    height: 224
                    radius: 12
                    color: theme.surfaceElevated
                    // No clip: RoundedImage confines itself on both arms; a clip is an
                    // unconditional batch root. coverMenu and CoverLightbox are
                    // view-root siblings, never descendants of this frame.
                    RoundedImage {
                        anchors.fill: parent
                        // A custom cover override (shared custom_artwork
                        // store) beats the url-keyed pipeline image.
                        source: (header.customCoverPath || "") !== ""
                            ? "file://" + header.customCoverPath
                            : (root.coverMap[header.artUrl] || "")
                        radius: 12
                    }
                    // Left-click: the lightbox (NEW — Slint's left-click is
                    // inert). Right-click: the cover menu (Slint
                    // AlbumPageView.slint:300-370).
                    MouseArea {
                        anchors.fill: parent
                        acceptedButtons: Qt.LeftButton | Qt.RightButton
                        cursorShape: Qt.PointingHandCursor
                        onClicked: function (mouse) {
                            if (mouse.button === Qt.RightButton) {
                                coverMenu.openAtCursor(coverMenuAnchor, mouse.x, mouse.y)
                            } else {
                                coverLightbox.openWith(root.bestCoverSource())
                            }
                        }
                    }
                    Item { id: coverMenuAnchor; anchors.fill: parent }
                }

                Column {
                    width: parent.width - 224 - 32
                    anchors.top: parent.top
                    anchors.topMargin: 4
                    spacing: 0

                    Text {
                        width: parent.width
                        text: header.title || ""
                        color: root.hdrStrong
                        font.pixelSize: theme.fontSection
                        font.weight: theme.weightBold
                        elide: Text.ElideRight
                    }
                    Item { width: 1; height: 4 }
                    // Credited-artist line (links + role suffixes).
                    Flow {
                        width: parent.width
                        spacing: 0
                        Repeater {
                            model: header.credits || []
                            delegate: Row {
                                required property var modelData
                                required property int index
                                spacing: 0
                                Text {
                                    visible: index > 0
                                    text: "  •  "
                                    color: root.hdrBody
                                    font.pixelSize: theme.fontHeading
                                    font.weight: theme.weightBold
                                }
                                Text {
                                    text: modelData[0]
                                    color: creditArea.containsMouse && modelData[1] !== "" ? root.hdrStrong : root.hdrBody
                                    font.pixelSize: theme.fontHeading
                                    font.weight: theme.weightBold
                                    MouseArea {
                                        id: creditArea
                                        anchors.fill: parent
                                        enabled: modelData[1] !== ""
                                        hoverEnabled: true
                                        cursorShape: modelData[1] !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                                        onClicked: QbzArtist.openArtist(modelData[1])
                                    }
                                }
                                Text {
                                    visible: modelData[2] !== ""
                                    text: " (" + modelData[2] + ")"
                                    color: root.hdrBody
                                    font.pixelSize: theme.fontHeading
                                }
                            }
                        }
                    }
                    Item { width: 1; height: 10 }
                    // Meta line (label as a clickable link when navigable).
                    Row {
                        spacing: 0
                        visible: (header.labelId || "") !== "" && (header.label || "") !== ""
                        Text {
                            visible: (header.metaPre || "") !== ""
                            text: (header.metaPre || "") + "   •   "
                            color: root.hdrBody
                            font.pixelSize: theme.fontBody
                        }
                        Text {
                            text: header.label || ""
                            color: labelArea.containsMouse ? theme.accent : root.hdrBody
                            font.pixelSize: theme.fontBody
                            MouseArea {
                                id: labelArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: QbzHome.openLabel(header.labelId)
                            }
                        }
                        Text {
                            visible: (header.metaPost || "") !== ""
                            text: "   •   " + (header.metaPost || "")
                            color: root.hdrBody
                            font.pixelSize: theme.fontBody
                        }
                    }
                    Text {
                        visible: (header.labelId || "") === "" || (header.label || "") === ""
                        width: parent.width
                        text: header.infoLine || ""
                        color: root.hdrBody
                        font.pixelSize: theme.fontBody
                        elide: Text.ElideRight
                    }

                    // Editorial description + Read more.
                    Item { visible: (header.description || "") !== ""; width: 1; height: 12 }
                    Text {
                        visible: (header.description || "") !== ""
                        width: parent.width
                        text: header.descriptionShort || ""
                        color: root.hdrBody
                        font.pixelSize: theme.fontLegal
                        wrapMode: Text.WordWrap
                    }
                    Item { visible: (header.description || "") !== (header.descriptionShort || ""); width: 1; height: 4 }
                    Text {
                        visible: (header.description || "") !== (header.descriptionShort || "")
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
                                while (shell && shell.openTextModal === undefined) shell = shell.parent
                                if (shell) shell.openTextModal(QbzSession.tr("About this album", QbzSession.trRev), header.description || "")
                            }
                        }
                    }

                    Item { width: 1; height: 20 }
                    // Action row — AlbumPageView.slint:504-640. One shared
                    // CircleAction for every button including Play (the
                    // hand-rolled 44px disc it used to be drifted from the
                    // control on ring, hover and glyph tint); the palette arm
                    // follows the header backdrop, exactly like the .slint's
                    // `on-surface: root.hdr-on-surface`.
                    Row {
                        spacing: 12
                        QbzCircleAction {
                            primary: true
                            overlay: root.hdrOverlay
                            name: "play-fill"
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzPlayer.playAlbum(header.id)
                        }
                        QbzCircleAction {
                            name: "shuffle"
                            overlay: root.hdrOverlay
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzPlayer.playAlbumShuffled(header.id)
                        }
                        QbzCircleAction {
                            readonly property bool favorite: root.toggleState("album", header.isFavorite)
                            name: favorite ? "heart-filled" : "heart"
                            overlay: root.hdrOverlay
                            active: favorite
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: {
                                root.setToggleState("album", !favorite)
                                QbzLibrary.libraryToggleFavorite("album", header.id)
                            }
                        }
                        // Radio: the Qobuz /radio/album endpoint, via the
                        // For-You tile seam (foryou_qt::start_album_radio —
                        // minimal track objects are enriched before play).
                        QbzCircleAction {
                            name: "radio"
                            overlay: root.hdrOverlay
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzHome.startAlbumRadio(header.id)
                        }
                        // Booklet: only when the album carries a PDF goody
                        // (AlbumPageView.slint:575-585); downloads via save-as.
                        QbzCircleAction {
                            visible: header.hasBooklet === true
                            name: "book-open"
                            overlay: root.hdrOverlay
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzAlbum.downloadBooklet()
                        }
                        // Mixtape/Collection picker (an album is accepted by
                        // BOTH kinds — myqbz_add_qt's Accepts engine).
                        QbzCircleAction {
                            name: "cassette-tape"
                            overlay: root.hdrOverlay
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: QbzAlbum.addToMixtape(header.id)
                        }
                        // Album info: the Credits/Review modal
                        // (AlbumInfoModal.qml, data via album_info_qt.rs).
                        QbzCircleAction {
                            name: "info"
                            overlay: root.hdrOverlay
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: albumInfo.openFor(header.id)
                        }
                        QbzCircleAction {
                            id: albumMenuBtn
                            name: "ellipsis"
                            overlay: root.hdrOverlay
                            anchors.verticalCenter: parent.verticalCenter
                            onClicked: function (mouse) { albumMenu.openAtCursor(albumMenuBtn, mouse.x, mouse.y) }
                        }
                    }
                }
            }

            Item { width: 1; height: 20 }
            // Header divider. The gradient band above sizes itself to THIS
            // item's y (.slint:189 atmo-height), so it keeps its id.
            Rectangle { id: headerDivider; width: parent.width - 64; height: 1; color: theme.borderSubtle }
            Item { width: 1; height: 8 }

            // --- Track list + label/awards sidebar ----------------------
            Row {
                width: parent.width - 64
                spacing: 32

                Column {
                    width: parent.width - (root.hasSidebar ? 232 : 0)
                    spacing: 0

                    // Track-list placeholder — same flag the spinner used,
                    // now in the shape of the list it is standing in for
                    // (toolbar band + column-header band + 8 rows at the
                    // TrackRow 50px pitch), so nothing shifts on arrival.
                    Column {
                        visible: root.primaryLoading
                        width: parent.width
                        spacing: 0

                        Item { width: 1; height: 52 }
                        Item { width: 1; height: 40 }
                        Repeater {
                            model: 8
                            delegate: Item {
                                required property int index
                                width: parent ? parent.width : 0
                                height: 50
                                QbzSkeleton {
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.left: parent.left
                                    anchors.leftMargin: 12
                                    variant: "block"
                                    width: 20
                                    height: 12
                                    cellIndex: index
                                    phase: skeletonPhase.on
                                }
                                QbzSkeleton {
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.left: parent.left
                                    anchors.leftMargin: 60
                                    variant: "block"
                                    width: Math.max(90, parent.width * 0.36)
                                    height: 14
                                    cellIndex: index
                                    phase: skeletonPhase.on
                                }
                                QbzSkeleton {
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.right: parent.right
                                    anchors.rightMargin: 128
                                    variant: "block"
                                    width: 52
                                    height: 12
                                    cellIndex: index
                                    phase: skeletonPhase.on
                                }
                                QbzSkeleton {
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.right: parent.right
                                    anchors.rightMargin: 48
                                    variant: "block"
                                    width: 52
                                    height: 12
                                    cellIndex: index
                                    phase: skeletonPhase.on
                                }
                            }
                        }
                    }

                    // Multi-select bulk bar (controls/QbzMultiSelectBar.qml),
                    // INLINE at the top of the track column — the Local
                    // Library album view's layout (bar in flow, content below)
                    // instead of a floating overlay.
                    QbzMultiSelectBar {
                        id: bulkBar
                        visible: root.multiSelect && !root.primaryLoading
                        width: parent.width
                        selectedCount: root.selectedCount
                        // The reference's full AlbumView inventory
                        // (AlbumPageView.slint:792-807), make-offline included.
                        actions: [
                            { "id": "select-all", "label": QbzSession.tr("Select all", QbzSession.trRev), "icon": "square-check-big", "danger": false, "needsSelection": false },
                            { "id": "play-next", "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "danger": false, "needsSelection": true },
                            { "id": "play-later", "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "danger": false, "needsSelection": true },
                            { "id": "queue", "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "danger": false, "needsSelection": true },
                            { "id": "add-to-playlist", "label": QbzSession.tr("Add to playlist", QbzSession.trRev), "icon": "list-music", "danger": false, "needsSelection": true },
                            { "id": "add-to-favorites", "label": QbzSession.tr("Add to Library", QbzSession.trRev), "icon": "heart", "danger": false, "needsSelection": true },
                            { "id": "make-offline", "label": QbzSession.tr("Make available offline", QbzSession.trRev), "icon": "cloud-download", "danger": false, "needsSelection": true },
                            { "id": "clear", "label": QbzSession.tr("Clear", QbzSession.trRev), "icon": "x", "danger": false, "needsSelection": true }
                        ]
                        onAction: function (id) { root.bulkAction(id) }
                    }

                    // Toolbar — quality badge + track search (+ inert select).
                    Row {
                        visible: !QbzAlbum.albumLoading
                        width: parent.width
                        height: 52
                        spacing: 16
                        // AlbumPageView.slint:692 mounts QualityBadgeFull —
                        // the contained chip (format mark + tier label over
                        // the exact bit-depth/rate line), NOT a loose mark
                        // plus a plain "16-bit / 44.1 kHz" string. The 1:1
                        // control already exists; this drew its own.
                        QualityBadgeFull {
                            id: qualityRow
                            anchors.verticalCenter: parent.verticalCenter
                            tier: header.qualityTier || ""
                            detail: header.qualityDetail || ""
                        }
                        // Clamped, and the badge slot only counts when the
                        // badge is actually there (QualityBadgeFull hides
                        // itself on an empty tier — an unclamped negative
                        // width is a silent layout trap).
                        Item {
                            width: Math.max(0, parent.width
                                - (qualityRow.visible ? qualityRow.width + 16 : 0)
                                - 168 - 30 - 2 * 16)
                            height: 1
                        }
                        Rectangle {
                            width: 168
                            height: 34
                            radius: 6
                            anchors.verticalCenter: parent.verticalCenter
                            color: theme.surfaceElevated
                            border.width: 1
                            border.color: theme.borderSubtle
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 10
                                anchors.rightMargin: 10
                                spacing: 7
                                QbzIcon {
                                    name: "search"
                                    width: 14
                                    height: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: "muted"
                                }
                                TextInput {
                                    width: parent.width - 21
                                    height: parent.height
                                    color: theme.textPrimary
                                    font.pixelSize: 13
                                    verticalAlignment: Text.AlignVCenter
                                    clip: true
                                    onTextEdited: root.trackQuery = text
                                    Text {
                                        visible: parent.text === ""
                                        anchors.fill: parent
                                        text: QbzSession.tr("Search tracks...", QbzSession.trRev)
                                        color: theme.textMuted
                                        font.pixelSize: 13
                                        verticalAlignment: Text.AlignVCenter
                                    }
                                }
                            }
                        }
                        // Multi-select toggle (AlbumPageView.slint:752-787):
                        // accent border + tint while active; leaving the mode
                        // clears the selection.
                        Rectangle {
                            width: 30
                            height: 30
                            radius: 6
                            anchors.verticalCenter: parent.verticalCenter
                            color: selectToggleArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                            border.width: 1
                            border.color: root.multiSelect ? theme.accent : theme.borderSubtle
                            QbzIcon {
                                name: "square-check-big"
                                width: 15
                                height: 15
                                anchors.centerIn: parent
                                tintName: root.multiSelect ? "accent" : "secondary"
                            }
                            MouseArea {
                                id: selectToggleArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: root.setMultiSelect(!root.multiSelect)
                            }
                        }
                    }

                    // Column header — rows/TrackListHeader.qml, i.e. the SAME
                    // rows/TrackCols.qml geometry the TrackRows below use.
                    //
                    // What was here disagreed with the rows on every number:
                    // spacing 16 vs the row's 14, Duration 80 vs 70, Quality
                    // 80 vs 92, and a title width that counted five gaps
                    // where the layout draws six. Those are Slint's own
                    // header numbers (AlbumPageView.slint:811-880) and they
                    // do NOT match primitives/TrackRow.slint — the reference
                    // has the same defect the owner reported here, so the
                    // port keeps the row's numbers and drops the second
                    // hardcoded copy entirely.
                    //
                    // The heart / cloud glyphs stay (they are the only
                    // labelling those two columns get) — and they still fill
                    // the band so `centerIn` centres them, which is the fix
                    // this block used to document at length.
                    TrackListHeader {
                        visible: !QbzAlbum.albumLoading
                        width: parent.width
                        bandHeight: 40
                        labelSpacing: 0.5
                        showDownload: true
                        favoriteGlyph: true
                        downloadGlyph: true
                    }

                    // Rows (with Disc / work headers).
                    Repeater {
                        model: root.visibleTracks
                        delegate: Column {
                            required property var modelData
                            required property int index
                            property var hdr: root.headerFor(index)
                            width: parent ? parent.width : 0

                            Rectangle {
                                visible: hdr && hdr.disc > 0
                                width: parent.width
                                height: 40
                                color: "transparent"
                                Text {
                                    anchors.left: parent.left
                                    anchors.leftMargin: 12
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: QbzSession.tr("Disc", QbzSession.trRev) + " " + hdr.disc
                                    color: theme.textMuted
                                    font.pixelSize: 13
                                    font.weight: theme.weightSemibold
                                    font.letterSpacing: 0.5
                                }
                            }
                            Row {
                                visible: hdr && hdr.work !== ""
                                width: parent.width
                                leftPadding: 12
                                rightPadding: 12
                                topPadding: 14
                                bottomPadding: 4
                                spacing: 0
                                Text {
                                    text: hdr.work
                                    color: theme.textPrimary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                }
                                Text {
                                    visible: modelData.workComposerName !== ""
                                    text: " ("
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                }
                                Text {
                                    visible: modelData.workComposerName !== ""
                                    text: modelData.workComposerName
                                    color: composerArea.containsMouse && modelData.workComposerId !== "" ? theme.textPrimary : theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                    MouseArea {
                                        id: composerArea
                                        anchors.fill: parent
                                        enabled: modelData.workComposerId !== ""
                                        hoverEnabled: true
                                        cursorShape: modelData.workComposerId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                                        onClicked: QbzArtist.openArtist(modelData.workComposerId)
                                    }
                                }
                                Text {
                                    visible: modelData.workComposerName !== ""
                                    text: ")"
                                    color: theme.textSecondary
                                    font.pixelSize: theme.fontBody
                                    font.weight: theme.weightBold
                                }
                            }
                            TrackRow {
                                item: modelData
                                number: index + 1
                                zebra: true
                                clickPlays: false
                                artistLink: true
                                qualityStyle: "text"
                                showDownload: true
                                downloadGlyph: true
                                selectMode: root.multiSelect
                                checked: root.selected[item.id] === true
                                onToggleSelect: root.toggleSelected(item.id)
                                menuShowLater: false
                                menuShowGoTo: false
                                onPlayRequested: QbzPlayer.playAlbumFrom(header.id, item.id)
                                onEnqueueRequested: function (m) {
                                    QbzPlayer.enqueueAlbumTrack(header.id, item.id, m === "next" ? "next" : "later")
                                }
                                // MyQBZ "Add to mixtape" — the HOST builds the
                                // AddItem array (TrackRow does not know
                                // itemType/source).
                                //
                                // SOURCE: this view never shows a local album.
                                // `open_album` (main.rs:538-548) routes any id
                                // that `is_local_feed_id("album", …)` accepts
                                // — a Plex `plex:` key, a group key, a path —
                                // to the LocalAlbum view instead, so what
                                // reaches `album_qt::TrackRow` here is always
                                // a `/album/get` response. `item.id` is a
                                // Qobuz catalog id by construction of the
                                // route, not by assumption at this call site.
                                onMixtapeRequested: QbzMyQbzAdd.open(JSON.stringify([{
                                    "itemType": "track", "source": "qobuz",
                                    "sourceItemId": item.id, "title": item.title || "",
                                    "subtitle": item.artist || "", "artworkUrl": "",
                                    "year": null, "trackCount": null
                                }]))
                            }
                        }
                    }
                }

                // Label / awards / external-links sidebar (200px).
                Column {
                    visible: root.hasSidebar
                    width: 200
                    spacing: 24

                    Column {
                        visible: (header.label || "") !== ""
                        width: parent.width
                        spacing: 8
                        SidebarHeading { text: QbzSession.tr("LABEL", QbzSession.trRev) }
                        SidebarCard {
                            name: header.label || ""
                            iconName: "disc"
                            gradA: "#6366f1"
                            gradB: "#8b5cf6"
                            // The label page HAS existed for a long time — the
                            // header chip above (:691) has been opening it all
                            // along. This card carried a "no label view yet"
                            // note and no handler, so the sidebar's LABEL entry
                            // looked clickable (it hovers) and did nothing.
                            onClicked: if ((header.labelId || "") !== "")
                                QbzHome.openLabel(header.labelId)
                        }
                    }
                    Column {
                        visible: awards.length > 0
                        width: parent.width
                        spacing: 8
                        SidebarHeading { text: QbzSession.tr("AWARDS", QbzSession.trRev) }
                        Repeater {
                            model: awards
                            delegate: SidebarCard {
                                required property var modelData
                                name: modelData[1]
                                iconName: "award"
                                gradA: "#b45309"
                                gradB: "#eab308"
                                // `modelData` is the (id, name) pair
                                // album_qt publishes. Qobuz omits the id on
                                // some /album/get entries, so with one we open
                                // directly and without one we hand the NAME to
                                // the resolver — 1:1 with the reference's
                                // "open" / "resolve-open" split
                                // (AlbumPageView.slint:1049-1055).
                                onClicked: {
                                    if ((modelData[0] || "") !== "")
                                        QbzHome.openAward(modelData[0], modelData[1] || "")
                                    else if ((modelData[1] || "") !== "")
                                        QbzHome.openAwardByName(modelData[1])
                                }
                            }
                        }
                    }

                    // EXTERNAL LINKS — Last.fm / Discogs / MusicBrainz deep
                    // links for this release. Present whenever the album has
                    // an artist and a title; they are ordinary web URLs, so
                    // they neither require nor touch a connected integration.
                    Column {
                        visible: header.showExternalLinks === true
                        width: parent.width
                        spacing: 8
                        SidebarHeading { text: QbzSession.tr("EXTERNAL LINKS", QbzSession.trRev) }
                        Row {
                            spacing: 8
                            BrandLink {
                                visible: (header.lastfmUrl || "") !== ""
                                iconSource: root.brandDir + "brand-lastfm.svg"
                                name: "Last.fm"
                                url: header.lastfmUrl || ""
                            }
                            BrandLink {
                                visible: (header.discogsUrl || "") !== ""
                                iconSource: root.brandDir + "brand-discogs.svg"
                                name: "Discogs"
                                url: header.discogsUrl || ""
                            }
                            BrandLink {
                                visible: (header.musicbrainzUrl || "") !== ""
                                iconSource: root.brandDir + "brand-musicbrainz.svg"
                                name: "MusicBrainz"
                                url: header.musicbrainzUrl || ""
                            }
                        }
                    }
                }
            }

            // --- Bottom carousels ---------------------------------------
            // Each one is deferred and paints the moment ITS fetch lands
            // (album_qt.rs spawn_deferred_rows). While a fetch is out the
            // rail's placeholder holds its band; when it resolves to nothing
            // both the placeholder and the rail are gone — no empty frame.
            //
            // WIDTH — every rail below spells `parent.width - 64` on purpose.
            // `page` is a Column with leftPadding/rightPadding 32, and a QML
            // Column's padding does NOT narrow `parent.width` for its
            // children: it only shifts them. A child that says
            // `width: parent.width` therefore comes out 64px too wide and
            // hangs 32px off the right edge — which put SectionRail's
            // right-aligned chevron Row entirely outside the viewport (owner
            // report: the bottom rails had no right chevron at all, and the
            // left one sat jammed against the window edge). The inset is
            // applied at the MOUNT SITES, not inside SectionRail /
            // RailSkeleton, exactly as the page's other direct children
            // already do it — see :442, :480, :690 and :695.
            Item { visible: moreRail.visible || moreRailSk.visible; width: 1; height: 40 }
            RailSkeleton {
                id: moreRailSk
                width: parent.width - 64
                visible: root.moreLoading
                phase: skeletonPhase.on
                cardCount: root.railSkeletonCount
            }
            SectionRail {
                id: moreRail
                width: parent.width - 64
                visible: (album.moreFromArtist || []).length > 0
                title: QbzSession.tr("From the same artist", QbzSession.trRev)
                items: album.moreFromArtist || []
                coverMap: root.coverMap
                // album/AlbumPageView.slint:1124-1132 — this rail is the ONE
                // carousel on the album page with `show-view-all: true`, and
                // its link is the SECOND door to the artist discography page
                // (main.rs:11839-11865 handles it with release_type "album" and
                // the name taken from AlbumState.artist, which is why
                // openReleases takes the name as a parameter). The other door
                // is "See discography" on the artist page itself.
                showViewAll: true
                onViewAllClicked: QbzArtist.openReleases(root.header.artistId || "",
                    root.header.artist || "", "album")
            }

            Item { visible: sugRail.visible || sugRailSk.visible; width: 1; height: 40 }
            RailSkeleton {
                id: sugRailSk
                width: parent.width - 64
                visible: root.suggestionsLoading
                phase: skeletonPhase.on
                cardCount: root.railSkeletonCount
            }
            SectionRail {
                id: sugRail
                width: parent.width - 64
                visible: (album.suggestions || []).length > 0
                title: QbzSession.tr("Listening suggestions", QbzSession.trRev)
                items: album.suggestions || []
                coverMap: root.coverMap
            }

            // Last.fm row. Absent — not empty, and not even a placeholder —
            // when Last.fm is not connected: album_qt.rs seeds similarLoading
            // false in that case and makes no network call. Reuses the same
            // delegate as the two Qobuz rows rather than a fourth card variant.
            Item { visible: simRail.visible || simRailSk.visible; width: 1; height: 40 }
            RailSkeleton {
                id: simRailSk
                width: parent.width - 64
                visible: root.similarLoading
                phase: skeletonPhase.on
                cardCount: root.railSkeletonCount
            }
            SectionRail {
                id: simRail
                width: parent.width - 64
                visible: (album.similarAlbums || []).length > 0
                title: QbzSession.tr("Similar albums", QbzSession.trRev)
                items: album.similarAlbums || []
                coverMap: root.coverMap
            }
        }
    }

    // Thin auto-hiding scrollbar (ListScrollbar).
    QbzScrollBar {
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        target: pageFlick
    }

    // Best available cover source for the lightbox: the custom override
    // file when set, else the remote best() URL (mega → … → large).
    function bestCoverSource() {
        var custom = header.customCoverPath || ""
        if (custom !== "") return "file://" + custom
        return header.artUrl || ""
    }

    // The cover menu's rows, rebuilt per open (Add vs Change+Remove flips
    // on hasCustomCover; "View cover" is this port's addition).
    function buildCoverMenuModel() {
        var rows = []
        if (header.hasCustomCover === true) {
            rows.push({ "label": QbzSession.tr("Change cover", QbzSession.trRev), "icon": "image-plus", "action": "add" })
            rows.push({ "label": QbzSession.tr("Remove cover", QbzSession.trRev), "icon": "trash-2", "action": "remove" })
        } else {
            rows.push({ "label": QbzSession.tr("Add cover", QbzSession.trRev), "icon": "image-plus", "action": "add" })
        }
        rows.push({ "label": QbzSession.tr("View cover", QbzSession.trRev), "icon": "eye", "action": "view" })
        rows.push({ "label": QbzSession.tr("Open in browser", QbzSession.trRev), "icon": "external-link", "action": "browser" })
        rows.push({ "label": QbzSession.tr("Save as…", QbzSession.trRev), "icon": "cloud-download", "action": "save" })
        return rows
    }

    // Cover right-click menu (AlbumPageView.slint:300-370) + "View cover",
    // the lightbox entry that is NEW in this port.
    QbzContextMenu {
        id: coverMenu
        menuWidth: 196
        // Rebuilt on every open so Add/Change/Remove track the live flag.
        onAboutToShow: coverMenuRepeater.model = root.buildCoverMenuModel()
        Repeater {
            id: coverMenuRepeater
            model: []
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
                    onClicked: {
                        coverMenu.close()
                        var a = modelData.action
                        if (a === "add") QbzAlbum.coverAddCustom(header.id, header.artUrl)
                        else if (a === "remove") QbzAlbum.coverRemoveCustom(header.id)
                        else if (a === "view") coverLightbox.openWith(root.bestCoverSource())
                        else if (a === "browser") QbzShell.openExternalUrl(header.artUrl)
                        else if (a === "save") QbzAlbum.coverSaveAs(header.id, header.title, header.artUrl)
                    }
                }
            }
        }
    }

    CoverLightbox { id: coverLightbox }

    // Album Info (Credits/Review) modal — mounted by its host view, the
    // TrackInfoModal pattern (one AlbumView instance exists at a time; the
    // Popup reparents to the window overlay).
    AlbumInfoModal { id: albumInfo }

    // Album ⋯ menu (AlbumContextMenu subset — "Play later" as a distinct
    // block-tail insert has no mode in playback_qt::enqueue_album (next /
    // append only), so "Add to queue" covers the append arm. The offline
    // row swaps on the live all-cached aggregate).
    QbzContextMenu {
        id: albumMenu
        menuWidth: 196
            Repeater {
                model: [
                    { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                    { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-plus", "action": "next" },
                    { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
                    { "label": root.toggleState("album", header.isFavorite) ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev), "icon": root.toggleState("album", header.isFavorite) ? "heart-filled" : "heart", "action": "favorite" },
                    { "label": root.toggleState("pin", header.isPinned) ? QbzSession.tr("Unpin", QbzSession.trRev) : QbzSession.tr("Pin", QbzSession.trRev), "icon": root.toggleState("pin", header.isPinned) ? "pin-filled" : "pin", "action": "pin" },
                    { "label": QbzSession.tr("Add to playlist", QbzSession.trRev), "icon": "list-music", "action": "playlist" },
                    { "label": QbzSession.tr("Add to mixtape", QbzSession.trRev), "icon": "cassette-tape", "action": "mixtape" },
                    { "label": QbzSession.tr("Share Qobuz link", QbzSession.trRev), "icon": "link", "action": "share-qobuz" },
                    { "label": QbzSession.tr("Share Album.link", QbzSession.trRev), "icon": "link", "action": "share-albumlink" },
                    // AlbumContextMenu.slint:137-152 — the offline row swaps
                    // on the all-cached aggregate (live from row flips).
                    { "label": root.fullyCachedLive ? QbzSession.tr("Refresh offline copy", QbzSession.trRev) : QbzSession.tr("Make available offline", QbzSession.trRev), "icon": root.fullyCachedLive ? "refresh-cw" : "cloud-download", "action": root.fullyCachedLive ? "recache-album" : "cache-album" },
                ]
                delegate: Rectangle {
                    required property var modelData
                    width: parent ? parent.width : 0
                    height: 33
                    radius: 5
                    color: amiArea.containsMouse ? theme.surfaceHover : "transparent"
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
                        id: amiArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            albumMenu.close()
                            var a = modelData.action
                            if (a === "play") QbzPlayer.playAlbum(header.id)
                            else if (a === "next") QbzPlayer.enqueueAlbum(header.id, "next")
                            else if (a === "queue") QbzPlayer.enqueueAlbum(header.id, "later")
                            else if (a === "favorite") {
                                root.setToggleState("album", !root.toggleState("album", header.isFavorite))
                                QbzLibrary.libraryToggleFavorite("album", header.id)
                            } else if (a === "pin") {
                                root.setToggleState("pin", !root.toggleState("pin", header.isPinned))
                                QbzLibrary.togglePin("album", header.id, header.title, header.artist, header.artUrl)
                            } else if (a === "playlist") {
                                // The loaded view's own track ids (Slint
                                // main.rs:12213 collects AlbumState.tracks).
                                var ids = []
                                var ts = root.album.tracks || []
                                for (var i = 0; i < ts.length; i++) {
                                    if (/^\d+$/.test(ts[i].id || "")) ids.push(ts[i].id)
                                }
                                if (ids.length > 0) QbzPlaylistPicker.openForTracks(JSON.stringify(ids))
                            } else if (a === "mixtape") {
                                QbzAlbum.addToMixtape(header.id)
                            } else if (a === "share-qobuz") {
                                QbzAlbum.shareQobuzLink(header.id)
                            } else if (a === "share-albumlink") {
                                QbzAlbum.shareAlbumLink(header.id)
                            } else if (a === "cache-album") {
                                QbzAlbum.albumCacheOffline(header.id)
                            } else if (a === "recache-album") {
                                QbzAlbum.albumRefreshOffline(header.id)
                            }
                        }
                    }
                }
            }

            // AlbumContextMenu.slint:153 — a 1px border-subtle separator, then
            // the album-blacklist toggle (:157-172). The .slint writes it as two
            // `if` arms (Block / Unblock) whose row count is constant; one row
            // with a flipping label is the same single row, and it is the shape
            // ArtistPageView.slint:561-572 uses for the identical toggle. Own
            // Rectangle rather than a sixth Repeater entry because the inline
            // delegate above has no separator arm — `{sep:true}` is CardMenu's
            // vocabulary, not this hand-rolled menu's (ArtistView.qml's
            // overflow menu draws its blacklist row exactly this way).
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
            Rectangle {
                id: ablkRow
                width: parent.width
                height: 33
                radius: 5
                // Seeded from the header document, which does NOT carry the
                // field yet (album_qt.rs still has to port album.rs:683's
                // `set_is_album_blocked` seed — spec 03 F13). Read defensively:
                // `undefined` folds to false through toggleState's
                // `fallback === true`, so the row is correct the moment the
                // seed lands and never throws before then.
                readonly property bool blocked: root.toggleState("blocked", header.isAlbumBlocked)
                color: ablkArea.containsMouse ? theme.surfaceHover : "transparent"
                Row {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    spacing: 8
                    QbzIcon { name: "blind-eye"; width: 15; height: 15; anchors.verticalCenter: parent.verticalCenter; tintName: "secondary" }
                    Text {
                        height: parent.height
                        width: parent.width - 23
                        text: ablkRow.blocked
                            ? QbzSession.tr("Unblock album", QbzSession.trRev)
                            : QbzSession.tr("Block this album", QbzSession.trRev)
                        color: theme.textSecondary
                        font.pixelSize: 13
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
                MouseArea {
                    id: ablkArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        albumMenu.close()
                        // Optimistic flip (main.rs:12835), then the mutation;
                        // `blacklistChanged` above settles it — or rolls it
                        // back on a write failure (main.rs:12859).
                        root.setToggleState("blocked", !ablkRow.blocked)
                        QbzBlacklist.albumToggle(header.id, header.title,
                            header.artist, header.artUrl)
                    }
                }
            }
        }
}
