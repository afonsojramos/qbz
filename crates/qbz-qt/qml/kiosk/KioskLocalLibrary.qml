// KioskLocalLibrary — the kiosk Local Library host.
//
// ── WHAT CHANGED AND WHY ──────────────────────────────────────────────────
// The K0 audit found this view reading LEGACY JSON documents while Full UI had
// already switched the same surfaces to the paged native models. That is not a
// styling difference, it is why the panel showed empty Artists and Tracks tabs
// on a machine whose library is fine: `local_albums_json` / `local_artists_json`
// / `local_tracks_json` are published by the legacy readers only, and once
// `albums/artists/tracks_native_active` is true (the production default) Rust
// stops republishing them. It also mounted `Repeater`s over the whole document
// with a Loader per row — 20 416 items and 10 053 Loaders on the Albums tab at
// the 10 000-album fixture, 1.19 s to idle; 20 487 / 10 075 on Artists at
// 1.87 s (evidence/runtime-baseline/partial-initial.jsonl).
//
// This file is now only a HOST. It owns:
//   1. the tab set, its default and its history;
//   2. the four legacy documents (still the authority for Folders, Genres and
//      as the automatic fallback when a catalog session fails);
//   3. the artwork window REGISTRY — the one policy that cannot be split per
//      surface, because eviction is only correct against every live surface at
//      once (the desktop `_windows` finding, reproduced);
//   4. the nav geometry it publishes into QbzKioskNav.
// Every tab body is its own file, mounted behind `Loader.active`, and each one
// consumes the authoritative reader for its surface.
//
// ── DEFAULT TAB ───────────────────────────────────────────────────────────
// Albums, always — contract §2.2. A fresh mount, a NavRail entry and a
// programmatic navigation with no tab all land on Albums, and the persisted
// tab ORDER is deliberately not consulted for the default: it may put Tracks
// first, and three different defaults in shell, settings and view is the
// defect that rule exists to close. Back/Forward is the exception and the
// point: a restored entry keeps the tab that entry actually recorded, Tracks
// included — the past is not rewritten.
//
// ── HISTORY ───────────────────────────────────────────────────────────────
// There is no second stack. `KioskNavigation` writes this view's opaque state
// onto the ONE `QbzShell` history, records a tab destination through
// `recordKioskTab` BEFORE the tab changes (so the entry that is pushed carries
// the OUTGOING state) and restores it on the way back.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    color: "transparent"

    QbzTheme { id: theme }

    function t(s) { return QbzSession.tr(s, QbzSession.trRev) }

    /// Content inset. 16 keeps a card clear of the NavRail at 800px.
    property real pad: 16


    // =====================================================================
    // Tabs
    // =====================================================================
    readonly property var knownTabs: ["genres", "albums", "artists", "folders", "tracks"]

    /// The user's own tab order drives the STRIP. It deliberately does not
    /// drive the DEFAULT (see the header).
    readonly property var orderedTabs: {
        var stored
        try {
            stored = JSON.parse(QbzBridge.settingsJson).localTabOrder
        } catch (e) {
            stored = null
        }
        var out = []
        var i
        if (Array.isArray(stored)) {
            for (i = 0; i < stored.length; i++)
                if (root.knownTabs.indexOf(stored[i]) >= 0 && out.indexOf(stored[i]) < 0)
                    out.push(stored[i])
        }
        // Anything the stored order omits still has to be reachable.
        for (i = 0; i < root.knownTabs.length; i++)
            if (out.indexOf(root.knownTabs[i]) < 0)
                out.push(root.knownTabs[i])
        return out
    }

    readonly property bool ephemeralActive: QbzLocal.localEphemeralActive

    /// The strip's contents. The open session appends its own tab, exactly as
    /// it does on the desktop, and it is the only tab that can VANISH while
    /// the user is standing on it.
    readonly property var visibleTabs: root.ephemeralActive
        ? root.orderedTabs.concat(["ephemeral"]) : root.orderedTabs

    function tabLabel(id) {
        if (id === "genres") return root.t("Genres")
        if (id === "albums") return root.t("Albums")
        if (id === "artists") return root.t("Artists")
        if (id === "folders") return root.t("Folders")
        if (id === "tracks") return root.t("Tracks")
        // The open session names itself; Rust computes the label so this view
        // and the nav flyout cannot call the same thing two different things.
        return QbzLocal.localEphemeralLabel !== ""
            ? QbzLocal.localEphemeralLabel : root.t("Open")
    }

    /// ALBUMS. Not `orderedTabs[0]`.
    property string activeTab: "albums"

    /// Opaque per-tab state carried on the shared history entry.
    property string selectedArtist: ""
    property string selectedGenre: ""

    function tabAvailable(tab) {
        if (tab === "ephemeral")
            return root.ephemeralActive
        return root.knownTabs.indexOf(tab) >= 0
    }

    /// THE recording setter. Everything that represents a user destination —
    /// the strip, the router handshake, `open ephemeral` — goes through it.
    function activateTab(tab) {
        if (!tab || tab === root.activeTab || !root.tabAvailable(tab))
            return
        // BEFORE the assignment: the entry that gets pushed has to carry the
        // state of the tab being LEFT, which is what Back restores.
        nav.recordTab(tab)
        root.activeTab = tab
    }

    /// A tab change that is NOT a destination: the ephemeral fallback, and the
    /// restoration of a history entry. Neither may push an entry.
    function setTabSilently(tab) {
        if (!tab || tab === root.activeTab)
            return
        root.activeTab = tab
    }

    // The router's ONE external tab seam (NavFlyout, the kiosk NavRail through
    // `navigateToTab`, and `open ephemeral`). ContentRouter writes this
    // property; `sequence` makes re-selecting the same tab re-apply.
    property var tabNavigationRequest: ({})
    onTabNavigationRequestChanged: {
        if (root.tabNavigationRequest && root.tabNavigationRequest.tab)
            root.activateTab(root.tabNavigationRequest.tab)
    }

    function selectArtist(name) { root.selectedArtist = name || "" }
    function selectGenre(key) { root.selectedGenre = key || "" }

    /// A local album card opens the routed local album page. Both halves are
    /// required: `openAlbum` only LOADS the document — the old kiosk called it
    /// alone, so a card tap loaded an album nobody ever navigated to and read
    /// as a dead control.
    function openAlbum(id) {
        if (!id)
            return
        QbzLocal.openAlbum(id)
        QbzShell.navigateTo("localalbum")
    }

    function loadActiveTab() { QbzLocal.loadTab(root.activeTab) }

    onActiveTabChanged: {
        root.loadActiveTab()
        root.publishNav(root._navColumns, 0)
        root.artworkRefresh()
    }

    /// The open session is the one tab that can disappear under the user (the
    /// disc is ejected, the folder is closed). Falling back to Albums is the
    /// desktop lifecycle, and it is SILENT: closing a session is not a
    /// navigation, and Back must not be able to resurrect it.
    onEphemeralActiveChanged: {
        if (!root.ephemeralActive && root.activeTab === "ephemeral")
            root.setTabSilently("albums")
        root.publishNav(root._navColumns, root._navItems)
    }

    Component.onCompleted: {
        // KioskNavigation has already completed (children complete first), so
        // a restored entry has set `activeTab` by now and this is its load.
        root.loadActiveTab()
        root.publishNav(1, 0)
    }

    // =====================================================================
    // History — one stack, this view's opaque state on it
    // =====================================================================
    KioskNavigation {
        id: nav
        route: "local"
        snapshot: ({
            "activeTab": root.activeTab,
            "selectedArtist": root.selectedArtist,
            "selectedGenre": root.selectedGenre
        })
        onRestore: function (saved) {
            if (!saved)
                return
            var tab = typeof saved.activeTab === "string" ? saved.activeTab : root.activeTab
            // NO RESURRECTION: an entry recorded while a session was open must
            // not reopen a session that has since been closed.
            if (!root.tabAvailable(tab))
                tab = "albums"
            root.activeTab = tab
            root.selectedArtist = typeof saved.selectedArtist === "string"
                ? saved.selectedArtist : ""
            root.selectedGenre = typeof saved.selectedGenre === "string"
                ? saved.selectedGenre : ""
        }
    }

    // =====================================================================
    // Nav geometry (QbzKioskNav)
    // =====================================================================
    // The mounted tab body is the only thing that knows its own column count
    // and item count, so it publishes them through here. The leading `tabs`
    // entries are the strip.
    property int _navColumns: 1
    property int _navItems: 0
    function publishNav(columns, items) {
        root._navColumns = Math.max(1, columns || 1)
        root._navItems = Math.max(0, items || 0)
        // `visibleTabs` is a derived binding, and an early publish (the
        // ephemeral flag arriving during construction) can reach here before
        // it has been evaluated. A geometry publish is not worth a TypeError.
        var tabs = root.visibleTabs ? root.visibleTabs.length : 0
        if (tabs === 0)
            return
        QbzKioskNav.publishNav(tabs, root._navColumns, tabs + root._navItems, false)
    }

    // Enter on a tab entry drives the same switch a tap does. The
    // `index < tabs` test is what keeps this handler and the mounted body's
    // disjoint: both see the same pulse.
    Connections {
        target: QbzKioskNav
        function onActivateSeqChanged() {
            if (!QbzKioskNav.navActive || QbzKioskNav.zone !== "content")
                return
            if (QbzKioskNav.index < 0 || QbzKioskNav.index >= QbzKioskNav.tabs)
                return
            var id = root.visibleTabs[QbzKioskNav.index]
            if (id !== undefined)
                root.activateTab(id)
        }
    }

    // =====================================================================
    // Legacy documents
    // =====================================================================
    // Still authoritative for Folders and for the Genres browser (whose
    // `load_tab("genres")` arm deliberately routes to `load_albums_legacy`),
    // and still the automatic fallback for Albums/Artists/Tracks when a
    // catalog session fails. A tab body reads these ONLY when its native
    // reader is inactive.
    function parseDoc(json, fallback) {
        if (json === "")
            return fallback
        try {
            return JSON.parse(json)
        } catch (e) {
            console.warn("[qbz-qt] kiosk local: bad document — " + e)
            return fallback
        }
    }
    readonly property var albums: ((root.activeTab === "albums" && !QbzLocal.localAlbumsNativeActive) || root.activeTab === "genres" || (root.activeTab === "artists" && !QbzLocal.localArtistsNativeActive)) ? root.parseDoc(QbzLocal.localAlbumsJson, []) : []
    readonly property var artists: (root.activeTab === "artists" && !QbzLocal.localArtistsNativeActive) ? root.parseDoc(QbzLocal.localArtistsJson, []) : []
    readonly property var folders: (root.activeTab === "folders") ? root.parseDoc(QbzLocal.localFoldersJson, []) : []
    readonly property var tracks: (root.activeTab === "tracks" && !QbzLocal.localTracksNativeActive) ? root.parseDoc(QbzLocal.localTracksJson, []) : []
    readonly property var ephemeral: (root.activeTab === "ephemeral") ? root.parseDoc(QbzLocal.localEphemeralJson, null) : null

    readonly property bool trackArtwork: QbzLocal.localTrackArtwork

    // =====================================================================
    // Artwork window registry
    // =====================================================================
    // Covers are id-keyed: a surface reports the artKeys of the rows it has
    // MOUNTED, Rust answers one `localArtworkReady` per resolved key, and the
    // union of every live window plus one window of margin is what survives
    // eviction. It lives here rather than per tab because two cover surfaces
    // can be alive at once (the Artists list and its drill-down grid), and a
    // per-surface keep-set makes each one delete the other's covers.
    property var artMap: ({})
    property var _artInbox: ({})
    property var _windows: ({})
    property var _pending: ({})
    // `real`, not `int`: Date.now() is ~1.7e12.
    property real _lastReportMs: 0

    signal artworkRefresh()

    function artPathOf(key) { return root.artMap[key] || "" }
    function artWanted(key) { return (key || "") !== "" }

    // Arrivals stream in one at a time. Rebinding `artMap` per arrival is
    // quadratic in the window, so they are coalesced into one rebind per
    // frame — the covers still appear progressively at 16ms granularity.
    Timer {
        id: artFlush
        interval: 16
        repeat: false
        onTriggered: {
            var next = Object.assign({}, root.artMap, root._artInbox)
            root._artInbox = ({})
            // A rebind needs a NEW object reference: a same-ref assignment is
            // not a change in QML.
            root.artMap = next
        }
    }

    Connections {
        target: QbzLocal
        function onLocalArtworkReady(key, path) {
            root._artInbox[key] = path
            if (!artFlush.running)
                artFlush.start()
        }
    }

    function applyWindow(key, rows, first, last) {
        if (!rows || rows.length === 0) {
            delete root._windows[key]
            return
        }
        last = Math.min(last, rows.length - 1)
        first = Math.max(0, first)
        if (first > last) {
            delete root._windows[key]
            return
        }
        root._windows[key] = { "rows": rows, "first": first, "last": last }
    }

    /// A surface that unmounts, scrolls off or has nothing to show stops
    /// holding its covers.
    function releaseWindow(key) {
        var k = key || "default"
        if (root._windows[k] === undefined && root._pending[k] === undefined)
            return
        delete root._windows[k]
        delete root._pending[k]
        root.flushWindows()
    }

    /// Evict against the UNION of every live window, then request what is
    /// still missing. A key already resolved is never re-sent: `artMap` IS the
    /// resolved set and a re-request costs Rust a stat per key.
    function flushWindows() {
        var keep = ({})
        var k, w, rows, i, ak, span, lo, hi
        for (k in root._windows) {
            w = root._windows[k]
            rows = w.rows
            span = w.last - w.first + 1
            lo = Math.max(0, w.first - span)
            hi = Math.min(rows.length - 1, w.last + span)
            for (i = lo; i <= hi; i++) {
                ak = rows[i] ? rows[i].artKey : ""
                if (ak)
                    keep[ak] = true
            }
        }
        var map = root.artMap
        var changed = false
        for (k in map) {
            if (!keep[k]) {
                delete map[k]
                changed = true
            }
        }
        // Evict the not-yet-flushed arrivals too, or a cover that landed for a
        // row we have just scrolled past would be re-added by the next flush.
        for (k in root._artInbox)
            if (!keep[k])
                delete root._artInbox[k]
        if (changed)
            root.artMap = Object.assign({}, map)

        var missing = []
        var seen = ({})
        for (k in root._windows) {
            w = root._windows[k]
            rows = w.rows
            for (i = w.first; i <= w.last; i++) {
                ak = rows[i] ? rows[i].artKey : ""
                if (!ak || seen[ak])
                    continue
                seen[ak] = true
                if (map[ak] !== undefined || root._artInbox[ak] !== undefined)
                    continue
                missing.push(ak)
            }
        }
        if (missing.length > 0)
            QbzLocal.artworkWindow(JSON.stringify(missing))
    }

    // Rate-limited to one resolution pass per 180ms, but LEADING EDGE: the
    // limiter exists so a flick cannot fire a pass per pixel, and a view that
    // has just mounted is not flicking. Pending reports are keyed by SURFACE,
    // so two surfaces reporting inside the same window both survive.
    Timer {
        id: windowDebounce
        interval: 180
        repeat: false
        onTriggered: {
            root._lastReportMs = Date.now()
            var pending = root._pending
            root._pending = ({})
            var any = false
            for (var k in pending) {
                var w = pending[k]
                root.applyWindow(k, w.rows, w.first, w.last)
                any = true
            }
            if (any)
                root.flushWindows()
        }
    }
    function queueWindowReport(rows, first, last, key) {
        var k = key || "default"
        if (!windowDebounce.running
                && Date.now() - root._lastReportMs >= windowDebounce.interval) {
            root._lastReportMs = Date.now()
            root.applyWindow(k, rows, first, last)
            root.flushWindows()
        } else {
            root._pending[k] = { "rows": rows, "first": first, "last": last }
            if (!windowDebounce.running)
                windowDebounce.start()
        }
    }

    // =====================================================================
    // Tab strip
    // =====================================================================
    // 64px — the contract's primary touch target — and the whole cell is the
    // hit area, not the text's bounding box. The strip scrolls horizontally so
    // a six-tab set with a long session name stays reachable at 800px.
    Rectangle {
        id: tabStrip

        anchors.left: root.left
        anchors.right: root.right
        anchors.top: root.top
        height: 64
        color: theme.surfaceMain

        Flickable {
            id: tabScroll
            anchors.fill: parent
            anchors.leftMargin: root.pad
            anchors.rightMargin: root.pad
            contentWidth: tabsRow.width
            contentHeight: height
            clip: true
            flickableDirection: Flickable.HorizontalFlick
            boundsBehavior: Flickable.StopAtBounds

            Row {
                id: tabsRow
                height: tabScroll.height
                spacing: 6

                // A FIXED tab list, never a data collection: at most the five
                // ordered tabs plus the open session.
                Repeater {
                    model: root.visibleTabs

                    delegate: Rectangle {
                        id: tab
                        required property string modelData
                        required property int index

                        readonly property bool active: root.activeTab === tab.modelData
                        readonly property bool navFocused: QbzKioskNav.navActive
                            && QbzKioskNav.zone === "content"
                            && QbzKioskNav.index === tab.index

                        width: Math.max(88, tabText.implicitWidth + 28)
                        height: tabsRow.height
                        color: tab.active
                            ? Qt.rgba(theme.accent.r, theme.accent.g, theme.accent.b, 0.10)
                            : "transparent"
                        radius: theme.radiusSm
                        border.width: tab.navFocused ? 2 : 0
                        border.color: theme.accent

                        Text {
                            id: tabText
                            anchors.centerIn: parent
                            width: Math.min(implicitWidth, 220)
                            text: root.tabLabel(tab.modelData)
                            color: tab.active ? theme.textPrimary : theme.textMuted
                            font.pixelSize: 16
                            font.weight: theme.weightSemibold
                            horizontalAlignment: Text.AlignHCenter
                            elide: Text.ElideRight
                            maximumLineCount: 1
                        }

                        Rectangle {
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 6
                            height: 3
                            radius: 2
                            color: tab.active ? theme.accent : "transparent"
                        }

                        MouseArea {
                            anchors.fill: parent
                            onClicked: root.activateTab(tab.modelData)
                        }
                    }
                }
            }
        }

        Rectangle {
            anchors.left: tabStrip.left
            anchors.right: tabStrip.right
            y: tabStrip.height - 1
            height: 1
            color: theme.borderSubtle
        }
    }

    // =====================================================================
    // Content — ONE tab exists at a time
    // =====================================================================
    // `Loader.active`, never `visible: false`: a hidden item still costs what
    // it mounted, and the Tracks body is the one that makes that matter. This
    // is also what keeps the Tracks model out of an Albums visit — nothing
    // subscribes to its page misses and nothing queries it until its own tab
    // is the mounted one.
    Item {
        id: content

        anchors.left: root.left
        anchors.right: root.right
        anchors.top: tabStrip.bottom
        anchors.bottom: root.bottom
        clip: true

        /// The open session renders with no indexed library at all — it is
        /// content from OUTSIDE the index, so gating it on `localAvailable`
        /// would hide the pane from exactly the people the feature is for.
        readonly property bool ephemeralShowing:
            root.activeTab === "ephemeral" && root.ephemeralActive

        KioskEmptyState {
            anchors.fill: parent
            visible: !QbzLocal.localAvailable && !content.ephemeralShowing
            // An EXISTING msgid, present in all eight catalogues — this lane
            // may not add translations, so no new msgid is introduced.
            text: root.t("No folders yet. Add a folder to build your local library.")
        }

        Loader {
            anchors.fill: parent
            active: QbzLocal.localAvailable && root.activeTab === "genres"
            sourceComponent: KioskLocalGenresTab { view: root }
        }
        Loader {
            anchors.fill: parent
            active: QbzLocal.localAvailable && root.activeTab === "albums"
            sourceComponent: KioskLocalAlbumsTab { view: root }
        }
        Loader {
            anchors.fill: parent
            active: QbzLocal.localAvailable && root.activeTab === "artists"
            sourceComponent: KioskLocalArtistsTab { view: root }
        }
        Loader {
            anchors.fill: parent
            active: QbzLocal.localAvailable && root.activeTab === "folders"
            sourceComponent: KioskLocalFoldersTab { view: root }
        }
        Loader {
            anchors.fill: parent
            active: QbzLocal.localAvailable && root.activeTab === "tracks"
            sourceComponent: KioskLocalTracksTab { view: root }
        }
        Loader {
            anchors.fill: parent
            active: content.ephemeralShowing
            sourceComponent: KioskLocalEphemeralTab { view: root }
        }

        // Retry. The kiosk primitive draws an error message and no action, and
        // a route that can only say "it failed" is a route the user has to
        // leave. One affordance for every retryable tab, mounted over the
        // message, 64px tall.
        readonly property string activeError: root.activeTab === "albums"
                ? QbzLocal.localAlbumsError
            : root.activeTab === "artists" && QbzLocal.localArtistsNativeActive ? QbzLocal.localArtistsNativeError
            : root.activeTab === "tracks" && QbzLocal.localTracksNativeActive ? QbzLocal.localTracksNativeError
            : root.activeTab === "genres" ? QbzLocal.localAlbumsError
            : ""

        Rectangle {
            id: retry
            visible: content.activeError !== ""
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 56
            width: Math.max(140, retryText.implicitWidth + 44)
            height: 64
            radius: theme.radiusSm
            color: theme.surfaceElevated
            border.width: 1
            border.color: theme.borderSubtle

            Text {
                id: retryText
                anchors.centerIn: parent
                text: root.t("Retry")
                color: theme.textPrimary
                font.pixelSize: 16
                font.weight: theme.weightSemibold
            }

            MouseArea {
                anchors.fill: parent
                onClicked: root.loadActiveTab()
            }
        }
    }
}
