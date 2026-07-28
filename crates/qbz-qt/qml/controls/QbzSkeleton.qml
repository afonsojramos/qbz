// QbzSkeleton — THE shared loading placeholder.
//
// Reference: crates/qbz-ui/ui/discover/HomeSkeleton.slint. The Slint HAS a
// skeleton treatment, and this is a 1:1 port of its primitive: a grey
// `surface-elevated` block on Radius.sm whose OPACITY breathes between 0.4
// and 0.85 on a 900ms shared phase. There is no travelling gradient in the
// Slint and there is none here — see the COST note below.
//
// What is a deliberate ADDITION beyond the Slint (which mounts a bare 36px
// LoadingSpinner in FavoritesView and no per-item treatment anywhere):
//   - the "row" shape (the Slint only has card + bar shapes),
//   - the per-item "art" / "circle" tiles the Library mounts over a card
//     whose cover has not landed yet,
//   - the animated-instance cap + the visibility gate.
//
// ── COST ───────────────────────────────────────────────────────────────────
// One instance = ONE animator, not one per bar: every bar binds its opacity
// to the single `breathe` real, so a 3-bar card animates one property. An
// instance whose `cellIndex` is past `animatedCap`, or whose view is hidden,
// or whose window is minimized/hidden, holds NO animator at all and renders
// as a static grey block (its Behavior is disabled, so nothing repaints).
//
// ── GATING RULE ────────────────────────────────────────────────────────────
// Freeze on NOT VISIBLE — the item being hidden or the window being
// minimized/hidden. NEVER on lost focus: a tiling desktop keeps windows
// visible and unfocused, and freezing those would stop the pulse under
// normal use. (Same rule as shell/AmbientField.qml.)
//
// ── DRIVING THE PHASE ──────────────────────────────────────────────────────
// Preferred: the HOST owns ONE 900ms Timer toggling a bool and every
// skeleton binds `phase:` to it — N placeholders, 1 timer. `selfDrive: true`
// mounts a private timer for one-off mounts.

import QtQuick
import QtQuick.Window
import "../theme"

Item {
    id: root

    // Shape:
    //   "card"   — grid cell: art square + title bar + subtitle bar
    //              (HomeSkeleton.slint's SkeletonRow card).
    //   "row"    — list row: square art + two bars.
    //   "art"    — bare square art tile (the per-item overlay).
    //   "block"  — one bar (section titles). Same shape as "art".
    //   "circle" — bare round art tile (ArtistCard/LabelCard's 190 circle).
    property string variant: "card"

    // Host-driven breathe phase (ignored when selfDrive is true).
    property bool phase: false
    property bool selfDrive: false

    // Per-instance opt-out, and the animated-instance cap: a host passes the
    // placeholder's position (0-based, viewport-relative where the list is
    // virtualized) and everything past the cap renders static.
    property bool animated: true
    property int cellIndex: 0
    readonly property int animatedCap: 48

    property real blockRadius: 8

    // "card" is the 200px card footprint: 200 art + 8 + 14 + 8 + 12 (the
    // Slint numbers). The other shapes are sized by the host.
    implicitWidth: root.variant === "card" ? 200 : 0
    implicitHeight: root.variant === "card" ? (root.width + 42) : 0

    QbzTheme { id: theme }

    readonly property bool windowShowing: root.Window.window
        ? (root.Window.window.visibility !== Window.Minimized
           && root.Window.window.visibility !== Window.Hidden)
        : true

    readonly property bool pulsing: root.animated
        && root.cellIndex < root.animatedCap
        && root.visible && root.windowShowing

    property bool _localPhase: false
    readonly property bool _phase: root.selfDrive ? root._localPhase : root.phase

    Timer {
        interval: 900
        repeat: true
        running: root.selfDrive && root.pulsing
        onTriggered: root._localPhase = !root._localPhase
    }

    // THE animated value of this skeleton (see COST). Frozen at the mid tone
    // whenever the instance is not pulsing — the Behavior is disabled then,
    // so the value snaps and no animator is left running.
    property real breathe: root.pulsing ? (root._phase ? 0.85 : 0.4) : 0.55
    Behavior on breathe {
        enabled: root.pulsing
        NumberAnimation { duration: 900; easing.type: Easing.InOutQuad }
    }

    Loader {
        anchors.fill: parent
        sourceComponent: root.variant === "card" ? cardShape
            : root.variant === "row" ? rowShape
            : root.variant === "circle" ? circleShape
            : blockShape
    }

    // ---- shapes (plain Components: they share the file scope, so `root`
    // and `theme` resolve — inline `component` blocks do not) --------------
    Component {
        id: blockShape
        Rectangle {
            color: theme.surfaceElevated
            radius: root.blockRadius
            opacity: root.breathe
        }
    }
    Component {
        id: circleShape
        Rectangle {
            color: theme.surfaceElevated
            radius: Math.min(width, height) / 2
            opacity: root.breathe
        }
    }
    Component {
        id: cardShape
        Column {
            spacing: 8
            Rectangle {
                width: root.width
                height: root.width
                radius: root.blockRadius
                color: theme.surfaceElevated
                opacity: root.breathe
            }
            Rectangle {
                width: root.width * 0.7
                height: 14
                radius: root.blockRadius
                color: theme.surfaceElevated
                opacity: root.breathe
            }
            Rectangle {
                width: root.width * 0.45
                height: 12
                radius: root.blockRadius
                color: theme.surfaceElevated
                opacity: root.breathe
            }
        }
    }
    Component {
        id: rowShape
        Row {
            spacing: 12
            Rectangle {
                width: root.height
                height: root.height
                radius: 6
                color: theme.surfaceElevated
                opacity: root.breathe
            }
            Column {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Rectangle {
                    width: root.width * 0.34
                    height: 13
                    radius: root.blockRadius
                    color: theme.surfaceElevated
                    opacity: root.breathe
                }
                Rectangle {
                    width: root.width * 0.2
                    height: 11
                    radius: root.blockRadius
                    color: theme.surfaceElevated
                    opacity: root.breathe
                }
            }
        }
    }
}
