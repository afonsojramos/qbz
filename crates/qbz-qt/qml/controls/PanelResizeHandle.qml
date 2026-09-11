// Edge drag handle for a resizable shell panel (#771) — the playlist sidebar's
// right edge and the queue/lyrics column's left edge.
//
// It owns NOTHING but the pointer: every drag position is handed to the host
// through `dragged(px)` and the host (AppShell) forwards it to the bridge,
// where `panel_resize.rs` decides clamp / snap / close. `released()` is where
// the host persists. `active` mirrors the press so the host can suspend the
// panel's width animation while the pointer is driving it (a 160 ms Behavior
// fighting a mouse reads as lag).
//
// Where it sits: on the border the user SEES. The panel's own edge is
// invisible — the content frame's 8px bezel is the same colour as the panel
// (AppShell.qml `contentFrame`) — so the visible line is the content PANE's
// edge, 8px in. The host places the strip there (owner, 2026-09-11, with the
// pointer screenshot: "ahí debería estar el grabber").
//
// Visual: one 2x28 notch in border-muted, vertically centred on the border
// (the Local Library tree rail draws a full-height line that lights up in
// accent; this is the "much more subtle" version the owner asked for). It
// brightens a little on hover/press, and `pulse()` — called by the host when
// a drag reaches a snap point (min, max, mini) — runs ONE short accent fade
// behind it. Nothing here ticks: the fade is a one-shot that ends.

import QtQuick
import "../theme"

Item {
    id: root
    width: 5

    /// The px the host cares about, in the host's coordinate space: for the
    /// sidebar the pointer's x (a width from the window's left edge); for
    /// the right column the host converts to a width itself.
    signal dragged(real px)
    signal released()
    readonly property bool active: area.pressed
    readonly property bool lit: area.containsMouse || area.pressed

    /// One accent pulse behind the notch — the host calls it when the drag
    /// reaches a snap point. Restarting mid-fade is fine (it just re-peaks).
    function pulse() {
        glowFade.restart()
    }

    /// "There is more past here": a steady, slightly larger glow that leans
    /// toward `beyondSide` (-1 = left, +1 = right) while the host says the
    /// drag is parked at a range end that can push through to something
    /// else (the queue column at its maximum -> Listen List). Static while
    /// on; nothing ticks.
    property bool beyond: false
    property int beyondSide: -1

    Rectangle {
        visible: root.beyond
        anchors.verticalCenter: parent.verticalCenter
        x: root.beyondSide < 0 ? (root.width / 2) - width + 2 : (root.width / 2) - 2
        width: 12
        height: 44
        radius: 6
        color: theme.accent
        opacity: 0.28
    }

    Rectangle {
        id: glow
        anchors.centerIn: parent
        width: 4
        height: 34
        radius: 2
        color: theme.accent
        opacity: 0.0
        SequentialAnimation {
            id: glowFade
            NumberAnimation { target: glow; property: "opacity"; to: 0.55; duration: 60 }
            NumberAnimation { target: glow; property: "opacity"; to: 0.0; duration: 260; easing.type: Easing.OutQuad }
        }
    }

    Rectangle {
        id: notch
        anchors.centerIn: parent
        width: 2
        height: 28
        radius: 1
        color: theme.borderMuted
        opacity: root.lit ? 0.95 : 0.55
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.SplitHCursor
        acceptedButtons: Qt.LeftButton
        // The strip is 5px wide: the pointer leaves it on the first move, so
        // the press must keep delivering moves to us, not to what is under.
        preventStealing: true
        onPositionChanged: function (mouse) {
            if (!pressed)
                return
            const p = area.mapToItem(root.parent, mouse.x, mouse.y)
            root.dragged(p.x)
        }
        onReleased: root.released()
        onCanceled: root.released()
    }
}
