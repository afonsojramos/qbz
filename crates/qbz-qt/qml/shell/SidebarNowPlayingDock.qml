// SidebarNowPlayingDock — the Large-NPB (mode 3) cover + spectrum dock: the
// L's vertical arm. QML port of crates/qbz-ui/ui/shell/SidebarNowPlayingDock.slint.
//
// Mounted as a ROOT overlay by AppShell, pinned flush to the window
// bottom-left (NOT inside the sidebar — that floated it above the bar).
// Layout, bottom-up: a square album cover (= the fed width) at the very
// bottom; ABOVE it a compact spectrum band the user can hide with the
// top-right eye button on the cover.
//
// HEIGHT IS LOAD-BEARING. `QbzShell.largeDockHeight` (shell_bridge.rs) is the
// single source of truth and the layout below is derived from the SAME
// constants:
//   ON  = 9 top pad + 42 band + 10 gap + 208 art + 4 bottom gap = 273
//   OFF = 9 top pad                    + 208 art + 4 bottom gap = 221
// Sidebar.qml reserves `largeDockHeight - npb height` so the playlist list
// stops ABOVE the cover, and AppShell pins this overlay at
// `height - largeDockHeight`. Change the arithmetic in one place and the
// cover slides off the window edge.
//
// PERF (the rule the ambient field already proved): the FFT streams replace
// their QList ~30x/s, so a Repeater bound to QbzViz.bars would rebuild every
// delegate every frame. The Repeaters below use STATIC models (column
// counts) and bind only each bar's HEIGHT — a tick re-evaluates bindings and
// instantiates nothing. Only the ACTIVE mode is instantiated (the modes sit
// behind `active:` Loaders), and the capture tap is gated by vizShouldRun, so
// a hidden band costs zero CPU: the producer parks and the drain thread sleeps.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    // Fed by AppShell (sidebar content width). The cover is square.
    // height comes from the bridge — never recompute it here.
    height: QbzShell.largeDockHeight

    // Fed by AppShell so the ambient-vs-solid surface rule lives in ONE place.
    property bool ambientOn: false

    readonly property bool bandOn: QbzShell.largeVisualizerOn
    readonly property int bandHeight: 42
    readonly property int padTop: 9
    readonly property int bandGap: 10
    // The cover's top edge: below the band when it is shown.
    readonly property int artY: padTop + (bandOn ? bandHeight + bandGap : 0)

    // Capture gate — the Slint AppShell's `viz-should-run`. OFF stops the FFT
    // producer outright (viz_qt.rs parks it), so leaving Large or hiding the
    // band costs nothing. The transport is mirrored separately (a paused
    // player parks the producer via now_playing.rs).
    readonly property bool vizShouldRun: root.visible && root.bandOn && root.Window.active
    onVizShouldRunChanged: QbzViz.setEnabled(root.vizShouldRun)
    Component.onCompleted: QbzViz.setEnabled(root.vizShouldRun)
    Component.onDestruction: QbzViz.setEnabled(false)

    // Top hairline between the playlists and the dock.
    Rectangle {
        x: 0
        y: 0
        width: root.width
        height: 1
        color: theme.borderSubtle
    }

    // ---- Spectrum band -----------------------------------------------------
    Rectangle {
        id: band
        visible: root.bandOn
        x: 0
        y: root.padTop
        width: root.width
        height: root.bandHeight
        radius: theme.radiusMd
        color: "#59000000"
        clip: true

        // Album-derived gradient, reusing the ambient triad (ambient_qt.rs)
        // instead of a second album-color pipeline — the Slint dock reads
        // ImmersiveState.spectrum-primary/secondary, which is the same idea.
        readonly property color topColor: QbzShell.ambientPrimary
        readonly property color bottomColor: QbzShell.ambientSecondary

        // Mode 0 — Bars: 28 MIRRORED columns over the 14 ACTIVE bins (the
        // 16-bin FFT leaves {1, 15} empty; SpectrumPanel.slint parity).
        Loader {
            anchors.fill: parent
            active: QbzShell.largeSpectrumMode === 0
            sourceComponent: Item {
                id: barsMode
                readonly property var activeBins: [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
                readonly property real slot: band.width / 28
                Repeater {
                    model: 28
                    delegate: Rectangle {
                        required property int index
                        // Mirror: columns 14..27 replay 13..0.
                        readonly property int bin: index < 14
                            ? barsMode.activeBins[index]
                            : barsMode.activeBins[27 - index]
                        readonly property real amp: Math.max(0, Math.min(1, QbzViz.bars[bin] || 0))
                        x: index * barsMode.slot + 1
                        width: barsMode.slot - 2
                        height: Math.max(2, amp * band.height)
                        y: band.height - height
                        radius: 1
                        gradient: Gradient {
                            GradientStop { position: 0.0; color: band.topColor }
                            GradientStop { position: 1.0; color: band.bottomColor }
                        }
                        Behavior on height {
                            NumberAnimation { duration: 90; easing.type: Easing.OutQuad }
                        }
                    }
                }
            }
        }

        // Mode 1 — Waveform: 48 downsampled L columns, mirrored about the
        // band's vertical centre (the L half is samples 0..255).
        Loader {
            anchors.fill: parent
            active: QbzShell.largeSpectrumMode === 1
            sourceComponent: Item {
                id: waveMode
                readonly property real slot: band.width / 48
                Repeater {
                    model: 48
                    delegate: Rectangle {
                        required property int index
                        readonly property real amp: Math.min(1, Math.abs(QbzViz.waveform[index * 5] || 0))
                        x: index * waveMode.slot + 1
                        width: waveMode.slot - 2
                        height: Math.max(2, amp * band.height)
                        y: (band.height - height) / 2
                        radius: 1
                        color: band.topColor
                        Behavior on height {
                            NumberAnimation { duration: 70; easing.type: Easing.OutQuad }
                        }
                    }
                }
            }
        }

        // Mode 2 — Energy: the 5 semantic bands (sub-bass .. air).
        Loader {
            anchors.fill: parent
            active: QbzShell.largeSpectrumMode === 2
            sourceComponent: Item {
                id: energyMode
                readonly property real slot: band.width / 5
                Repeater {
                    model: 5
                    delegate: Rectangle {
                        required property int index
                        readonly property real amp: Math.max(0, Math.min(1, QbzViz.energy[index] || 0))
                        x: index * energyMode.slot + 3
                        width: energyMode.slot - 6
                        height: Math.max(2, amp * band.height)
                        y: band.height - height
                        radius: 1
                        gradient: Gradient {
                            GradientStop { position: 0.0; color: band.topColor }
                            GradientStop { position: 1.0; color: band.bottomColor }
                        }
                        Behavior on height {
                            NumberAnimation { duration: 90; easing.type: Easing.OutQuad }
                        }
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

    // ---- Album cover -------------------------------------------------------
    // Drop-shadow approximation of the Slint dock's 24px blur (two offset
    // rects — QtQuick.Effects renders nothing on the software path, see
    // RoundedImage.qml).
    Rectangle {
        x: 0
        y: root.artY + 4
        width: root.width
        height: root.width
        radius: theme.radiusMd
        color: "#66000000"
    }

    Rectangle {
        id: art
        x: 0
        y: root.artY
        width: root.width
        height: root.width
        radius: theme.radiusMd
        color: root.ambientOn ? theme.surfaceMainA22 : theme.surfaceMain
        clip: true

        RoundedImage {
            visible: QbzPlayer.npHasTrack
            anchors.fill: parent
            source: QbzPlayer.npArtworkPath
            radius: theme.radiusMd
        }
        QbzIcon {
            visible: !QbzPlayer.npHasTrack
            name: "music"
            width: root.width * 0.32
            height: root.width * 0.32
            anchors.centerIn: parent
            tintName: "muted"
        }

        // Hover tracker over the whole cover — reveals the toggle. The toggle
        // ALSO stays visible while the band is hidden, so it can be brought back.
        MouseArea {
            id: coverHover
            anchors.fill: parent
            hoverEnabled: true
        }

        // Single top-right toggle: show/hide the spectrum band.
        Rectangle {
            width: 28
            height: 28
            x: parent.width - width - 8
            y: 8
            radius: 8
            color: eyeHover.containsMouse ? "#2effffff" : "#80000000"
            border.width: 1
            border.color: "#33ffffff"
            opacity: (coverHover.containsMouse || eyeHover.containsMouse || !root.bandOn) ? 1.0 : 0.0
            Behavior on opacity { NumberAnimation { duration: 140 } }

            QbzIcon {
                anchors.centerIn: parent
                name: root.bandOn ? "eye" : "eye-off"
                width: 15
                height: 15
                tintName: "primary"
            }
            MouseArea {
                id: eyeHover
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: QbzShell.largeToggleVisualizer()
            }
        }
    }
}
