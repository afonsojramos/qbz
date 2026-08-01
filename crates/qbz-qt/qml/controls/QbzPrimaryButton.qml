// QbzPrimaryButton — the accent-filled labelled button
// (primitives/QbzPrimaryButton.slint). The port had no such control outside
// LoginScreen.qml's inline copy; MyQBZ has five call sites (the Add modal's
// "Create & Add", Create's "+ New Mixtape/Collection", Edit's Save/Delete,
// Mix's "Add to queue", the builder footer's "Create Collection").
// Promoting LoginScreen's copy onto this file is follow-up debt, not part of
// this migration.
//
// THE API IS `btnEnabled` / `btnHeight`, NOT `enabled` / `height`:
// `enabled` shadows Item.enabled (which also disarms child MouseAreas
// through a different mechanism) and the call sites all pass an explicit
// height — 36 in the builder footer, 38 in all four modals, 48 default
// (QbzPrimaryButton.slint:15).
//
// Numbers, off the .slint: r8 (:16), opacity 0.5 when disabled (:17 — NOT
// the 0.4 SettingsButton uses), accent / accent-hover fill (:24) with a
// 150ms transition (:26), destructive = SOLID danger darkened 15% on hover
// (:22-23) — `Theme.danger-hover` is a translucent tint for danger-BG
// surfaces and would render the button pale (the rationale is spelled out at
// :18-21). Padding 20 per side so `implicitWidth = label + 40` (:31-34);
// label Typography.button (17px) semibold (:36-41).
//
// The label colour is `theme.accentGlyphColor`, not a hardcoded
// `Theme.accent-text`: the owner-approved measured on-accent selector
// (theme/QbzTheme.qml, "ON AN ACCENT FILL"). Do not "restore" #ffffff here.
//
// NOT ported (deliberate, both are port-wide gaps rather than MyQBZ ones):
// the .slint's FocusScope Enter/Space activation and its keyboard focus ring
// (:62-86). This port has no keyboard/shortcut layer at all — the whole tree
// carries two key handlers (QbzLineEdit.qml:157, HeaderBar.qml:551) — so
// wiring one here would be the only focusable button in the app.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    property string label: ""
    /// NOT `enabled` — see the header.
    property bool btnEnabled: true
    property bool destructive: false
    property int btnHeight: 48
    /// Label size. The control draws Typography.button (17px), which is right
    /// for the footer chips it was written for; the folder / playlist modals
    /// are 15px (Typography.body) in the reference, and hardcoding 17 there
    /// would be a visible parity break on every one of them.
    property int labelSize: theme.fontButton
    signal clicked()

    QbzTheme { id: theme }

    // Height is pinned; the width follows the label so the button auto-sizes
    // as a footer chip, and a call site that wants a full-width button
    // assigns `width` (Item.width defaults to implicitWidth).
    height: root.btnHeight
    implicitWidth: lbl.implicitWidth + 40
    radius: theme.radiusSm
    opacity: root.btnEnabled ? 1.0 : 0.5

    color: root.destructive
        ? ((btnArea.containsMouse && root.btnEnabled)
            ? Qt.darker(theme.danger, 1.15) : theme.danger)
        : ((btnArea.containsMouse && root.btnEnabled)
            ? theme.accentHover : theme.accent)
    Behavior on color { NumberAnimation { duration: 150 } }

    Text {
        id: lbl
        anchors.centerIn: parent
        width: Math.min(implicitWidth, root.width - 40)
        text: root.label
        color: theme.accentGlyphColor
        font.pixelSize: root.labelSize
        font.weight: theme.weightSemibold
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    MouseArea {
        id: btnArea
        anchors.fill: parent
        enabled: root.btnEnabled
        hoverEnabled: root.btnEnabled
        cursorShape: root.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: root.clicked()
    }
}
