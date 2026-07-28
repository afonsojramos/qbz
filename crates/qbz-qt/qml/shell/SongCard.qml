// SongCard — the now-playing card of the full bar (cover + title + context
// meta + the in-card audio stamp), extracted VERBATIM from PlayerBar.qml
// (phase 25 size split). 1:1 with crates/qbz-ui/ui/shell/SongCard.slint.
//
// `glass` renders the Classic contained card (60px art, 64px tall, radius 8,
// surface-elevated fill); New uses the flush 74px variant. Large mounts it
// with showArt/showBadges false — the cover lives in the sidebar dock and the
// AudioStamp moves to the right cluster, so the text column reclaims the
// stamp's 132px.
//
// The title click opens Track Info (SongCard.slint: `root.track-info()` from
// the title TouchArea, Qobuz-only) — the host bar owns the modal, this just
// emits `trackInfoRequested()`.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    property bool showArt: true
    property bool showBadges: true
    property bool glass: false
    property int artSize: glass ? 60 : 74

    /// Title clicked — the host opens the Track Info modal.
    signal trackInfoRequested()

    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    // Track Info is a Qobuz-only surface (Slint gates it on
    // NowPlayingState.source == "qobuz"). The Qt port publishes no np_source,
    // so a numeric track id is the proxy — local / Plex / ephemeral rows do
    // not carry one. TODO(glue): publish np_source (see the report).
    readonly property bool qobuzTrack: QbzPlayer.npHasTrack
        && /^[0-9]+$/.test(QbzPlayer.npTrackId)

    QbzTheme { id: theme }

    width: 200
    height: glass ? 64 : 74
    radius: glass ? 8 : 0
    color: ambientOn ? "transparent" : (glass ? theme.surfaceElevated : "transparent")

    Row {
        anchors.left: parent.left
        // SongCard.slint's inner-row width formula: the card minus the
        // stamp + a 4px gap ONLY when the stamp is rendered (Large hides
        // it — the AudioStamp moves to the right column — so the text
        // column must reclaim those 132px).
        anchors.right: stamp.visible ? stamp.left : parent.right
        anchors.rightMargin: stamp.visible ? 4 : 0
        height: parent.height
        spacing: 0

        Item {
            visible: root.showArt
            width: root.showArt ? root.artSize : 0
            height: parent.height
            Rectangle {
                width: root.artSize
                height: root.artSize
                anchors.centerIn: parent
                radius: 6
                color: theme.surfaceElevated
                clip: true
                RoundedImage {
                    visible: QbzPlayer.npHasTrack
                    anchors.fill: parent
                    source: QbzPlayer.npArtworkPath
                    radius: 6
                }
                QbzIcon {
                    visible: !QbzPlayer.npHasTrack
                    name: "music"
                    width: root.artSize * 0.5
                    height: root.artSize * 0.5
                    anchors.centerIn: parent
                    tintName: "muted"
                }
                // Fetch overlay (resolving/downloading).
                Rectangle {
                    visible: QbzPlayer.npHasTrack && QbzPlayer.npLoading
                    anchors.fill: parent
                    radius: 6
                    color: "#aa000000"
                    QbzSpinner {
                        width: root.artSize * 0.5
                        height: root.artSize * 0.5
                        anchors.centerIn: parent
                        size: root.artSize * 0.5
                    }
                }
            }
        }
        Item { width: root.showArt ? 11 : 0; height: 1 }

        Column {
            width: parent.width - (root.showArt ? root.artSize + 11 : 0)
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Text {
                width: parent.width
                text: QbzPlayer.npHasTrack ? QbzPlayer.npTitle
                    : QbzSession.tr("Nothing playing", QbzSession.trRev)
                color: theme.textPrimary
                font.pixelSize: theme.fontBody
                font.weight: theme.weightMedium
                elide: Text.ElideRight
                // Title -> Track Info (SongCard.slint). The MouseArea is a
                // CHILD of the Text so it can't take layout space in the
                // Column.
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: root.qobuzTrack ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: if (root.qobuzTrack) root.trackInfoRequested()
                }
            }
            Row {
                visible: QbzPlayer.npHasTrack
                spacing: 7
                height: 18
                // Context-stack icon ("Show track playing context" pref).
                Rectangle {
                    visible: QbzPlayer.showContextIcon
                    width: 16
                    height: 18
                    radius: 3
                    color: ctxArea.containsMouse ? theme.surfaceHover : "transparent"
                    QbzIcon {
                        name: "layers"
                        width: 13
                        height: 13
                        anchors.verticalCenter: parent.verticalCenter
                        tintName: ctxArea.containsMouse ? "primary" : "muted"
                    }
                    MouseArea {
                        id: ctxArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: if (QbzPlayer.npAlbumId !== "") QbzAlbum.openAlbum(QbzPlayer.npAlbumId)
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzPlayer.npArtist
                    color: artistArea.containsMouse && QbzPlayer.npArtistId !== "" ? theme.accent : theme.textMuted
                    font.pixelSize: 12
                    elide: Text.ElideRight
                    MouseArea {
                        id: artistArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: QbzPlayer.npArtistId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (QbzPlayer.npArtistId !== "") QbzArtist.openArtist(QbzPlayer.npArtistId)
                    }
                }
                Rectangle {
                    width: 3
                    height: 3
                    radius: 1.5
                    color: theme.textMuted
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: QbzPlayer.npAlbum
                    color: albumArea.containsMouse && QbzPlayer.npAlbumId !== "" ? theme.accent : theme.textMuted
                    font.pixelSize: 12
                    elide: Text.ElideRight
                    MouseArea {
                        id: albumArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: QbzPlayer.npAlbumId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (QbzPlayer.npAlbumId !== "") QbzAlbum.openAlbum(QbzPlayer.npAlbumId)
                    }
                }
            }
        }
    }

    SongCardStamp {
        id: stamp
        visible: root.showBadges && QbzPlayer.npHasTrack
        anchors.right: parent.right
        // Classic (glass) insets the stamp by pad + stamp-right-margin
        // (2px each) so it clears the visible card border.
        anchors.rightMargin: root.glass ? 4 : 0
        anchors.verticalCenter: parent.verticalCenter
    }
}
