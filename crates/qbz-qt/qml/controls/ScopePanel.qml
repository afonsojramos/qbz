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

    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.width: 1
        border.color: theme.borderSubtle
        radius: theme.radiusMd
    }

    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 8
        width: 1
        height: parent.height - 16
        color: theme.borderSubtle
    }
    Rectangle {
        x: 8
        anchors.verticalCenter: parent.verticalCenter
        width: parent.width - 16
        height: 1
        color: theme.borderSubtle
    }
    Rectangle {
        visible: root.scopeMode === 0
        anchors.centerIn: parent
        width: parent.width * 0.5
        height: parent.height * 0.5
        color: "transparent"
        border.width: 1
        border.color: theme.borderSubtle
        radius: width / 2
    }

    ScopeTraceItem {
        anchors.fill: parent
        anchors.margins: root.compact ? 3 : 8
        mode: root.scopeMode
        points: root.scopeMode === 0 ? QbzViz.goniometer : QbzViz.oscilloscope
        traceColor: root.traceColor
        lineWidth: root.compact ? 1.0 : 1.5
    }

    Text {
        visible: root.scopeMode === 0 && !root.compact
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: 12
        text: QbzViz.stereoCorrelation.toFixed(2)
        color: theme.textMuted
        font.pixelSize: 11
    }
}
