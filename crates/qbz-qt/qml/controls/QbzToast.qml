// QbzToast — the app's in-app toast host (primitives/Toast.slint), mounted
// ONCE in AppShell.qml as a sibling of the content frame. Until this file the
// port had NO toast seam at all: TrackRow.qml:597-600, LabelView.qml:25,
// LyricsPanel.qml:16, PlayerBar.qml:515-517, NowPlayingBarSmall.qml:461-463
// and local_bulk.rs:190-193 each document the same gap and fall back to
// silence. Silence is not 1:1 — "Already in {name}" is the ONLY signal that a
// duplicate add did nothing, and MyQBZ alone routes 26 distinct messages
// through crate::toast.
//
// CONTRACT. Rust publishes `QbzShell.toastJson`
// (`{ seq, kind, message, persistent }`, ToastDoc): ONE toast at a time, a new
// one replaces the current, and Rust has ALREADY applied the `in_app_toasts`
// settings gate (errors always pass) — this file must not re-read the setting.
// The auto-hide timer lives HERE, keyed off `seq`, so Rust never owns a timer
// nor needs a hide round-trip: success/info 3000ms, warning 4000, error 5000,
// buffering persistent (qbz-slint-common/src/toast.rs:31-40).
//
// The component is ALWAYS in the tree (never `if`-gated) so the entry
// animation runs; the close button's MouseArea is live only while a toast is
// showing, so the transparent overlay never eats a click meant for the content
// underneath. The root itself carries no MouseArea at all.
//
// Geometry (Toast.slint): bubble 44px tall, width = content capped at
// `parent.width - 80`, horizontally centred, `y = parent.height - height -
// (showing ? 100 : 84)` so it slides up the last 16px, 200ms ease-out on both
// y and opacity, r8 surface-elevated, padding 16 left / 8 right, row spacing
// 12, 20px kind icon, 14px message elided to one line, 24x24 r4 close with a
// 16px glyph.
//
// The .slint's `drop-shadow-blur: 16px / #00000066` (:55-56) is DROPPED: Qt's
// shadow effects render nothing on this port's software path (the same reason
// NavFlyout.qml:30-33 and every modal drop it). The 44px elevated fill on a
// dark scene carries the separation.
//
// ASSET + TINT PREREQUISITE (wiring items W21/W22): `circle-check-big`,
// `circle-alert` and `triangle-alert` are NOT in qml/assets/icons/* yet, and
// icon_tint_qt.rs's TOKENS has no `success` / `danger` tint. QbzIcon takes a
// tint NAME, never a colour, so both halves must land or the success/error
// glyphs render NOTHING with nothing logged.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    // Guarded parse — a raw JSON.parse inside a binding throws on the
    // pre-publish frame and takes the host down with it
    // (PlaylistView.qml:55-61).
    readonly property var doc: {
        try {
            return JSON.parse(QbzShell.toastJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property int seq: root.doc.seq || 0
    readonly property string kind: root.doc.kind || "info"
    readonly property string message: root.doc.message || ""
    readonly property bool persistent: root.doc.persistent === true

    property bool showing: false

    readonly property int autoHideMs: root.kind === "error" ? 5000
        : root.kind === "warning" ? 4000 : 3000

    readonly property string iconName: root.kind === "success" ? "circle-check-big"
        : root.kind === "error" ? "circle-alert"
        : root.kind === "warning" ? "triangle-alert"
        : root.kind === "buffering" ? "loader-circle"
        : "info"
    readonly property string iconTint: root.kind === "success" ? "success"
        : root.kind === "error" ? "danger"
        : root.kind === "warning" ? "warning"
        : "accent"

    // A new sequence number IS the show event (a repeated message with the
    // same text must still re-show, so the trigger cannot be the text).
    onSeqChanged: {
        if (root.seq <= 0 || root.message === "")
            return
        root.showing = true
        hideTimer.stop()
        if (!root.persistent)
            hideTimer.restart()
    }

    Timer {
        id: hideTimer
        interval: root.autoHideMs
        repeat: false
        onTriggered: root.showing = false
    }

    Rectangle {
        id: bubble

        // 16 pad + 20 icon + 12 gap + message + 12 gap + 24 close + 8 pad.
        readonly property real chromeWidth: 16 + 20 + 12 + 12 + 24 + 8
        width: Math.min(bubble.chromeWidth + Math.ceil(msg.implicitWidth),
                        Math.max(0, root.width - 80))
        height: 44
        x: Math.round((root.width - width) / 2)
        y: root.height - height - (root.showing ? 100 : 84)
        opacity: root.showing ? 1.0 : 0.0
        Behavior on y { NumberAnimation { duration: 200; easing.type: Easing.OutQuad } }
        Behavior on opacity { NumberAnimation { duration: 200; easing.type: Easing.OutQuad } }

        radius: 8
        color: theme.surfaceElevated

        Row {
            anchors.fill: parent
            anchors.leftMargin: 16
            anchors.rightMargin: 8
            spacing: 12

            QbzIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: root.iconName
                width: 20
                height: 20
                tintName: root.iconTint
            }
            Text {
                id: msg
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, bubble.width - bubble.chromeWidth)
                text: root.message
                color: theme.textPrimary
                font.pixelSize: 14
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 24
                height: 24
                radius: 4
                color: closeArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    anchors.centerIn: parent
                    name: "x"
                    width: 16
                    height: 16
                    tintName: closeArea.containsMouse ? "textPrimary" : "muted"
                }
                MouseArea {
                    id: closeArea
                    anchors.fill: parent
                    // Live ONLY while a toast is showing — the hidden overlay
                    // must not capture clicks (Toast.slint:106-108).
                    enabled: root.showing
                    hoverEnabled: root.showing
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        hideTimer.stop()
                        root.showing = false
                    }
                }
            }
        }
    }
}
