// QbzLineEdit — THE shared text input. Two arms over ONE control, so there
// is never a second search-box implementation in the tree:
//
//   * PLAIN (default, unchanged): 240x34 elevated r8 bordered input that
//     commits on Enter AND on focus loss (the Tauri onchange semantics —
//     PlaybackSettings.slint). Settings panels use this arm.
//
//   * SEARCH (`searchMode`): leading magnifier, LIVE `edited(text)` and a
//     trailing X. With `expandable` it is primitives/ExpandableSearch.slint
//     1:1 — a 34px (30px when `sm`) magnifier slot that, on click, opens a
//     RIGHT-ANCHORED field growing LEFT over whatever sits beside it (200ms
//     ease-in-out), the magnifier cross-fading out (120ms); focus is grabbed
//     one tick later because focusing synchronously inside the open races the
//     open animation (the Slint comment at ExpandableSearch.slint:26); the X
//     clears AND closes; Esc closes. The closed footprint is fixed, so
//     nothing to the control's right ever moves.
//
// Every per-view search box (Local Library toolbars, tree rail, folder
// detail, queue) is this control.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    // --- plain arm (API unchanged) ---------------------------------------
    property string text: ""
    property string placeholder: ""
    property bool isPassword: false
    signal committed(string value)

    // --- search arm -------------------------------------------------------
    /// Leading magnifier + live `edited` + trailing X.
    property bool searchMode: false
    /// Collapse to the magnifier slot (implies searchMode semantics).
    property bool expandable: false
    /// Bootstrap-style small variant (ExpandableSearch `sm`): 30px, not 34px.
    property bool sm: false
    /// Open width of the expandable field (ExpandableSearch max-open-width).
    property int openWidth: 196
    /// surface-elevated fill (toolbar variant) vs surface-card (rail/overlay).
    property bool elevated: true
    property bool open: false
    signal edited(string value)

    QbzTheme { id: theme }

    readonly property int collapsedSize: sm ? 30 : 34
    readonly property int glyph: sm ? 13 : 14

    // The ROOT keeps the closed footprint; only the inner field animates, so
    // a positioner never reflows while the search opens.
    width: expandable ? collapsedSize : 240
    height: (searchMode || expandable) ? collapsedSize : 34
    color: "transparent"

    function clearSearch() {
        input.text = ""
        root.text = ""
        root.edited("")
    }
    function closeSearch() {
        clearSearch()
        root.open = false
        input.focus = false
    }

    // Deferred focus — see the header note.
    onOpenChanged: if (open) focusDefer.restart()
    Timer {
        id: focusDefer
        interval: 30
        repeat: false
        onTriggered: input.forceActiveFocus()
    }

    // --- Closed: the magnifier toggle (fades, so both directions are smooth)
    Rectangle {
        visible: root.expandable
        x: root.width - width
        width: root.collapsedSize
        height: root.collapsedSize
        radius: 6
        opacity: root.open ? 0.0 : 1.0
        Behavior on opacity { NumberAnimation { duration: 120 } }
        color: openArea.containsMouse ? theme.surfaceHover : "transparent"
        QbzIcon {
            name: "search"
            width: root.sm ? 14 : 16
            height: root.sm ? 14 : 16
            anchors.centerIn: parent
            tintName: openArea.containsMouse ? "primary" : "muted"
        }
        MouseArea {
            id: openArea
            anchors.fill: parent
            enabled: !root.open
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.open = true
        }
    }

    // --- The field. Right-anchored, so an expandable one grows LEFT. ------
    Rectangle {
        id: box
        x: root.width - width
        width: root.expandable ? (root.open ? root.openWidth : 0) : root.width
        height: root.height
        radius: (root.searchMode || root.expandable) ? 6 : theme.radiusSm
        color: root.elevated ? theme.surfaceElevated : theme.surfaceCard
        border.width: (root.expandable && !root.open) ? 0 : 1
        border.color: input.activeFocus ? theme.accent : theme.borderSubtle
        clip: true
        Behavior on width {
            enabled: root.expandable
            NumberAnimation { duration: 200; easing.type: Easing.InOutQuad }
        }

        Row {
            anchors.fill: parent
            anchors.leftMargin: root.searchMode ? (root.expandable ? 11 : 9) : 12
            anchors.rightMargin: root.searchMode ? 5 : 12
            spacing: root.searchMode ? 7 : 0

            QbzIcon {
                visible: root.searchMode
                name: "search"
                width: root.glyph
                height: root.glyph
                anchors.verticalCenter: parent.verticalCenter
                tintName: "muted"
            }
            Item {
                // Clamped: the open animation passes through width 0 and a
                // negative width is a silent layout trap.
                width: Math.max(0, parent.width
                    - (root.searchMode ? root.glyph + 7 : 0)
                    - (clearSlot.visible ? clearSlot.width + 7 : 0))
                height: parent.height
                clip: true
                TextInput {
                    id: input
                    anchors.fill: parent
                    color: theme.textPrimary
                    font.pixelSize: root.searchMode ? 12 : theme.fontBody
                    verticalAlignment: Text.AlignVCenter
                    clip: true
                    selectByMouse: true
                    echoMode: root.isPassword ? TextInput.Password : TextInput.Normal
                    text: root.text
                    onAccepted: root.committed(text)
                    onActiveFocusChanged: if (!activeFocus) root.committed(text)
                    onTextEdited: root.edited(text)
                    Keys.onEscapePressed: function (event) {
                        if (root.expandable) {
                            root.closeSearch()
                            event.accepted = true
                        } else {
                            event.accepted = false
                        }
                    }
                    // External republish (e.g. Reset) re-seeds the field while
                    // it is not being edited.
                    Binding {
                        target: input
                        property: "text"
                        value: root.text
                        when: !input.activeFocus
                    }
                }
                Text {
                    visible: input.text === ""
                    anchors.fill: parent
                    text: root.placeholder
                    color: theme.textMuted
                    font.pixelSize: root.searchMode ? 12 : theme.fontBody
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
            }
            // Trailing X — clears (plain search) or clears AND closes
            // (expandable, back to the magnifier).
            Item {
                id: clearSlot
                visible: root.searchMode && (root.expandable || input.text !== "")
                width: visible ? (root.sm ? 22 : 24) : 0
                height: parent.height
                Rectangle {
                    anchors.centerIn: parent
                    width: root.sm ? 22 : 24
                    height: root.sm ? 22 : 24
                    radius: 4
                    color: clearArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        name: "x"
                        width: 12
                        height: 12
                        anchors.centerIn: parent
                        tintName: clearArea.containsMouse ? "primary" : "muted"
                    }
                    MouseArea {
                        id: clearArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.expandable ? root.closeSearch() : root.clearSearch()
                    }
                }
            }
        }
    }
}
