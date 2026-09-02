// QbzToggle (primitives/QbzToggle.slint) — extracted from SettingsView.qml
// in phase 19 so every settings panel shares the one replica.
// 40x22 pill r11, 16px knob, accent when on, opacity .4 disabled,
// 120ms ease-out knob travel. Emits toggled(newValue); never self-flips.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    property bool checked: false
    property bool enabled: true
    signal toggled(bool value)

    QbzTheme { id: theme }

    width: 40
    height: 22
    radius: 11
    color: checked ? theme.accent : theme.surfaceElevated
    opacity: enabled ? 1.0 : 0.4
    activeFocusOnTab: enabled
    Accessible.role: Accessible.CheckBox
    Accessible.checked: checked
    border.width: activeFocus ? 2 : 0
    border.color: checked ? theme.accentGlyphColor : theme.accent

    // Space is the conventional toggle key. Accepting it here is
    // load-bearing: it prevents the event from bubbling to AppShell's global
    // play/pause binding while this control owns keyboard focus.
    Keys.onPressed: function (event) {
        if (root.enabled && !event.isAutoRepeat
                && (event.key === Qt.Key_Space
                    || event.key === Qt.Key_Return
                    || event.key === Qt.Key_Enter)) {
            root.toggled(!root.checked)
            event.accepted = true
        }
    }
    Accessible.onToggleAction: if (root.enabled) root.toggled(!root.checked)

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
        onPressed: if (root.enabled) root.forceActiveFocus()
        onClicked: if (root.enabled) root.toggled(!root.checked)
    }
}
