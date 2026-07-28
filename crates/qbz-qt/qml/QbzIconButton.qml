// QbzIconButton — the square icon button (Slint BarControls IconButton /
// HeaderIconButton), consolidated in phase 22 from PlayerBar.BarIconBtn +
// NowPlayingBarSmall.BarIconBtn (verbatim twins), QueuePanel.PanelIconBtn
// and HeaderBar.HeaderIconBtn.
// Arms: `btnSize` (32 default; 30 compact panels, 36 header), `iconSize`,
// `active` (accent glyph), `activeBackground` (HeaderBar: elevated fill
// when active), `btnEnabled` (dims 0.3 + muted glyph, disarms).

import QtQuick
import com.blitzfc.qbz

Rectangle {
    property string name: ""
    property int btnSize: 32
    property int iconSize: 16
    property bool active: false
    property bool activeBackground: false
    property bool btnEnabled: true
    signal clicked()

    QbzTheme { id: theme }

    width: btnSize
    height: btnSize
    radius: theme.radiusSm
    opacity: btnEnabled ? 1.0 : 0.3
    color: (biArea.containsMouse && btnEnabled) ? theme.surfaceHover
        : (active && activeBackground) ? theme.surfaceElevated : "transparent"
    QbzIcon {
        name: parent.name
        width: parent.iconSize
        height: parent.iconSize
        anchors.centerIn: parent
        tintName: !parent.btnEnabled ? "muted"
            : parent.active ? "accent"
            : biArea.containsMouse ? "primary" : "secondary"
    }
    MouseArea {
        id: biArea
        anchors.fill: parent
        enabled: parent.btnEnabled
        hoverEnabled: true
        cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (parent.btnEnabled) parent.clicked()
    }
}
