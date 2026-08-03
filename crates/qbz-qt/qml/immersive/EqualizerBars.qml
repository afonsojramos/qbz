// EqualizerBars — the 4-bar "now playing" equalizer (§6.7 of the
// 2026-08-02 immersive-port contract), port of the EqualizerBars component
// in crates/qbz-ui/ui/immersive/ImmersiveTrackInfo.slint:23-74.
//
// 18x14 box, four 3px bars at 2px spacing (manual x: 0/5/10/15 — a Row
// positioner would own the children's y and the bars must bottom-align by
// their OWN animated height). Base heights 60/100/40/80% of the 14px box,
// each sweeping scaleY 0.3..1.0 on the shared 800ms loop with phase offsets
// 0 / 0.25 / 0.125 / 0.375 (the Svelte animation-delays 0.0/0.2/0.1/0.3 of
// the 0.8s period, as fractions).
//
// The Slint loop is pull-based off animation-tick(); the Qt twin is ONE
// 800ms linear NumberAnimation on `t` (0->1), so the whole equalizer is a
// single animation node. The wave is the Slint formula verbatim:
//   0.3 + 0.7 * (0.5 - 0.5*cos((t+phase mod 1) * 2π))
// (Svelte keyframes 0/100% -> 0.3, 50% -> 1.0). The loop runs only while
// the item is visible — every mount site gates visibility on npPlaying
// (ImmersiveTrackInfo.slint:85,:100), so a paused player animates nothing.
// Slint's reduce-motion coarse clock has NO Qt twin (D11) — not ported.

import QtQuick

Item {
    id: root

    /// Bar color — every immersive mount passes #7c3aed.
    property color tint: "#7c3aed"

    width: 18
    height: 14
    implicitWidth: 18
    implicitHeight: 14

    // Loop phase 0..1, 800ms (ImmersiveTrackInfo.slint:32).
    property real t: 0
    NumberAnimation on t {
        from: 0
        to: 1
        duration: 800
        loops: Animation.Infinite
        running: root.visible
    }

    // Slint wave() verbatim (:34-36).
    function wave(phase) {
        var u = (root.t + phase) % 1.0
        return 0.3 + 0.7 * (0.5 - 0.5 * Math.cos(u * 6.283185))
    }

    // bar 1: delay 0.0, base 60% (:42-48)
    Rectangle {
        x: 0
        width: 3
        y: root.height - height
        height: root.height * 0.6 * root.wave(0.0)
        radius: 1
        color: root.tint
    }
    // bar 2: delay 0.2, base 100% (:50-56)
    Rectangle {
        x: 5
        width: 3
        y: root.height - height
        height: root.height * 1.0 * root.wave(0.25)
        radius: 1
        color: root.tint
    }
    // bar 3: delay 0.1, base 40% (:58-64)
    Rectangle {
        x: 10
        width: 3
        y: root.height - height
        height: root.height * 0.4 * root.wave(0.125)
        radius: 1
        color: root.tint
    }
    // bar 4: delay 0.3, base 80% (:66-72)
    Rectangle {
        x: 15
        width: 3
        y: root.height - height
        height: root.height * 0.8 * root.wave(0.375)
        radius: 1
        color: root.tint
    }
}
