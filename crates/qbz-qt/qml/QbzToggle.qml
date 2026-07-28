// QbzToggle (primitives/QbzToggle.slint) — extracted from SettingsView.qml
// in phase 19 so every settings panel shares the one replica.
// 40x22 pill r11, 16px knob, accent when on, opacity .4 disabled,
// 120ms ease-out knob travel. Emits toggled(newValue); never self-flips.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    property bool checked: false
    property bool enabled: true
    signal toggled(bool value)

    QbzTheme { id: theme }

    width: 40
    height: 22
    radius: 11
    color: checked ? theme.accent : theme.surfaceElevated
    opacity: enabled ? 1.0 : 0.4

    Rectangle {
        width: 16
        height: 16
        radius: 8
        color: theme.textPrimary
        y: 3
        x: parent.checked ? parent.width - width - 3 : 3
        Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutQuad } }
    }
    MouseArea {
        anchors.fill: parent
        cursorShape: parent.enabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (parent.enabled) parent.toggled(!parent.checked)
    }
}
