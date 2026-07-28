// Custom vertical scrollbar — replica of primitives/ListScrollbar.slint.
// 14px gutter (not over the rows), 6px #ffffff12 track, 8px thumb
// (#ffffff44, widens to 10px / #ffffff77 on hover|drag), 48px min thumb,
// auto-hide 900ms after the last scroll. Attach as a sibling anchored to
// the right of a Flickable/GridView/ListView and set `target`.

import QtQuick

Item {
    id: root
    required property Flickable target

    width: 14
    visible: maxScroll > 0

    readonly property real maxScroll: Math.max(0, target.contentHeight - target.height)
    readonly property real thumbH: target.contentHeight > 0
        ? Math.max(48, target.height / target.contentHeight * height)
        : height
    readonly property real travel: Math.max(0, height - thumbH)
    // Shown while scrolling, hovering, or dragging (auto-hide).
    property bool scrollActive: false
    readonly property bool shown: scrollActive || barArea.containsMouse || barArea.pressed

    Connections {
        target: root.target
        function onContentYChanged() {
            root.scrollActive = true
            hideTimer.restart()
        }
    }
    Timer {
        id: hideTimer
        interval: 900
        onTriggered: root.scrollActive = false
    }

    // Track.
    Rectangle {
        width: 6
        x: Math.round((parent.width - width) / 2)
        height: parent.height
        radius: 3
        color: "#12ffffff"
        opacity: root.shown ? 1.0 : 0.0
        Behavior on opacity { NumberAnimation { duration: 160 } }
    }
    // Thumb.
    Rectangle {
        width: barArea.containsMouse || barArea.pressed ? 10 : 8
        x: Math.round((parent.width - width) / 2)
        height: root.thumbH
        y: root.maxScroll > 0 ? (root.target.contentY / root.maxScroll) * root.travel : 0
        radius: 5
        color: barArea.containsMouse || barArea.pressed ? "#77ffffff" : "#44ffffff"
        opacity: root.shown ? 1.0 : 0.0
        Behavior on width { NumberAnimation { duration: 100 } }
        Behavior on opacity { NumberAnimation { duration: 160 } }
    }
    // Drag / click-to-position over the whole gutter.
    MouseArea {
        id: barArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onPressed: position(mouseY)
        onPositionChanged: if (pressed) position(mouseY)
        function position(my) {
            if (root.travel <= 0) return
            var frac = Math.min(1, Math.max(0, (my - root.thumbH / 2) / root.travel))
            root.target.contentY = frac * root.maxScroll
        }
    }
}
