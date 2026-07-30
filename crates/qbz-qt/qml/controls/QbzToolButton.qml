// QbzToolButton — the MyQBZ detail toolbar's `ToolBtn`
// (myqbz/MixtapeDetailView.slint:85-151): a 30px bordered chip that is
// icon-only, icon+label, or icon+label+badge, with TWO distinct active looks.
//
// Why not controls/BrowseGenreButton.qml: that one is hard-bound to
// `QbzBridge.genreFilterJson` and builds its own label out of it, so a generic
// label/badge/active-mode contract cannot be a parameter of it.
//
// THE TWO ACTIVE MODES are the whole point, and they are not interchangeable:
//   * `fillActive: true`  — active paints an ACCENT FILL (the Filter button,
//     which is labelled and carries the filter count).
//   * `fillActive: false` — active paints an accent BORDER on the normal
//     elevated fill (the icon-only Select / Reset buttons). Never a fill.
//
// Numbers: height 30, r6, width = the row's own padded width (:97-99, so the
// padding centres the content rather than an absolute x offset), 1px border;
// padding 8/side icon-only and 10/side labelled (:108-109); row spacing 6;
// glyph 15x15; label 12px; badge 16x16 r8 with 10px semibold text.
//
// Where the .slint hardcodes `#ffffff` on the accent fill (:120, :127, :137)
// this uses `theme.accentGlyphTint` / `theme.accentGlyphColor` — the measured
// on-accent selector (theme/QbzTheme.qml, "ON AN ACCENT FILL"). The badge's
// non-filled text stays `accentText` because there it sits on the accent disc,
// which is what that token is for.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    property string name: ""
    property string label: ""
    /// > 0 renders the trailing count disc.
    property int badge: 0
    property bool active: false
    /// Active = accent FILL (labelled buttons) vs accent BORDER (icon-only).
    property bool fillActive: false
    signal clicked()

    QbzTheme { id: theme }

    readonly property bool filled: root.active && root.fillActive
    readonly property int padH: root.label === "" ? 8 : 10

    height: 30
    implicitWidth: 2 * root.padH + row.implicitWidth
    radius: 6
    color: root.filled ? theme.accent
        : (toolArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated)
    border.width: 1
    border.color: (root.active && !root.fillActive) ? theme.accent : theme.borderSubtle

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6

        QbzIcon {
            anchors.verticalCenter: parent.verticalCenter
            name: root.name
            width: 15
            height: 15
            tintName: root.filled ? theme.accentGlyphTint
                : (root.active ? "textPrimary" : "muted")
        }
        Text {
            visible: root.label !== ""
            anchors.verticalCenter: parent.verticalCenter
            text: root.label
            color: root.filled ? theme.accentGlyphColor
                : (root.active ? theme.textPrimary : theme.textSecondary)
            font.pixelSize: 12
            verticalAlignment: Text.AlignVCenter
        }
        Rectangle {
            visible: root.badge > 0
            anchors.verticalCenter: parent.verticalCenter
            width: root.badge > 0 ? 16 : 0
            height: 16
            radius: 8
            color: root.filled ? theme.accentGlyphColor : theme.accent
            Text {
                anchors.centerIn: parent
                text: root.badge
                color: root.filled ? theme.accent : theme.accentText
                font.pixelSize: 10
                font.weight: theme.weightSemibold
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
        }
    }

    MouseArea {
        id: toolArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
