// ImmersiveAtmosphere — the Kawarp-like animated atmosphere underlay (§6.1
// of the 2026-08-02 immersive-port contract), port of
// crates/qbz-ui/ui/immersive/ImmersiveAtmosphere.slint:8-83.
//
// FOUR animated layers of the SAME 128x128 atmosphere bitmap (host-generated:
// tiny artwork sample -> blur -> color adjust -> vignette), each oversized
// past the viewport and drifting on its own sine pair, plus a STATIC fallback
// (the plain cover at 0.35 opacity) when no atmosphere bitmap exists. A
// `dim` overlay and a 180-degree gradient scrim sit on top (:77-82). The
// component only animates/composites — it can be reused outside ImmersiveView.
//
// Mounted by ImmersiveView as layer 2 (§5.1): ALWAYS the underlay (the Slint
// shader-mode==0 gate is constant in v1, ruling 1), source
// QbzImmersive.atmosphereUrl, fallback QbzPlayer.npArtworkPath,
// `animated: QbzPlayer.npPlaying`, dim 0.15 (:1313-1321). Spectrum/WaveBed
// paint opaque #000 over it — intended faithful behavior
// (SpectrumPanel.slint:45-46,118, ImmersiveWaveBedPanel.slint:49).
//
// The drift clock: Slint reads animation-tick() pull-based and FREEZES it at
// 0 while !animated (:16). The Qt twin is ONE linear NumberAnimation on
// `tick` (ms), reset to 0 whenever `animated` drops, so the static pose is
// the Slint tick=0 pose exactly. All x/y offsets are plain bindings of tick
// — nothing is animated per-layer.

import QtQuick

Item {
    id: root

    /// 128x128 atmosphere PNG (file:// URL; "" = not ready). NEVER cleared
    /// on track change by the publisher (flicker fix, §3).
    property string source: ""
    /// Plain cover (file:// URL) shown when `source` is empty.
    property string fallbackSource: ""
    /// Dark overlay opacity (:11, :78).
    property real dim: 0.15
    /// Layer opacities (:12-13).
    property real baseOpacity: 0.95
    property real warpOpacity: 0.48
    /// Gates the layer drift (:14); the mount binds QbzPlayer.npPlaying.
    property bool animated: true

    // Drift clock in ms (:16). 24h period, linear; reset to 0 on the
    // animated->static edge so the static pose is the Slint tick=0 pose.
    property real tick: 0
    NumberAnimation on tick {
        from: 0
        to: 86400000
        duration: 86400000
        loops: Animation.Infinite
        running: root.animated
    }
    onAnimatedChanged: if (!animated) tick = 0

    clip: true

    // Root surface (:18).
    Rectangle {
        anchors.fill: parent
        color: "#0a0a0b"
    }

    // --- Layer 1 (:21-32) -------------------------------------------------
    Image {
        readonly property real sx: Math.sin(root.tick / 7600.0 * 6.283185)
            + 0.35 * Math.sin((root.tick / 3300.0 + 1.9) * 6.283185)
        readonly property real sy: Math.sin((root.tick / 11200.0 + 1.1) * 6.283185)
        visible: root.source !== ""
        source: root.source
        fillMode: Image.PreserveAspectCrop
        width: root.width + 520
        height: root.height + 420
        x: -260 + sx * 132
        y: -210 + sy * 58
        opacity: root.baseOpacity
    }
    // --- Layer 2 (:33-44) -------------------------------------------------
    Image {
        readonly property real sx: Math.sin((root.tick / 9200.0 + 0.4) * 6.283185)
            + 0.42 * Math.sin((root.tick / 4100.0 + 2.6) * 6.283185)
        readonly property real sy: Math.sin((root.tick / 14800.0 + 1.7) * 6.283185)
        visible: root.source !== ""
        source: root.source
        fillMode: Image.PreserveAspectCrop
        width: root.width + 620
        height: root.height + 500
        x: -310 - sx * 156
        y: -250 - sy * 74
        opacity: root.warpOpacity
    }
    // --- Layer 3 (:45-56) -------------------------------------------------
    Image {
        readonly property real sx: Math.sin((root.tick / 5700.0 + 1.6) * 6.283185)
        readonly property real sy: Math.sin((root.tick / 12300.0 + 0.2) * 6.283185)
            + 0.28 * Math.sin((root.tick / 3600.0 + 2.8) * 6.283185)
        visible: root.source !== ""
        source: root.source
        fillMode: Image.PreserveAspectCrop
        width: root.width + 700
        height: root.height + 540
        x: -350 + sx * 118
        y: -270 + sy * 92
        opacity: 0.24
    }
    // --- Layer 4 (:57-67) -------------------------------------------------
    Image {
        readonly property real sx: Math.sin((root.tick / 15100.0 + 2.4) * 6.283185)
        readonly property real sy: Math.sin((root.tick / 6900.0 + 1.4) * 6.283185)
        visible: root.source !== ""
        source: root.source
        fillMode: Image.PreserveAspectCrop
        width: root.width + 560
        height: root.height + 620
        x: -280 - sx * 102
        y: -310 + sy * 128
        opacity: 0.18
    }
    // --- Static fallback (:68-76) ------------------------------------------
    Image {
        visible: root.source === "" && root.fallbackSource !== ""
        source: root.fallbackSource
        fillMode: Image.PreserveAspectCrop
        width: root.width + 220
        height: root.height + 220
        x: -110
        y: -110
        opacity: 0.35
    }

    // dim overlay (:77-79).
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, root.dim)
    }
    // 180-degree scrim (:80-82): #00000066 top -> transparent 35% ->
    // #00000080 bottom (RRGGBBAA converted).
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#66000000" }
            GradientStop { position: 0.35; color: "transparent" }
            GradientStop { position: 1.0; color: "#80000000" }
        }
    }
}
