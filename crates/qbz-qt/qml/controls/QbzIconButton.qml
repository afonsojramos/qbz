// QbzIconButton — the square icon button (Slint BarControls IconButton /
// HeaderIconButton), consolidated in phase 22 from PlayerBar.BarIconBtn +
// NowPlayingBarSmall.BarIconBtn (verbatim twins), QueuePanel.PanelIconBtn
// and HeaderBar.HeaderIconBtn.
// Arms: `btnSize` (32 default; 30 compact panels, 36 header), `iconSize`,
// `active` (accent glyph), `activeBackground` (HeaderBar: elevated fill
// when active), `btnEnabled` (dims 0.3 + muted glyph, disarms).
//
// LIGHT-THEME CORRECTNESS: the glyph tints are PRE-BAKED SVG colours
// (assets/icons/<tint>/) — "primary" is literally #ffffff and "secondary"
// #cccccc, i.e. the DARK theme's text-primary / text-secondary frozen into
// the asset. On a light theme those render near-white on a light surface and
// the whole button set disappears, which is the reported bug. The bake is
// therefore chosen from the LIVE token's lightness instead of being
// hardcoded: bright token -> the light bake, dark token -> the dark bake.
// TODO(glue): the durable home for this is a QbzTheme.tintFor(token) helper
// (or real runtime tinting in QbzIcon) — see the report; it is duplicated in
// FavToggle / TransportControls / SongCard until then.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: btn
    property string name: ""
    property int btnSize: 32
    property int iconSize: 16
    property bool active: false
    property bool activeBackground: false
    property bool btnEnabled: true
    signal clicked()

    QbzTheme { id: theme }

    // Bake that actually matches Theme.text-primary / text-secondary in the
    // ACTIVE theme (BarControls.slint IconButton: hover -> text-primary,
    // idle -> text-secondary). text-muted maps to the "muted" bake in both
    // polarities: #888888 is mid-grey and reads on light and dark alike.
    readonly property string tintStrong: theme.textPrimary.hslLightness > 0.5 ? "primary" : "black"
    readonly property string tintWeak: theme.textSecondary.hslLightness > 0.5 ? "secondary" : "muted"

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
            : biArea.containsMouse ? btn.tintStrong : btn.tintWeak
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
