// PlayerBar — the full 112px now-playing bar for modes 0 (New), 1
// (Classic) and 3 (Large) — QML port of crates/qbz-ui/ui/shell/
// PlayerBar.slint (phase 18). Mode 2 (Small) mounts NowPlayingBarSmall
// instead (the NowPlayingBar seam picks the body).
//
// Composition (1:1): a full-width SeekBar with times (6px margins; in
// Large the left edge insets by the sidebar width so the seek arm clears
// the docked cover), then the controls row in three responsive symmetric
// columns (the side columns share a fraction so PLAY lands on the window
// centre at every width):
//   width <  1366  -> 39% | 22% | 39%
//   1366..<1920    -> 30% | 40% | 30%
//   width >= 1920  -> 25% | 50% | 25%
// Classic uses a fixed 32.4/35.2/32.4 split (col-side 0.324).
//
//   New (0):     LEFT SongCard · CENTER transport (filled disc) · RIGHT cluster
//   Classic (1): LEFT transport · CENTER glass SongCard (<=560px) · RIGHT cluster
//   Large (3):   LEFT SongCard WITHOUT the cover (it lives in the sidebar
//                dock, shifted right to clear it) · CENTER transport ·
//                RIGHT small AudioStamp + cluster.
//
// Wired: transport (shuffle/prev/play/next/repeat), seek, volume+mute,
// lyrics, queue, the Now-Playing-view flyout (npbSetMode), open
// album/artist via the SongCard meta links. Inert (POC-NOTEs): Connect
// (device flyout), track-info, the dot-LED output badges use the real
// backend state but the volume LOCK (ALSA hw / remote) is not enforced.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    readonly property bool largeActive: QbzShell.npbMode === 3 && QbzShell.sidebarState === 0
    readonly property bool isClassic: QbzShell.npbMode === 1

    QbzTheme { id: theme }

    // Responsive zones (PlayerBar.slint). Classic fixes col-side at 0.324;
    // New/Large use the width-driven side fraction. Both keep the two side
    // columns EQUAL so the centre column stays dead centre.
    property real sideFrac: root.width >= 1920 ? 0.25 : (root.width >= 1366 ? 0.30 : 0.39)
    property real colSide: isClassic ? 0.324 : sideFrac
    property real colCentre: 1.0 - 2.0 * colSide
    // The docked-cover width the Large seek arm + song card must clear
    // (sidebar rendered width; 240 when open).
    readonly property int dockWidth: largeActive ? 240 : 0

    readonly property var settingsDoc: parseSettings()
    function parseSettings() {
        try {
            return JSON.parse(QbzBridge.settingsJson)
        } catch (e) {
            return ({})
        }
    }
    readonly property bool backendPipewire: settingsDoc.backendIsPipewire === true
    readonly property bool dacPassthrough: settingsDoc.dacPassthrough === true

    function fmt(secs) {
        var m = Math.floor(secs / 60)
        var s = Math.floor(secs % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    // --- Shared bits -------------------------------------------------------

    // Square compact icon button (BarControls.IconButton).


    // Dot LED (circle-dot on / circle off) + constant-colour label — the
    // New-mode output indicators (SongCard dot-leds).
    component DotLed: Row {
        property string label: ""
        property bool on: false
        spacing: 4
        height: 12
        Rectangle {
            width: 8
            height: 8
            radius: 4
            anchors.verticalCenter: parent.verticalCenter
            color: parent.on ? theme.accent : "transparent"
            border.width: 1
            border.color: parent.on ? theme.accent : theme.textMuted
        }
        Text {
            text: parent.label
            color: theme.textMuted
            font.pixelSize: 9
            font.weight: theme.weightSemibold
            font.letterSpacing: 0.5
            verticalAlignment: Text.AlignVCenter
        }
    }

    // The audio stamp (quality badge + dot LEDs) — New mode keeps the full
    // stamp inside the SongCard; Large mounts a compact one on the right
    // column's left wall.
    component AudioStamp: Row {
        property bool showLeds: true
        spacing: 8
        height: 30
        layoutDirection: Qt.RightToLeft

        Row {
            spacing: 5
            layoutDirection: Qt.RightToLeft
            anchors.verticalCenter: parent.verticalCenter
            Text {
                visible: QbzPlayer.npQualityLabel !== ""
                text: QbzPlayer.npQualityLabel
                font.pixelSize: 9
                font.weight: theme.weightSemibold
                color: theme.textPrimary
                elide: Text.ElideRight
                anchors.verticalCenter: parent.verticalCenter
            }
            QualityMini {
                // The stamp shows the cd glyph for ANY non-hires tier with
                // a label (not just "cd") — map that exactly.
                tier: QbzPlayer.npQualityTier === "hires" ? "hires"
                    : (QbzPlayer.npQualityLabel !== "" ? "cd" : "")
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        Column {
            visible: showLeds
            anchors.verticalCenter: parent.verticalCenter
            spacing: 1
            DotLed { label: "PIPEWIRE"; on: root.backendPipewire }
            DotLed { label: "DACPASS"; on: root.dacPassthrough }
        }
    }

    // SongCard (SongCard.slint): cover + title + context meta + the in-card
    // audio stamp. `glass` renders the Classic contained card.
    component SongCard: Rectangle {
        property bool showArt: true
        property bool showBadges: true
        property bool glass: false
        property int artSize: glass ? 60 : 74
        width: 200
        height: glass ? 64 : 74
        radius: glass ? 8 : 0
        color: ambientOn ? "transparent" : (glass ? theme.surfaceElevated : "transparent")

        Row {
            anchors.left: parent.left
            anchors.right: stamp.left
            anchors.rightMargin: 4
            height: parent.height
            spacing: 0

            Item {
                visible: showArt
                width: showArt ? artSize : 0
                height: parent.height
                Rectangle {
                    width: artSize
                    height: artSize
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
                        width: artSize * 0.5
                        height: artSize * 0.5
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
                            width: artSize * 0.5
                            height: artSize * 0.5
                            anchors.centerIn: parent
                            size: artSize * 0.5
                        }
                    }
                }
            }
            Item { width: showArt ? 11 : 0; height: 1 }

            Column {
                width: parent.width - (showArt ? artSize + 11 : 0)
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                Text {
                    width: parent.width
                    text: QbzPlayer.npHasTrack ? QbzPlayer.npTitle : QbzSession.tr("Nothing playing", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontBody
                    font.weight: theme.weightMedium
                    elide: Text.ElideRight
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
                            onClicked: if (QbzPlayer.npAlbumId !== "") QbzBridge.openAlbum(QbzPlayer.npAlbumId)
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
                            onClicked: if (QbzPlayer.npArtistId !== "") QbzBridge.openArtist(QbzPlayer.npArtistId)
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
                            onClicked: if (QbzPlayer.npAlbumId !== "") QbzBridge.openAlbum(QbzPlayer.npAlbumId)
                        }
                    }
                }
            }
        }
        AudioStamp {
            id: stamp
            visible: showBadges && QbzPlayer.npHasTrack
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    // Transport cluster (TransportControls.slint): shuffle · prev · play ·
    // next · repeat. play-circle = the filled 44px accent disc (New/Large);
    // classic-actions = plain glyphs, left-aligned (Classic).
    component TransportControls: Row {
        property bool playCircle: true
        spacing: playCircle ? 12 : 4
        height: 44

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
            width: playCircle ? 44 : 32
            height: playCircle ? 44 : 32
            radius: width / 2
            anchors.verticalCenter: parent.verticalCenter
            opacity: QbzPlayer.npHasTrack ? 1.0 : 0.3
            color: playCircle ? (playArea.containsMouse && QbzPlayer.npHasTrack ? theme.accentHover : theme.accent)
                : ((playArea.containsMouse && QbzPlayer.npHasTrack) ? theme.surfaceHover : "transparent")
            QbzIcon {
                name: QbzPlayer.npPlaying ? "pause" : "play-fill"
                width: playCircle ? 20 : 16
                height: playCircle ? 20 : 16
                anchors.centerIn: parent
                tintName: playCircle ? "black" : "primary"
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
    }

    // ============================ the bar =================================
    Column {
        anchors.fill: parent
        spacing: 0

        Item { width: 1; height: 6 }

        // Full-width seek bar (times at the ends). In Large the seek arm is
        // LEFT-INSET by the docked-cover width so it begins right of the
        // dock (AppShell.slint's large-active padding).
        Item {
            width: parent.width
            height: 20
            Text {
                x: root.dockWidth > 0 ? root.dockWidth + 6 : 6
                anchors.verticalCenter: parent.verticalCenter
                visible: QbzPlayer.npHasTrack
                text: root.fmt(QbzPlayer.npElapsedSecs)
                color: theme.textMuted
                font.pixelSize: 11
            }
            Rectangle {
                id: seekTrack
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.leftMargin: (root.dockWidth > 0 ? root.dockWidth + 6 : 6) + (QbzPlayer.npHasTrack ? 46 : 0)
                anchors.rightMargin: 6 + (QbzPlayer.npHasTrack ? 46 : 0)
                height: 4
                radius: 2
                anchors.verticalCenter: parent.verticalCenter
                color: theme.surfaceElevated
                Rectangle {
                    width: parent.width * Math.min(Math.max(QbzPlayer.npCacheProgress, 0), 1)
                    height: parent.height
                    radius: 2
                    color: theme.borderMuted
                }
                Rectangle {
                    width: parent.width * Math.min(Math.max(QbzPlayer.npProgress, 0), 1)
                    height: parent.height
                    radius: 2
                    color: theme.accent
                }
                Rectangle {
                    visible: QbzPlayer.npHasTrack
                    width: 12
                    height: 12
                    radius: 6
                    color: theme.textPrimary
                    x: parent.width * Math.min(Math.max(QbzPlayer.npProgress, 0), 1) - width / 2
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
            Text {
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                visible: QbzPlayer.npHasTrack
                text: "-" + root.fmt(Math.max(0, QbzPlayer.npDurationSecs - QbzPlayer.npElapsedSecs))
                color: theme.textMuted
                font.pixelSize: 11
            }
            MouseArea {
                anchors.left: seekTrack.left
                anchors.right: seekTrack.right
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                cursorShape: QbzPlayer.npHasTrack ? Qt.PointingHandCursor : Qt.ArrowCursor
                onPressed: if (QbzPlayer.npHasTrack) QbzPlayer.seek(Math.min(Math.max(mouseX / width, 0), 1))
                onPositionChanged: if (pressed && QbzPlayer.npHasTrack) QbzPlayer.seek(Math.min(Math.max(mouseX / width, 0), 1))
            }
        }

        Item { width: 1; height: 6 }

        // --- Controls: the responsive symmetric zones -----------------------
        Item {
            width: parent.width
            height: parent.height - 38

            // LEFT column.
            Item {
                anchors.left: parent.left
                anchors.leftMargin: 6
                width: (parent.width - 12) * root.colSide
                height: parent.height
                clip: true

                // New (0) AND Large (3): the song card (Large drops the cover
                // — it lives in the dock — and shifts right to clear it).
                SongCard {
                    visible: !root.isClassic
                    x: root.largeActive ? root.dockWidth + 8 : 0
                    width: parent.width - x
                    anchors.verticalCenter: parent.verticalCenter
                    showArt: !root.largeActive
                    showBadges: !root.largeActive
                }
                // Classic: transport cluster hugging the left edge (plain
                // play glyph, Tauri arrangement).
                TransportControls {
                    visible: root.isClassic
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    playCircle: false
                }
            }

            // CENTRE column.
            Item {
                x: 6 + (parent.width - 12) * root.colSide
                width: (parent.width - 12) * root.colCentre
                height: parent.height
                clip: true

                // New (0) AND Large (3): centred transport, PLAY on the
                // window centre.
                TransportControls {
                    visible: !root.isClassic
                    anchors.centerIn: parent
                    playCircle: true
                }
                // Classic: the contained glass song card (<=560px cap).
                SongCard {
                    visible: root.isClassic
                    glass: true
                    width: Math.min(parent.width, 560)
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            // RIGHT column — secondary actions + volume, clustered at the
            // right wall.
            Item {
                anchors.right: parent.right
                anchors.rightMargin: 6
                width: (parent.width - 12) * root.colSide
                height: parent.height
                clip: true

                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 1

                    // Large: the SMALL AudioStamp just LEFT of the icon
                    // cluster at a fixed 12px gap (the badges move out of the
                    // song card in Large).
                    AudioStamp {
                        visible: root.largeActive && QbzPlayer.npHasTrack
                        showLeds: false
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Item { visible: root.largeActive; width: 12; height: 1 }

                    // Qobuz Connect — inert (device flyout out of scope).
                    QbzIconButton { name: "element-connect" }

                    // Now-Playing view — the mode flyout (phase 18).
                    QbzIconButton {
                        name: "layout-grid"
                        active: viewMenu.opened
                        onClicked: viewMenu.openBelowRight(viewBtn)
                        id: viewBtn
                    }

                    // Lyrics.
                    QbzIconButton {
                        name: "mic-vocal"
                        active: QbzShell.lyricsOpen
                        onClicked: QbzShell.toggleLyrics()
                    }

                    Item { width: 6; height: 1 }

                    // Volume.
                    QbzIconButton {
                        name: QbzPlayer.npMuted ? "volume-x" : "volume-2"
                        active: QbzPlayer.npMuted
                        onClicked: QbzPlayer.toggleMute()
                    }
                    Item {
                        width: 81
                        height: 32
                        Rectangle {
                            id: volTrack
                            width: parent.width
                            height: 4
                            radius: 2
                            anchors.verticalCenter: parent.verticalCenter
                            color: theme.surfaceElevated
                            Rectangle {
                                width: parent.width * QbzPlayer.npVolume
                                height: parent.height
                                radius: 2
                                color: theme.accent
                            }
                            Rectangle {
                                width: 16
                                height: 16
                                radius: 8
                                color: theme.textPrimary
                                x: parent.width * QbzPlayer.npVolume - width / 2
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onPressed: QbzPlayer.setVolume(Math.min(Math.max(mouseX / width, 0), 1))
                            onPositionChanged: if (pressed)
                                QbzPlayer.setVolume(Math.min(Math.max(mouseX / width, 0), 1))
                        }
                    }

                    Item { width: 6; height: 1 }

                    // Queue panel toggle.
                    QbzIconButton {
                        name: "list-ordered"
                        active: QbzShell.queueOpen
                        onClicked: QbzShell.toggleQueue()
                    }
                }
            }
        }

        Item { width: 1; height: 6 }
    }

    // The Now-Playing-view mode menu (shared with the Small bar).
    ViewModeMenu { id: viewMenu }
}
