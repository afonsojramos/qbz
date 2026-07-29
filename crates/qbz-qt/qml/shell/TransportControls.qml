// TransportControls — the shared transport cluster of BOTH now-playing bars
// (TransportControls.slint): track-info · shuffle · prev · play · next ·
// repeat · "+" (Add to… flyout), plus the inline favorite heart in Classic.
// Extracted verbatim from PlayerBar.qml (phase 25 size split) and mounted by
// NowPlayingBarSmall too — its cluster was a byte-for-byte twin at
// playCircle:false / classicActions:false.
//
// 2px spacing on 32px buttons = the same 34px pitch as Tauri's 30px
// `.control-btn` + 4px gap. play-circle = the filled accent disc (New/Large);
// Classic/Small get the plain 34px glyph, 20px in BOTH modes exactly like
// Tauri's size={20}.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Row {
    id: tc

    property bool playCircle: true
    /// Classic ADDS the inline favorite toggle (Tauri parity).
    property bool classicActions: false
    /// Now-playing favorite state, for the Classic heart.
    property bool favorite: false

    /// Emitted by "+" with the button as anchor, so the flyout is owned
    /// by the bar (one menu, both mount points).
    signal addRequested(var anchorItem)
    /// Emitted by the (i) button — the bar owns the Track Info modal.
    signal trackInfoRequested()

    QbzTheme { id: theme }

    // Both are runtime-tinted tokens now (see QbzIconButton's header), so they
    // are the live colours rather than a two-value approximation of them.
    //
    // `tintOnAccent` is the glyph sitting ON the filled play disc, whose fill
    // is theme.accent (theme.accentHover under the cursor). It used to name
    // `accentText` directly; it goes through the shared on-accent selector
    // now. That is NOT a colour change on 34 of the 35 palettes — the
    // selector's first arm IS accent-text — the point is that PLAY, the
    // checkboxes, the genre pills and the circle actions all take one
    // decision. They did not before: CircleAction.slint:72 hardcodes #ffffff
    // for the same job while QbzCheckbox.slint:24 uses accent-text, and the
    // port inherited both spellings. See theme/QbzTheme.qml, "ON AN ACCENT
    // FILL", for why accent-text alone is not enough (rose-pine-dawn: 2.56:1)
    // and why the hover fill does not get a property of its own.
    readonly property string tintStrong: "textPrimary"
    readonly property string tintOnAccent: theme.accentGlyphTint

    spacing: 2
    height: 44

    // Track info — the first control, left of shuffle (1:1 with the
    // other views; it also restores PLAY's symmetric centring).
    QbzIconButton {
        name: "info"
        btnEnabled: QbzPlayer.npHasTrack
        anchors.verticalCenter: parent.verticalCenter
        onClicked: tc.trackInfoRequested()
    }
    QbzIconButton {
        name: "shuffle"
        active: QbzPlayer.npShuffle
        btnEnabled: QbzPlayer.npHasTrack
        anchors.verticalCenter: parent.verticalCenter
        onClicked: QbzPlayer.toggleShuffle()
    }
    QbzIconButton {
        name: "skip-back"
        btnEnabled: QbzPlayer.npHasTrack
        anchors.verticalCenter: parent.verticalCenter
        onClicked: QbzPlayer.previous()
    }
    Rectangle {
        width: tc.playCircle ? 38 : 34
        height: tc.playCircle ? 38 : 34
        radius: tc.playCircle ? width / 2 : theme.radiusSm
        anchors.verticalCenter: parent.verticalCenter
        opacity: QbzPlayer.npHasTrack ? 1.0 : 0.3
        color: tc.playCircle
            ? (playArea.containsMouse && QbzPlayer.npHasTrack ? theme.accentHover : theme.accent)
            : ((playArea.containsMouse && QbzPlayer.npHasTrack) ? theme.surfaceHover : "transparent")
        QbzIcon {
            name: QbzPlayer.npPlaying ? "pause" : "play-fill"
            width: 20
            height: 20
            anchors.centerIn: parent
            tintName: tc.playCircle ? tc.tintOnAccent
                : (playArea.containsMouse ? "accent" : tc.tintStrong)
        }
        MouseArea {
            id: playArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: QbzPlayer.npHasTrack ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (QbzPlayer.npHasTrack) QbzPlayer.togglePlay()
        }
    }
    QbzIconButton {
        name: "skip-forward"
        btnEnabled: QbzPlayer.npHasTrack
        anchors.verticalCenter: parent.verticalCenter
        onClicked: QbzPlayer.next()
    }
    QbzIconButton {
        name: QbzPlayer.npRepeatMode === 2 ? "repeat-1" : "repeat"
        active: QbzPlayer.npRepeatMode > 0
        btnEnabled: QbzPlayer.npHasTrack
        anchors.verticalCenter: parent.verticalCenter
        onClicked: QbzPlayer.cycleRepeat()
    }
    // "+" — Tauri's add-to-playlist button, grouped into the shared
    // "Add to…" flyout in 2.0.
    QbzIconButton {
        id: addBtn
        name: "plus"
        btnEnabled: QbzPlayer.npHasTrack
        anchors.verticalCenter: parent.verticalCenter
        onClicked: tc.addRequested(addBtn)
    }
    FavToggle {
        visible: tc.classicActions
        favorite: tc.favorite
        anchors.verticalCenter: parent.verticalCenter
    }
}
