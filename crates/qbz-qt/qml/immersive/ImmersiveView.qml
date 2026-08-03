// ImmersiveView — the immersive root overlay (ImmersiveView.slint:1186-1705),
// the Qt port of contract 2026-08-02-immersive-port §5.1-§5.5. Mounted by
// AppShell AFTER QbzToast and BEFORE ArtPreviewOverlay/QbzTooltip (§5.1
// declaration-order z convention).
//
// BLOCK B2+B3+B4+B5 SCOPE: the overlay shell (root + click-blocker + chrome +
// auto-hide + exit paths 2-4 + the fullscreen guard + seek arrows) PLUS the
// B3 panels (the ImmersiveAtmosphere underlay, the five FOCUS panels Album
// Reactive / Static / Coverflow / Spectrum / Wave Bed, the layer-4
// ImmersiveSongCard) PLUS the B4 panels: FOCUS Lyrics (mode 4,
// LyricsFocusPanel) and Queue (mode 5, QueueTabsPanel), and the full §5.6
// SPLIT layout (50/50: artwork + ImmersiveTrackMeta LEFT, the Lyrics /
// TrackInfo / Suggestions / Queue panels RIGHT) with the §5.5 entry loads —
// PLUS the B5 layer-5 ImmersivePlayerBar (TinyBar/VolumeBar, §5.7/§5.8) and
// the B5 layer-7 ImmersiveSearchCortinilla (§3.4).
//
// Layer order bottom->top (§5.1):
//   1. root color #0a0a0b, clip (:1199-1200)
//   2. ImmersiveAtmosphere — ALWAYS the underlay (the Slint shader-mode==0
//      gate is constant in v1, ruling 1; :1313-1321)
//   2b. FULL-COVERAGE click-blocker MouseArea (load-bearing — without it
//      clicks pass through to the desktop header search field, the dock's
//      spectrum band and the viz eye toggle; port of the Slint root
//      TouchArea :1283-1286). Stacked BELOW panels + chrome.
//   3. FOCUS panels (viewMode==0 && mode==N) / the SPLIT layout (viewMode==1)
//   4. ImmersiveSongCard (viewMode==0 && mode==6 && npHasTrack — trap 19;
//      does NOT fade with the chrome, :1602-1605)
//   5. ImmersivePlayerBar (B5, :1607-1616) — centered, y = height-90-24,
//      fades with the chrome like the header
//   6. ImmersiveHeader band
//   7. ImmersiveSearchCortinilla (B5, :1695-1705) — topmost

import QtQuick
import QtQuick.Effects
import QtQuick.Window
import com.blitzfc.qbz
import "../controls"
import "../shell"
import "../theme"

Item {
    id: root

    // Self-gated on the bridge open flag (§5.1: closed = invisible,
    // non-interactive, zero-cost). Every open/close transition funnels
    // through QbzImmersive's open setter (§3/D3) — this item only reacts.
    visible: QbzImmersive.open
    clip: true
    // focus: true while open so the root's Keys handlers (Escape, seek
    // arrows) are live; the 30ms grab timer below re-asserts it after mount
    // (:1259-1267).
    focus: QbzImmersive.open

    // §5.4 text-input gate: Shift+I + the seek arrows are inert while ANY
    // text input has focus. The immersive surface's own input is the header
    // search field (D16 seam).
    readonly property bool textInputActive: header.searchActive

    // --- §5.4 fullscreen / shell guard ------------------------------------
    // Captured on the OPEN edge; on the close edge FullScreen is dropped
    // only if the window was NOT fullscreen before (port of
    // main.rs:8660-8716, as QML state — no 150ms timer, D3).
    property bool wasFullscreen: false
    // Kiosk seam: Qt has no kiosk screen yet, so this is always false and
    // the kiosk shell-restore arm is a documented no-op (D4) — the kiosk
    // Loader contract is 2026-08-02-kiosk-port §4.6 (Claude).
    property bool preKiosk: false

    Connections {
        target: QbzImmersive
        function onOpenChanged() {
            var w = root.Window.window
            if (QbzImmersive.open) {
                root.wasFullscreen = w !== null && w.visibility === Window.FullScreen
                root.wake()
                focusGrab.restart()
            } else {
                if (!root.wasFullscreen && !root.preKiosk
                        && w !== null && w.visibility === Window.FullScreen)
                    w.visibility = Window.Windowed
                // Focus hygiene (measured 2026-08-02 over RFB): when the
                // overlay hides while its search field holds activeFocus, Qt
                // leaves activeFocus STRANDED on the now-invisible TextInput —
                // Main.qml's Shift+I gate (activeFocusItem instanceof
                // TextInput) and the seek-arrow textInputActive gate then stay
                // inert forever. Hand focus to the window content root so the
                // gates see a non-text item again.
                if (w !== null)
                    w.contentItem.forceActiveFocus()
            }
        }
    }

    // 30ms focus-grab after mount (:1259-1267) — focusing synchronously
    // inside the open edge races the visibility transition.
    Timer {
        id: focusGrab
        interval: 30
        repeat: false
        onTriggered: root.forceActiveFocus()
    }

    // --- Layer 1: root surface ---------------------------------------------
    Rectangle {
        anchors.fill: parent
        color: "#0a0a0b"
    }

    // --- Layer 2: the atmosphere underlay (§5.1, :1313-1321) ----------------
    // ALWAYS the underlay in v1 (the Slint shader-mode==0 gate is constant,
    // ruling 1). Source = the host-generated 128x128 atmosphere PNG, fallback
    // the plain cover; dim 0.15. `animated` binds npPlaying (:1321); the
    // && open arm stops the drift clock while the overlay is closed (the
    // Slint view is UNMOUNTED then — same zero cost). Spectrum/WaveBed paint
    // opaque #000 over it — intended (§5.1).
    ImmersiveAtmosphere {
        anchors.fill: parent
        source: QbzImmersive.atmosphereUrl
        fallbackSource: QbzPlayer.npArtworkPath
        animated: QbzPlayer.npPlaying && QbzImmersive.open
        dim: 0.15
    }

    // --- Layer 2b: the click-blocker (§5.1, load-bearing) ------------------
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onPressed: function (mouse) { mouse.accepted = true }
        onReleased: function (mouse) { mouse.accepted = true }
        onClicked: function (mouse) { mouse.accepted = true }
        onDoubleClicked: function (mouse) { mouse.accepted = true }
        onWheel: function (wheel) { wheel.accepted = true }
    }

    // --- §5.3 auto-hide / wake ----------------------------------------------
    // hideTimer 6000ms (:1271); hides chrome only when !immSearchOpen &&
    // !pointerInChrome (:1275); pointerInChrome = pointerInWindow &&
    // (y <= 64 || y >= height-132) (:1196-1197, HoverHandler on the root);
    // wake on any mouse move (:1283-1286); 300ms fades on the chrome itself.
    property bool chromeVisible: true
    readonly property bool pointerInWindow: rootHover.hovered
    readonly property bool pointerInChrome: rootHover.hovered
        && (rootHover.point.position.y <= 64
            || rootHover.point.position.y >= root.height - 132)

    function wake() {
        chromeVisible = true
        hideTimer.restart()
    }

    // Last hover position that WOKE the chrome. Load-bearing delta guard:
    // Qt re-delivers HoverMove at the unchanged position whenever the scene
    // under a stationary cursor updates (constant on any animated surface —
    // and under the VNC platform it storms every frame, measured 2026-08-02).
    // Without the guard each re-delivery restarts hideTimer and the chrome
    // never auto-hides. "Wake on any mouse move" (:1283-1286) means an actual
    // position CHANGE.
    property point lastHoverPos: Qt.point(-1, -1)

    HoverHandler {
        id: rootHover
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        onPointChanged: {
            var p = rootHover.point.position
            if (Math.abs(p.x - root.lastHoverPos.x) > 0.5
                    || Math.abs(p.y - root.lastHoverPos.y) > 0.5) {
                root.lastHoverPos = p
                root.wake()
            }
        }
    }

    Timer {
        id: hideTimer
        interval: 6000
        repeat: false
        running: root.visible
        onTriggered: {
            if (!QbzImmersive.immSearchOpen && !root.pointerInChrome)
                root.chromeVisible = false
            else
                restart() // suppressed — re-check after another interval
        }
    }

    // --- Layer 3: the FOCUS panels (B3 + B4) --------------------------------
    // FOCUS: one panel per mode, gated viewMode==0 && mode==N, full-viewport
    // — each panel carries its own Slint-internal reserves (pad-top 52/70,
    // the 132px player clearance; §6.1/§6.2/§6.6).
    AlbumReactivePanel {
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 0
    }
    StaticPanel {
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 1
    }
    CoverflowPanel {
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 2
    }
    SpectrumPanel {
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 3
    }
    WaveBedPanel {
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 6
    }
    // FOCUS Lyrics (mode 4, B4 §6.6): the giant-line variant. It shares the
    // immersive LyricsSyncEngine with the split lyrics mount (trap 22 — the
    // gate covers BOTH lyric surfaces and nothing else). No entry load: Qt
    // fetches lyrics automatically per track (§5.5).
    LyricsFocusPanel {
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 4
        lines: root.immLyricsLines
        synced: root.immLyricsSynced
        status: root.immLyricsStatus
        sync: immLyricsEngine
    }
    // FOCUS Queue (mode 5, B4 §6.5): the same QueueTabsPanel as the split
    // placement, full-screen (centered: false -> min(vw-96, 720) column).
    QueueTabsPanel {
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 5
        centered: false
    }

    // --- The shared immersive lyrics engine + document (§6.6, trap 22) ------
    // ONE LyricsSyncEngine for BOTH immersive lyric surfaces (FOCUS mode 4
    // AND SPLIT panel 0); the gate is the §6.6 formula verbatim
    // (lyrics_sync.rs:214-217 parity). The QbzLyrics doc fields are parsed
    // once here and handed to both mounts.
    readonly property var immLyricsDoc: {
        try {
            return JSON.parse(QbzLyrics.docJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property var immLyricsLines: root.immLyricsDoc.lines !== undefined
        && root.immLyricsDoc.lines !== null ? root.immLyricsDoc.lines : []
    readonly property bool immLyricsSynced: root.immLyricsDoc.synced === true
    readonly property int immLyricsStatus: root.immLyricsDoc.status !== undefined
        ? root.immLyricsDoc.status : 0

    LyricsSyncEngine {
        id: immLyricsEngine
        gateOpen: QbzImmersive.open
            && ((QbzImmersive.viewMode === 0 && QbzImmersive.mode === 4)
                || (QbzImmersive.viewMode === 1 && QbzImmersive.splitPanel === 0))
    }
    // Every doc commit resets the ladder and re-lands on the right line (the
    // LyricsPanel.qml:60-66 idiom — the Slint kick() on commit/panel open).
    Connections {
        target: QbzLyrics
        function onDocJsonChanged() {
            immLyricsEngine.reset()
            immLyricsEngine.kick()
        }
    }

    // --- Layer 3 (viewMode==1): the SPLIT layout (§5.6/D1) -------------------
    // HorizontalLayout padding-top 56 bottom 132 sides 56 spacing 48
    // (:1423-1427); LEFT column EXPLICIT width (root.width - 160)/2 — the
    // shipped Slint is a true 50/50 (:1435,:1516; the stale "55/45"/"11:9"
    // comments at :1406-1410,:1503-1510 are wrong, D1). Artwork + the B3
    // ImmersiveTrackMeta LEFT; the active data panel RIGHT, on a pad-20
    // clipped plate with NO glass backing (owner removed it,
    // :1484-1487,:1517-1518).
    Item {
        id: splitRoot
        anchors.fill: parent
        visible: QbzImmersive.viewMode === 1

        // col-h: the column height below the header/player clearance
        // (:1415). colW: the explicit half (:1435).
        readonly property real colW: (root.width - 160) / 2
        readonly property real colH: root.height - 56 - 132

        Row {
            anchors.fill: parent
            anchors.topMargin: 56
            anchors.bottomMargin: 132
            anchors.leftMargin: 56
            anchors.rightMargin: 56
            spacing: 48

            // LEFT (50%): artwork + the shared metadata block, centered in
            // its half (:1433-1501), spacing 28.
            Item {
                id: leftCol
                width: splitRoot.colW
                height: parent.height

                // Native source resolution (physical px) of the active
                // cover, so the art is never UPSCALED beyond its own size
                // (:1449-1452) — the AlbumReactivePanel.qml artProbe idiom
                // (a hidden probe Image on the same file:// cache path; the
                // shared image cache makes the second decode free).
                Image {
                    id: splitArtProbe
                    visible: false
                    source: QbzPlayer.npArtworkPath
                }
                readonly property real srcNative: splitArtProbe.status === Image.Ready
                    ? splitArtProbe.implicitWidth : 0
                // max(240, min(colW, colH-200, native)) (:1453-1455).
                readonly property real artSize: Math.max(240,
                    Math.min(splitRoot.colW, splitRoot.colH - 200, leftCol.srcNative))

                Column {
                    anchors.centerIn: parent
                    width: parent.width
                    spacing: 28

                    // Artwork square, radius 12, shadow 40/#00000099/12
                    // (:1458-1476).
                    Item {
                        width: parent.width
                        height: leftCol.artSize

                        // Static drop shadow, offset-y 12 — the
                        // CoverflowPanel MultiEffect idiom (blur 40 of
                        // blurMax 64); no shaders -> a plain offset rect.
                        Rectangle {
                            x: (parent.width - leftCol.artSize) / 2
                            y: 12
                            width: leftCol.artSize
                            height: leftCol.artSize
                            radius: 12
                            color: "#99000000"
                            layer.enabled: !root._noShaders
                            layer.effect: MultiEffect {
                                blurEnabled: true
                                blurMax: 64
                                blur: 0.625 // 40/64
                            }
                        }
                        // Neutral placeholder while the cover is unresolved.
                        Rectangle {
                            visible: QbzPlayer.npArtworkPath === ""
                            x: (parent.width - leftCol.artSize) / 2
                            width: leftCol.artSize
                            height: leftCol.artSize
                            radius: 12
                            color: "#14ffffff"
                        }
                        RoundedImage {
                            x: (parent.width - leftCol.artSize) / 2
                            width: leftCol.artSize
                            height: leftCol.artSize
                            visible: QbzPlayer.npArtworkPath !== ""
                            radius: 12
                            source: QbzPlayer.npArtworkPath
                        }
                    }

                    // The shared metadata block (:1484-1500). The Slint
                    // meta-plate is padding-only (the glass fill was removed
                    // by the owner) — 22px side gutters so the elided rows
                    // have a real width to truncate against.
                    ImmersiveTrackMeta {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: parent.width - 44
                        equalizerTint: "#7c3aed"
                    }
                }
            }

            // RIGHT (50%): the active split panel on a clipped plate, NO
            // glass backing (:1511-1596). The panel's inner box is inset by
            // the 20px pad on all sides (:1522-1527).
            Item {
                width: splitRoot.colW
                height: parent.height
                clip: true

                Item {
                    anchors.fill: parent
                    anchors.margins: 20
                    clip: true

                    // splitPanel 0: the shared lyrics lines view with the
                    // §6.6 immersive knobs EXACT (:1528-1549). The sizes are
                    // computed from the WINDOW width, not the column
                    // (:1533-1537). Slint's show-attribution: false has NO
                    // Qt twin (LyricsLinesView.qml has no such property and
                    // no attribution row — D13, nothing to set).
                    LyricsLinesView {
                        anchors.fill: parent
                        visible: QbzImmersive.splitPanel === 0
                        lines: root.immLyricsLines
                        synced: root.immLyricsSynced
                        showTranslation: QbzLyrics.showTranslation
                        sync: immLyricsEngine
                        sidePadding: 8
                        lineGap: Math.round(Math.max(16,
                            Math.min(root.width * 0.014, 26)))
                        sizeInactive: Math.round(Math.max(24,
                            Math.min(root.width * 0.018, 34)))
                        sizeActive: Math.round(Math.max(28,
                            Math.min(root.width * 0.022, 42)))
                        dimmingMode: QbzLyrics.dimmingMode
                        autoFollow: true
                        uppercase: QbzLyrics.uppercase
                        fontIndex: QbzLyrics.fontIndex
                        // Solid white active line in immersive
                        // (data-panels.md §2, :1548).
                        activeColor: "#ffffff"
                        onSeekRequested: function (timeMs) {
                            if (QbzPlayer.npDurationSecs > 0)
                                QbzPlayer.seek(timeMs / 1000 / QbzPlayer.npDurationSecs)
                        }
                    }

                    // splitPanel 1: the inline Track Info credits panel
                    // (§6.3). Entry load at mount + reload on track change
                    // below (:1557-1560, §5.5).
                    TrackInfoPanel {
                        anchors.fill: parent
                        visible: QbzImmersive.splitPanel === 1
                        onVisibleChanged: if (visible)
                            QbzAlbum.openTrackInfo(QbzPlayer.npTrackId)
                    }

                    // splitPanel 2: the live Suggestions panel (§6.4). Entry
                    // load at mount + reload on track change below
                    // (:1574-1577, §5.5).
                    SuggestionsPanel {
                        anchors.fill: parent
                        visible: QbzImmersive.splitPanel === 2
                        onVisibleChanged: if (visible)
                            QbzSuggestions.load(QbzPlayer.npTrackId)
                    }

                    // splitPanel 3: the Queue + History panel, centered in
                    // the right half (§6.5; same component as the focus
                    // mount, centered: true fills the slot). The panel
                    // self-fires queuePanelOpened() when it becomes visible
                    // (:253-255).
                    QueueTabsPanel {
                        anchors.fill: parent
                        visible: QbzImmersive.splitPanel === 3
                        centered: true
                    }
                }
            }
        }
    }

    // --- Layer 4: ImmersiveSongCard (§5.1, :1602-1605) ----------------------
    // Visible ONLY viewMode==0 && mode==6 && npHasTrack (trap 19);
    // bottom-right 24px insets; NON-interactive; does NOT fade with the
    // auto-hide chrome (no opacity binding here — that is deliberate).
    ImmersiveSongCard {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.rightMargin: 24
        anchors.bottomMargin: 24
        visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === 6
            && QbzPlayer.npHasTrack
    }

    // §5.5: TrackInfo + Suggestions RELOAD on track change while their panel
    // is mounted (:1557-1560, :1574-1577 — the Slint `changed live-track-id`
    // watchers; the dedup guard lives in the Rust loaders).
    Connections {
        target: QbzPlayer
        function onNpTrackIdChanged() {
            if (!root.visible || QbzImmersive.viewMode !== 1)
                return
            if (QbzImmersive.splitPanel === 1)
                QbzAlbum.openTrackInfo(QbzPlayer.npTrackId)
            else if (QbzImmersive.splitPanel === 2)
                QbzSuggestions.load(QbzPlayer.npTrackId)
        }
    }

    // No-shader renderer detection, verbatim from SpectrumBand.qml (the
    // SPLIT artwork drop shadow is a MultiEffect blur; no shaders -> plain
    // offset rect).
    readonly property bool _noShaders: GraphicsInfo.api === GraphicsInfo.Software
        || GraphicsInfo.api === GraphicsInfo.Null

    // --- Exit paths 3 + seek arrows (§5.4, §7) -------------------------------
    // Escape on the root: dismisses the search cortinilla FIRST when open
    // (§3.4), exits immersive otherwise. The search field's own Escape
    // declines and propagates here (ImmersiveHeader.qml comment).
    Keys.onEscapePressed: function (event) {
        if (QbzImmersive.immSearchOpen)
            QbzImmersive.dismissSearch()
        else
            QbzImmersive.open = false
        event.accepted = true
    }
    // Seek arrows (§7, keybindings.rs:107-110,572-581): ArrowRight/Left ±5s,
    // Shift+ArrowRight/Left ±10s. Target clamps to DURATION (not
    // npSeekableMax — that clamp belongs to the B5 TinyBar only, trap 9).
    // Position base is the 1Hz npElapsedSecs (D14, resolved). Inert while a
    // text input has focus (§5.4) — the gate is load-bearing because key
    // events PROPAGATE from the focused field up to this root.
    Keys.onPressed: function (event) {
        if (root.textInputActive)
            return
        var d = 0
        if (event.key === Qt.Key_Right)
            d = (event.modifiers & Qt.ShiftModifier) ? 10 : 5
        else if (event.key === Qt.Key_Left)
            d = -((event.modifiers & Qt.ShiftModifier) ? 10 : 5)
        else
            return
        if (QbzPlayer.npDurationSecs > 0) {
            var target = Math.max(0, Math.min(QbzPlayer.npDurationSecs,
                                              QbzPlayer.npElapsedSecs + d))
            QbzPlayer.seek(target / QbzPlayer.npDurationSecs)
        }
        event.accepted = true
    }

    // --- Layer 5: the player bar (§5.7, :1607-1616) -------------------------
    // Centered, y = height-90-24; fades with the auto-hide chrome on the
    // SAME binding as the header (300ms), inert while hidden.
    ImmersivePlayerBar {
        id: playerBar
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.height - 90 - 24
        viewportWidth: root.width
        opacity: root.chromeVisible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 300 } }
        enabled: opacity > 0
        onWakeRequested: root.wake()
    }

    // --- Layer 6: the header band (§5.2) --------------------------------------
    ImmersiveHeader {
        id: header
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.leftMargin: 24
        anchors.rightMargin: 24
        anchors.topMargin: 16
        height: 36
        opacity: root.chromeVisible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 300 } }
        enabled: opacity > 0
    }

    // --- Layer 7: the search cortinilla (§3.4, :1695-1705) -------------------
    // TOPMOST: above the header band and the player bar. Self-gated on
    // QbzImmersive.immSearchOpen && query >= 2 chars; NOT chrome-faded (its
    // open flag is exactly what suppresses the auto-hide, trap 10). The
    // accent follows the atmosphere so the keyboard-active row matches.
    ImmersiveSearchCortinilla {
        anchors.fill: parent
        accent: QbzShell.ambientAccent
    }
}
