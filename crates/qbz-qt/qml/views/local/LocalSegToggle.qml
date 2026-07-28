// Two-segment icon toggle — ONE component for both ViewToggle (grid/list,
// LocalLibraryView.slint:358) and ModeToggle (flat/tree, :384). Slint keeps
// them as two components because its ToggleButton cannot take a model; both
// bodies are otherwise byte-identical (60x30, radius 6, surface-elevated,
// two ToggleButton `sm` children), so here they are one component driven by
// a `segments` model.
//
// ADR-008: small-radius rectangle, never a pill.

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Rectangle {
    id: root

    /// [{ id, icon }] — exactly two entries on every current call site.
    property var segments: []
    property string mode: ""
    signal setMode(string value)

    QbzTheme { id: theme }

    width: 60
    height: 30
    radius: 6
    color: theme.surfaceElevated
    clip: true

    Row {
        anchors.centerIn: parent
        spacing: 2
        Repeater {
            model: root.segments
            delegate: Rectangle {
                id: seg
                required property var modelData
                readonly property bool active: modelData.id === root.mode
                width: 26
                height: 24
                radius: 4
                color: active ? theme.surfaceMain
                     : segArea.containsMouse ? theme.surfaceHover : "transparent"
                QbzIcon {
                    name: seg.modelData.icon
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    tintName: seg.active ? "accent" : "secondary"
                }
                MouseArea {
                    id: segArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.setMode(seg.modelData.id)
                }
            }
        }
    }
}
