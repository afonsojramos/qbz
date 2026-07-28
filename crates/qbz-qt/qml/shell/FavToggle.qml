// FavToggle — the inline favorite heart the Classic bar mounts next to "+"
// (Tauri's `.control-btn` heart with fill=currentColor when favorited),
// extracted verbatim from PlayerBar.qml (phase 25 size split).
//
// NOT a QbzIconButton: that control only tints accent/primary/secondary/muted
// and the favorited heart must take the theme's `favorite` red (Slint
// TransportControls uses Theme.favorite the same way).

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root
    /// Favorite state of the now-playing track (QueueState.now-playing-favorite
    /// in Slint) — the host bar already parses the queue document.
    property bool favorite: false

    QbzTheme { id: theme }

    width: 32
    height: 32
    radius: theme.radiusSm
    opacity: QbzPlayer.npHasTrack ? 1.0 : 0.3
    color: (favArea.containsMouse && QbzPlayer.npHasTrack) ? theme.surfaceHover : "transparent"

    QbzIcon {
        name: root.favorite ? "heart-filled" : "heart"
        width: 16
        height: 16
        anchors.centerIn: parent
        tintName: root.favorite ? "favorite"
            : (favArea.containsMouse ? "primary" : "secondary")
    }
    MouseArea {
        id: favArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: QbzPlayer.npHasTrack ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (QbzPlayer.npHasTrack && QbzPlayer.npTrackId !== "")
            QbzQueue.queueToggleFavorite("track", QbzPlayer.npTrackId)
    }
}
