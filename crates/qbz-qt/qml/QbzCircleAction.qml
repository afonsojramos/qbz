// QbzCircleAction — the on-surface circular header action (Slint
// primitives/CircleAction.slint on-surface arm), consolidated in phase 22
// from THREE copies (AlbumView.CircleBtn ≡ ArtistView.CircleBtn,
// PlaylistView.CircleBtn = superset with the `primary` + `btnEnabled`
// arms). 32px elevated disc + 1.5px muted ring (active = accent glyph);
// `primary` = 40px accent disc (black glyph).
// NOTE: the OVERLAY-palette circle buttons (on artwork) are the separate
// CardOverlayButton.qml — different palette, deliberately not merged.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    property string name: ""
    property bool active: false
    property bool primary: false
    property bool btnEnabled: true
    signal clicked(var mouse)

    QbzTheme { id: theme }

    width: primary ? 40 : 32
    height: primary ? 40 : 32
    radius: width / 2
    color: primary ? (cbArea.containsMouse && btnEnabled ? theme.accentHover : theme.accent)
         : (cbArea.containsMouse || active) ? theme.surfaceHover : theme.surfaceElevated
    border.width: primary ? 0 : 1.5
    border.color: theme.borderMuted
    opacity: btnEnabled ? 1.0 : 0.4
    QbzIcon {
        name: parent.name
        width: primary ? 20 : 15
        height: primary ? 20 : 15
        anchors.centerIn: parent
        tintName: parent.primary ? "black" : (parent.active ? "accent" : "primary")
    }
    MouseArea {
        id: cbArea
        anchors.fill: parent
        enabled: parent.btnEnabled
        hoverEnabled: true
        cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: function (mouse) { parent.clicked(mouse) }
    }
}
