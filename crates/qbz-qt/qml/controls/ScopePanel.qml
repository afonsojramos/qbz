import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    // 0 = goniometer, 1 = pitch-locked oscilloscope.
    property int scopeMode: 0
    property color traceColor: theme.accent
    property bool compact: false

    // Instrument guides stay deliberately quieter than the trace. They are
    // static scene-graph nodes; only the native trace item changes per frame.
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.compact ? 4 : parent.height * 0.08
        width: 1
        height: root.compact ? parent.height - 8 : parent.height * 0.84
        color: Qt.rgba(theme.textMuted.r, theme.textMuted.g, theme.textMuted.b,
                       root.scopeMode === 0 ? 0.22 : 0.12)
    }
    Rectangle {
        x: root.compact ? 4 : parent.width * 0.08
        anchors.verticalCenter: parent.verticalCenter
        width: root.compact ? parent.width - 8 : parent.width * 0.84
        height: 1
        color: Qt.rgba(theme.textMuted.r, theme.textMuted.g, theme.textMuted.b,
                       root.scopeMode === 0 ? 0.22 : 0.15)
    }

    Repeater {
        model: root.scopeMode === 1 && !root.compact ? 3 : 0
        delegate: Rectangle {
            required property int index
            x: parent.width * (index + 1) / 4
            y: parent.height * 0.14
            width: 1
            height: parent.height * 0.72
            color: Qt.rgba(theme.textMuted.r, theme.textMuted.g, theme.textMuted.b, 0.08)
        }
    }

    Rectangle {
        visible: root.scopeMode === 0
        anchors.centerIn: parent
        width: parent.width * (root.compact ? 0.76 : 0.84)
        height: width
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(theme.textMuted.r, theme.textMuted.g, theme.textMuted.b, 0.18)
        radius: width / 2
    }
    Rectangle {
        visible: root.scopeMode === 0 && !root.compact
        anchors.centerIn: parent
        width: parent.width * 0.46
        height: width
        color: "transparent"
        border.width: 1
        border.color: Qt.rgba(theme.textMuted.r, theme.textMuted.g, theme.textMuted.b, 0.09)
        radius: width / 2
    }
    Repeater {
        model: root.scopeMode === 0 && !root.compact ? 2 : 0
        delegate: Rectangle {
            required property int index
            anchors.centerIn: parent
            width: parent.width * 0.76
            height: 1
            rotation: index === 0 ? 45 : -45
            color: Qt.rgba(theme.textMuted.r, theme.textMuted.g, theme.textMuted.b, 0.08)
        }
    }

    ScopeTraceItem {
        anchors.fill: parent
        anchors.margins: root.compact ? 3 : 8
        mode: root.scopeMode
        points: root.scopeMode === 0 ? QbzViz.goniometer : QbzViz.oscilloscope
        traceColor: root.traceColor
        lineWidth: root.compact ? 1.35 : 2.2
        trailDepth: root.scopeMode === 0 ? (root.compact ? 3 : 5) : (root.compact ? 2 : 4)
        fillOpacity: root.compact ? 0.08 : 0.16
    }

    Item {
        visible: root.scopeMode === 0 && !root.compact
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 12
        width: 132
        height: 18

        Rectangle {
            anchors.left: parent.left
            anchors.right: value.left
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            height: 2
            radius: 1
            color: Qt.rgba(theme.textMuted.r, theme.textMuted.g, theme.textMuted.b, 0.22)
            Rectangle {
                width: 4
                height: 10
                radius: 2
                anchors.verticalCenter: parent.verticalCenter
                x: (parent.width - width) * Math.max(0, Math.min(1,
                    (QbzViz.stereoCorrelation + 1) * 0.5))
                color: root.traceColor
            }
        }
        Text {
            id: value
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: QbzViz.stereoCorrelation.toFixed(2)
            color: theme.textMuted
            font.pixelSize: 11
        }
    }
}
