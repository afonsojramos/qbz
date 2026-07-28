// SpectrumBand — the compact 42px spectrum strip above the Large-NPB cover.
// Split out of SidebarNowPlayingDock.qml: the dock owns the layout arithmetic
// and the cover, this file owns the three render modes and their motion.
// QML port of the `spectrum := Rectangle` block in
// crates/qbz-ui/ui/shell/SidebarNowPlayingDock.slint:122-183 (VBar / WaveCol).
//
// PERF — this strip redraws next to a full-window ambient canvas, so:
//  1. The FFT streams REPLACE their QList each publish, so a Repeater bound to
//     QbzViz.bars would rebuild every delegate every frame. The Repeaters use
//     STATIC models (column counts) and bind only each bar's HEIGHT.
//  2. Each mode marshals its QList into JS ONCE per publish (a `target`
//     binding), never once per delegate — the 512-float waveform read is the
//     single most expensive thing in the dock.
//  3. ONE shared Gradient for every bar, not one object per delegate.
//  4. Motion comes from ONE VizSettle driver per mode, NOT 28 per-bar
//     Behaviors (the Slint dock animates each bar; one driver is the cheap
//     equivalent — see VizSettle.qml for the full argument).
//  5. Only the ACTIVE mode is instantiated, and only while the band is shown:
//     hiding it unloads the delegates, and viz_qt.rs only ever publishes the
//     one stream the visible mode consumes.
//  6. Capture itself is gated by the dock's vizShouldRun — a hidden band costs
//     zero: the producer parks and the drain thread sleeps.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: band

    QbzTheme { id: theme }

    // Shown/hidden by the cover's eye toggle (dock-owned).
    property bool shown: true
    // Mirrors the dock's capture gate: false = the stream is frozen, so the
    // interpolator must park too.
    property bool capturing: false

    visible: band.shown
    radius: theme.radiusMd
    color: "#59000000"
    clip: true

    // Album-derived gradient, reusing the ambient triad (ambient_qt.rs) instead
    // of a second album-color pipeline — the Slint dock reads
    // ImmersiveState.spectrum-primary/secondary, which is the same idea.
    readonly property color topColor: QbzShell.ambientPrimary
    readonly property color bottomColor: QbzShell.ambientSecondary

    // ONE gradient instance shared by every bar (perf note 3).
    Gradient {
        id: barGradient
        GradientStop { position: 0.0; color: band.topColor }
        GradientStop { position: 1.0; color: band.bottomColor }
    }

    // Mode 0 — Bars: 28 MIRRORED columns over the 14 ACTIVE bins (the 16-bin
    // FFT leaves {1, 15} empty; SpectrumPanel.slint parity).
    Loader {
        anchors.fill: parent
        active: band.shown && QbzShell.largeSpectrumMode === 0
        sourceComponent: Item {
            id: barsMode
            readonly property var activeBins: [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
            readonly property real slot: band.width / 28

            // Marshal the QList into JS ONCE per publish (perf note 2). Already
            // clamped 0..1 in Rust (processor.rs map_to_log_bars).
            VizSettle {
                id: barsSettle
                target: QbzViz.bars
                live: band.capturing
            }

            Repeater {
                model: 28
                delegate: Rectangle {
                    required property int index
                    // Mirror: columns 14..27 replay 13..0.
                    readonly property int bin: index < 14
                        ? barsMode.activeBins[index]
                        : barsMode.activeBins[27 - index]
                    x: index * barsMode.slot + 1
                    width: barsMode.slot - 2
                    height: Math.max(2, barsSettle.at(bin) * band.height)
                    y: band.height - height
                    radius: 1
                    gradient: barGradient
                }
            }
        }
    }

    // Mode 1 — Waveform: 48 downsampled L columns, mirrored about the band's
    // vertical centre (the L half is samples 0..255).
    Loader {
        anchors.fill: parent
        active: band.shown && QbzShell.largeSpectrumMode === 1
        sourceComponent: Item {
            id: waveMode
            readonly property real slot: band.width / 48

            VizSettle {
                id: waveSettle
                live: band.capturing
                // The |sample| envelope is folded in HERE, once per publish (48
                // ops), instead of an abs+min per column per rendered frame.
                target: {
                    var s = QbzViz.waveform;
                    var out = new Array(48);
                    for (var i = 0; i < 48; ++i) {
                        var v = s[i * 5] || 0;
                        if (v < 0) v = -v;
                        out[i] = v > 1 ? 1 : v;
                    }
                    return out;
                }
            }

            Repeater {
                model: 48
                delegate: Rectangle {
                    required property int index
                    x: index * waveMode.slot + 1
                    width: waveMode.slot - 2
                    height: Math.max(2, waveSettle.at(index) * band.height)
                    y: (band.height - height) / 2
                    radius: 1
                    color: band.topColor
                }
            }
        }
    }

    // Mode 2 — Energy: the 5 semantic bands (sub-bass .. air).
    Loader {
        anchors.fill: parent
        active: band.shown && QbzShell.largeSpectrumMode === 2
        sourceComponent: Item {
            id: energyMode
            readonly property real slot: band.width / 5

            VizSettle {
                id: energySettle
                target: QbzViz.energy
                live: band.capturing
            }

            Repeater {
                model: 5
                delegate: Rectangle {
                    required property int index
                    x: index * energyMode.slot + 3
                    width: energyMode.slot - 6
                    height: Math.max(2, energySettle.at(index) * band.height)
                    y: band.height - height
                    radius: 1
                    gradient: barGradient
                }
            }
        }
    }

    // Click the band → cycle Bars -> Waveform -> Energy (persisted).
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: QbzShell.largeCycleSpectrum()
    }
}
