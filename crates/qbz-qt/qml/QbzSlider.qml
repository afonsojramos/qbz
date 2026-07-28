// QbzSlider (primitives/QbzSlider.slint) — extracted from SettingsView.qml
// in phase 19. 200x22, 4px r2 track, accent fill, 16px thumb; integer
// steps. Like the Slint original the thumb follows the pointer during a
// drag (local dragValue) and commits each step via changed(int).

import QtQuick
import com.blitzfc.qbz

Item {
    property int minimum: 0
    property int maximum: 10
    property int value: 0
    signal changed(int newValue)

    QbzTheme { id: theme }

    width: 200
    height: 22

    readonly property int thumbSize: 16
    readonly property real travel: width - thumbSize
    readonly property real fraction: maximum > minimum
        ? Math.max(0, Math.min(1, (value - minimum) / (maximum - minimum))) : 0
    property bool dragging: false
    property real dragFraction: fraction
    readonly property real shownFraction: dragging ? dragFraction : fraction
    onFractionChanged: if (!dragging) dragFraction = fraction

    function commit(frac) {
        const v = Math.round(minimum + Math.max(0, Math.min(1, frac)) * (maximum - minimum))
        if (v !== value) changed(v)
    }

    Rectangle { // track
        x: 0
        y: Math.round((parent.height - height) / 2)
        width: parent.width
        height: 4
        radius: 2
        color: theme.surfaceElevated
    }
    Rectangle { // accent fill
        x: 0
        y: Math.round((parent.height - height) / 2)
        width: parent.thumbSize / 2 + parent.shownFraction * parent.travel
        height: 4
        radius: 2
        color: theme.accent
    }
    Rectangle { // thumb
        width: parent.thumbSize
        height: parent.thumbSize
        radius: parent.thumbSize / 2
        x: parent.shownFraction * parent.travel
        anchors.verticalCenter: parent.verticalCenter
        color: theme.textPrimary
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onPressed: {
            parent.dragging = true
            parent.dragFraction = Math.max(0, Math.min(1, (mouse.x - parent.thumbSize / 2) / parent.travel))
            parent.commit(parent.dragFraction)
        }
        onPositionChanged: {
            if (pressed) {
                parent.dragFraction = Math.max(0, Math.min(1, (mouse.x - parent.thumbSize / 2) / parent.travel))
                parent.commit(parent.dragFraction)
            }
        }
        onReleased: parent.dragging = false
    }
}
