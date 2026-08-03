// ImmersiveView — the immersive root overlay (ImmersiveView.slint:1186-1705),
// the Qt port of contract 2026-08-02-immersive-port §5.1-§5.5. Mounted by
// AppShell AFTER QbzToast and BEFORE ArtPreviewOverlay/QbzTooltip (§5.1
// declaration-order z convention).
//
// BLOCK B2+B3 SCOPE: the overlay shell (root + click-blocker + chrome +
// auto-hide + exit paths 2-4 + the fullscreen guard + seek arrows) PLUS the
// B3 panels: the ImmersiveAtmosphere underlay (layer 2), the five FOCUS
// panels (Album Reactive / Static / Coverflow / Spectrum / Wave Bed) and the
// layer-4 ImmersiveSongCard. Lyrics (mode 4) and Queue (mode 5) stay STUBS
// until B4; the SPLIT stubs stay until B4 (ImmersiveTrackMeta is already
// wired under splitArtStub, §6.7); B5 adds the player bar + search
// cortinilla (layer 7 is therefore absent, per contract §12 B2).
//
// Layer order bottom->top (§5.1):
//   1. root color #0a0a0b, clip (:1199-1200)
//   2. ImmersiveAtmosphere — ALWAYS the underlay (the Slint shader-mode==0
//      gate is constant in v1, ruling 1; :1313-1321)
//   2b. FULL-COVERAGE click-blocker MouseArea (load-bearing — without it
//      clicks pass through to the desktop header search field, the dock's
//      spectrum band and the viz eye toggle; port of the Slint root
//      TouchArea :1283-1286). Stacked BELOW panels + chrome.
//   3. FOCUS panels (viewMode==0 && mode==N) / SPLIT stubs (viewMode==1)
//   4. ImmersiveSongCard (viewMode==0 && mode==6 && npHasTrack — trap 19;
//      does NOT fade with the chrome, :1602-1605)
//   6. ImmersiveHeader band (layer 5 — the PlayerBar — is B5)

import QtQuick
import com.blitzfc.qbz
import "../controls"
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

    // --- Layer 3: the FOCUS panels (B3) + the remaining stubs (B4) ---------
    // FOCUS: one panel per mode, gated viewMode==0 && mode==N, full-viewport
    // — the B2 stubs' 24/64/132 insets are replaced by each panel's own
    // Slint-internal reserves (pad-top 52/70, the 132px player clearance;
    // §6.1/§6.2). Lyrics (mode 4) and Queue (mode 5) stay stub Rectangles
    // until B4; the Queue stub keeps its §5.5 entry load
    // (QbzQueue.queuePanelOpened) so the B2 header-menu wiring still works.
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
    Repeater {
        model: [
            { "m": 4, "label": QbzSession.tr("Lyrics", QbzSession.trRev) },
            { "m": 5, "label": QbzSession.tr("Queue", QbzSession.trRev) },
        ]
        delegate: Rectangle {
            required property var modelData
            anchors.fill: parent
            anchors.leftMargin: 24
            anchors.rightMargin: 24
            anchors.topMargin: 64
            anchors.bottomMargin: 132
            visible: QbzImmersive.viewMode === 0 && QbzImmersive.mode === modelData.m
            radius: 12
            color: "#0dffffff"
            border.width: 1
            border.color: "#2effffff"
            Text {
                anchors.centerIn: parent
                text: modelData.label
                color: "#80ffffff"
                font.pixelSize: 15
            }
            onVisibleChanged: if (visible && modelData.m === 5)
                QbzQueue.queuePanelOpened()
        }
    }

    // SPLIT (viewMode==1): left artwork placeholder + right plate with one
    // stub per splitPanel. The real 50/50 layout lands in B4 (§5.6/D1).
    Item {
        anchors.fill: parent
        anchors.leftMargin: 24
        anchors.rightMargin: 24
        anchors.topMargin: 64
        anchors.bottomMargin: 132
        visible: QbzImmersive.viewMode === 1

        Rectangle {
            id: splitArtStub
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(240, Math.min(parent.width / 2 - 24, parent.height - 200))
            height: width
            radius: 12
            color: "#0dffffff"
            border.width: 1
            border.color: "#2effffff"
        }
        // B3 seam (§6.7): the real 50/50 SPLIT left column lands in B4
        // (§5.6/D1); ImmersiveTrackMeta is already wired under the artwork
        // placeholder so the SPLIT gate renders something real.
        ImmersiveTrackMeta {
            anchors.top: splitArtStub.bottom
            anchors.topMargin: 20
            anchors.left: splitArtStub.left
            width: splitArtStub.width
        }
        Repeater {
            model: [
                { "sp": 0, "label": QbzSession.tr("Lyrics", QbzSession.trRev) },
                { "sp": 1, "label": QbzSession.tr("Track Info", QbzSession.trRev) },
                { "sp": 2, "label": QbzSession.tr("Suggestions", QbzSession.trRev) },
                { "sp": 3, "label": QbzSession.tr("Queue", QbzSession.trRev) },
            ]
            delegate: Rectangle {
                required property var modelData
                anchors.left: splitArtStub.right
                anchors.leftMargin: 48
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                visible: QbzImmersive.viewMode === 1
                    && QbzImmersive.splitPanel === modelData.sp
                radius: 12
                color: "#0dffffff"
                border.width: 1
                border.color: "#2effffff"
                Text {
                    anchors.centerIn: parent
                    text: modelData.label
                    color: "#80ffffff"
                    font.pixelSize: 15
                }
                onVisibleChanged: {
                    if (!visible)
                        return
                    if (modelData.sp === 3)
                        QbzQueue.queuePanelOpened()
                    else if (modelData.sp === 1)
                        QbzAlbum.openTrackInfo(QbzPlayer.npTrackId)
                    // sp==2 Suggestions: entry load lands in B4 with the
                    // QbzSuggestions singleton (contract §12 cross-block rule:
                    // no QML may reference QbzSuggestions before B4).
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

    // §5.5: TrackInfo RELOADS on track change while its panel is mounted
    // (:1557-1560). The Suggestions twin of this reload is B4.
    Connections {
        target: QbzPlayer
        function onNpTrackIdChanged() {
            if (root.visible && QbzImmersive.viewMode === 1
                    && QbzImmersive.splitPanel === 1)
                QbzAlbum.openTrackInfo(QbzPlayer.npTrackId)
        }
    }

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
}
