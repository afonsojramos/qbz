// Bottom now-playing / transport bar — QML port of
// crates/qbz-ui/ui/shell/PlayerBarSmall.slint (Mode C "Small"). Mounted by
// the NowPlayingBar seam when npbMode == 2 (phase 18).
//
// 3px full-width top seekbar (3-color: background / cache / progress),
// then the symmetric 3-column controls row (~39px):
//   LEFT   — 37px album art + 2-line title/artist + fixed time column
//   CENTER — transport cluster (info · shuffle · prev · play · next ·
//            repeat · +)
//   RIGHT  — AudioStamp · Connect · Cast · Settings · ViewMode · Lyrics ·
//            Volume · Queue
// Total height 42px (ShellState.npb-small-height; npb-small-extra is 0 on
// Linux).
//
// POC-NOTE: playback is not wired (phase 4) — the bar renders its EMPTY
// state (NowPlayingState.has-track == false): idle music-glyph art,
// "Nothing playing" title, transport disabled (opacity 0.3), no time
// column, no AudioStamp. Connect / Cast / Settings / ViewMode / Lyrics are
// inert visual replicas (their flyouts are out of scope); Volume, Queue,
// Shuffle, Repeat, Mute are live against the POC NowPlayingModel.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    id: root
    // surface-card @ 0.5 while the ambient background is active (phase 14,
    // PlayerBarSmall.slint).
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzBridge.ambientMode > 0 && QbzBridge.npHasTrack
       

    QbzTheme { id: theme }

    // Responsive side fraction — IDENTICAL to PlayerBarSmall's mechanism:
    // LEFT and RIGHT columns share this fraction so the CENTER transport
    // column lands PLAY on the window centre.
    //   width <  1366  -> 39% | 22% | 39%
    //   1366..<1920    -> 30% | 40% | 30%
    //   width >= 1920  -> 25% | 50% | 25%
    property real sideFrac: root.width >= 1920 ? 0.25 : (root.width >= 1366 ? 0.30 : 0.39)
    property real colCentre: 1.0 - 2.0 * root.sideFrac
    // WIDE = inline horizontal volume slider; NARROW = button-only.
    property bool volumeWide: root.width >= 1366

    function fmt(secs) {
        var m = Math.floor(secs / 60)
        var s = Math.floor(secs % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    // Square compact icon button (BarControls.IconButton): disabled = dim
    // 0.3 + non-clickable; active = accent tint.
    component BarIconBtn: Rectangle {
        property string name: ""
        property int iconSize: 16
        property bool active: false
        property bool btnEnabled: true
        signal clicked()

        width: 32
        height: 32
        radius: theme.radiusSm
        opacity: btnEnabled ? 1.0 : 0.3
        color: (biArea.containsMouse && btnEnabled) ? theme.surfaceHover : "transparent"

        QbzIcon {
            name: parent.name
            width: parent.iconSize
            height: parent.iconSize
            anchors.centerIn: parent
            tintName: parent.active ? "accent"
                : (biArea.containsMouse && parent.btnEnabled)
                    ? "primary" : "secondary"
        }
        MouseArea {
            id: biArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: parent.btnEnabled ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: if (parent.btnEnabled) parent.clicked()
        }
    }

    // Thin vertical separator (PlayerBarSmall's Sep): a 1px line,
    // border-subtle, ~60% of the row height, vertically centred; the
    // horizontal gaps are the caller's.
    component Sep: Item {
        property int gapLeft: 6
        property int gapRight: 6
        width: gapLeft + 1 + gapRight
        height: parent ? parent.height : 0
        Rectangle {
            x: gapLeft
            width: 1
            height: 24
            anchors.verticalCenter: parent.verticalCenter
            color: theme.borderSubtle
        }
    }

    Column {
        anchors.fill: parent
        spacing: 0

        // === A. TOP full-width seekbar (the content/bar divider) ========
        Item {
            width: parent.width
            height: 3

            Rectangle {
                id: seekTrack
                width: parent.width
                height: 3
                radius: 2
                color: theme.surfaceElevated

                // Buffered / cache line.
                Rectangle {
                    width: parent.width * Math.min(Math.max(QbzBridge.npCacheProgress, 0), 1)
                    height: parent.height
                    radius: 2
                    color: theme.textMuted
                    opacity: 0.35
                }
                // Playback progress line.
                Rectangle {
                    width: parent.width * Math.min(Math.max(QbzBridge.npProgress, 0), 1)
                    height: parent.height
                    radius: 2
                    color: theme.accent
                }
                // Hover thumb.
                Rectangle {
                    width: 12
                    height: 12
                    radius: 6
                    color: theme.textPrimary
                    x: parent.width * Math.min(Math.max(QbzBridge.npProgress, 0), 1) - width / 2
                    anchors.verticalCenter: parent.verticalCenter
                    opacity: seekArea.containsMouse ? 1.0 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 100 } }
                }
            }
            // Tall, easy-to-grab hit area over the thin line.
            MouseArea {
                id: seekArea
                width: parent.width
                height: 18
                anchors.verticalCenter: seekTrack.verticalCenter
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                enabled: QbzBridge.npHasTrack
                onClicked: QbzBridge.seek(Math.min(Math.max(mouseX / width, 0), 1))
            }
        }

        // === B. CONTROLS ROW — symmetric 3 columns =======================
        Item {
            width: parent.width
            height: parent.height - 3

            // ---- LEFT column: art + title/artist + time column ---------
            Item {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: (root.width - 6) * root.sideFrac
                clip: true

                Row {
                    anchors.fill: parent
                    spacing: 0

                    // Song card: 37px art (3px surface-card border, flush to
                    // the window's left edge) + tight 2-line text block.
                    // Width = the column minus the Sep + time column, so the
                    // title elides at exactly the free space.
                    Item {
                        height: parent.height
                        width: parent.width - (QbzBridge.npHasTrack ? 59 : 0)

                        Rectangle {
                            id: artBox
                            width: 37
                            height: 37
                            anchors.verticalCenter: parent.verticalCenter
                            radius: 6
                            border.width: 3
                            border.color: theme.surfaceCard
                            color: theme.surfaceElevated
                            clip: true

                            RoundedImage {
                                anchors.fill: parent
                                source: QbzBridge.npArtworkPath
                                radius: 6
                            }
                            // Idle (no track): a STATIC album glyph
                            // placeholder (50% of the art size, muted).
                            QbzIcon {
                                visible: !QbzBridge.npHasTrack
                                name: "music"
                                width: 18.5
                                height: 18.5
                                anchors.centerIn: parent
                                tintName: "muted"
                            }
                        }

                        // Title + artist — constrained to the art's inner
                        // height (37 − 2×3 = 31px) and vertically centred
                        // (SongCard text-center).
                        Column {
                            id: metaBlock
                            width: Math.max(0, parent.width - 37 - 9)
                            height: 31
                            anchors.left: artBox.right
                            anchors.leftMargin: 9
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 1

                            Text {
                                width: parent.width
                                text: QbzBridge.npHasTrack
                                    ? QbzBridge.npTitle : QbzBridge.tr("Nothing playing")
                                color: theme.textPrimary
                                font.pixelSize: 13
                                font.weight: theme.weightMedium
                                elide: Text.ElideRight
                            }
                            Text {
                                visible: QbzBridge.npHasTrack
                                width: parent.width
                                text: QbzBridge.npArtist
                                color: theme.textMuted
                                font.pixelSize: 11
                                elide: Text.ElideRight
                            }
                        }
                    }

                    // Separator: song-card | time column (8px toward the
                    // card, 10px toward the timer).
                    Sep {
                        visible: QbzBridge.npHasTrack
                        gapLeft: 8
                        gapRight: 10
                    }

                    // Time column — fixed 40px 2-row block (elapsed /
                    // total), monospace for stable digit positions.
                    Column {
                        visible: QbzBridge.npHasTrack
                        width: 40
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1
                        Text {
                            text: root.fmt(QbzBridge.npElapsedSecs)
                            font.family: "monospace"
                            font.pixelSize: 10
                            color: theme.textMuted
                        }
                        Text {
                            text: root.fmt(QbzBridge.npDurationSecs)
                            font.family: "monospace"
                            font.pixelSize: 10
                            color: theme.textMuted
                        }
                    }
                }
            }

            // ---- CENTER column: transport, window-centred --------------
            Item {
                anchors.left: parent.left
                anchors.leftMargin: (root.width - 6) * root.sideFrac
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: (root.width - 6) * root.colCentre
                clip: true

                Row {
                    anchors.centerIn: parent
                    spacing: 2

                    BarIconBtn {
                        name: "info"
                        btnEnabled: QbzBridge.npHasTrack
                        // POC-NOTE: track-info panel is out of scope.
                    }
                    BarIconBtn {
                        name: "shuffle"
                        active: QbzBridge.npShuffle
                        btnEnabled: QbzBridge.npHasTrack
                        onClicked: QbzBridge.toggleShuffle()
                    }
                    BarIconBtn {
                        name: "skip-back"
                        btnEnabled: QbzBridge.npHasTrack
                        onClicked: QbzBridge.previous()
                    }
                    // Play/pause — the Classic plain glyph (play-circle:
                    // false in Small): 34px, 20px glyph, text-primary,
                    // accent on hover.
                    Rectangle {
                        width: 34
                        height: 34
                        radius: theme.radiusSm
                        opacity: QbzBridge.npHasTrack ? 1.0 : 0.3
                        color: (playArea.containsMouse && QbzBridge.npHasTrack)
                            ? theme.surfaceHover : "transparent"
                        QbzIcon {
                            name: QbzBridge.npPlaying ? "pause" : "play-fill"
                            width: 20
                            height: 20
                            anchors.centerIn: parent
                            tintName: playArea.containsMouse ? "accent" : "primary"
                        }
                        MouseArea {
                            id: playArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: QbzBridge.npHasTrack ? Qt.PointingHandCursor : Qt.ArrowCursor
                            onClicked: if (QbzBridge.npHasTrack) QbzBridge.togglePlay()
                        }
                    }
                    BarIconBtn {
                        name: "skip-forward"
                        btnEnabled: QbzBridge.npHasTrack
                        onClicked: QbzBridge.next()
                    }
                    BarIconBtn {
                        name: QbzBridge.npRepeatMode === 2 ? "repeat-1" : "repeat"
                        active: QbzBridge.npRepeatMode !== 0
                        btnEnabled: QbzBridge.npHasTrack
                        onClicked: QbzBridge.cycleRepeat()
                    }
                    BarIconBtn {
                        name: "plus"
                        btnEnabled: QbzBridge.npHasTrack
                        // POC-NOTE: the grouped "Add to…" flyout is out of
                        // scope.
                    }
                }
            }

            // ---- RIGHT column: the control SET, right-aligned ----------
            // Order (PlayerBarSmall mount order): AudioStamp · Connect ·
            // Cast · Settings · ViewMode · Lyrics · Volume · Queue.
            Item {
                anchors.right: parent.right
                anchors.rightMargin: 6
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: (root.width - 6) * root.sideFrac
                clip: true

                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    height: parent.height
                    spacing: 1

                    // AudioStamp — tier + detail line, width-clamped.
                    // POC-NOTE: row 2 (backend/mode LEDs) is not rendered —
                    // the POC model carries no output-backend state.
                    Column {
                        visible: QbzBridge.npHasTrack
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1
                        Row {
                            spacing: 5
                            layoutDirection: Qt.RightToLeft
                            Text {
                                text: QbzBridge.npQualityLabel
                                font.pixelSize: 9
                                font.weight: theme.weightSemibold
                                color: theme.textPrimary
                                elide: Text.ElideRight
                            }
                            Text {
                                text: QbzBridge.npQualityTier === "hires" ? "HI-RES"
                                    : QbzBridge.npQualityTier === "mp3" ? "MP3"
                                    : QbzBridge.npQualityTier === "lossless" ? "LOSSLESS" : "CD"
                                font.pixelSize: 9
                                font.weight: theme.weightBold
                                font.letterSpacing: 0.3
                                color: theme.textSecondary
                            }
                        }
                    }

                    // Separator: AudioStamp | icon cluster (10px toward the
                    // stamp, 8px toward the buttons).
                    Sep {
                        visible: QbzBridge.npHasTrack
                        gapLeft: 10
                        gapRight: 8
                    }

                    // POC-NOTE: Connect / Cast / Settings / ViewMode /
                    // Lyrics are inert visual replicas — their flyouts
                    // (device pickers, audio toggles, NPB-mode menu) are
                    // out of scope for the POC.
                    BarIconBtn { name: "monitor-speaker" }
                    BarIconBtn { name: "cast" }
                    BarIconBtn { name: "settings-2" }
                    BarIconBtn {
                        id: smallViewBtn
                        name: "layout-grid"
                        active: smallViewMenu.opened
                        onClicked: smallViewMenu.openBelowRight(smallViewBtn)
                    }
                    BarIconBtn {
                        name: "mic-vocal"
                        active: QbzBridge.lyricsOpen
                        onClicked: QbzBridge.toggleLyrics()
                    }

                    Item { width: 4; height: 1 }

                    // Volume — mute icon + (WIDE) inline horizontal slider.
                    BarIconBtn {
                        name: QbzBridge.npMuted ? "volume-x" : "volume-2"
                        active: QbzBridge.npMuted
                        onClicked: QbzBridge.toggleMute()
                    }
                    Item {
                        visible: root.volumeWide
                        width: 72
                        height: 32
                        Rectangle {
                            id: volTrack
                            width: parent.width
                            height: 4
                            radius: 2
                            anchors.verticalCenter: parent.verticalCenter
                            color: theme.surfaceElevated
                            Rectangle {
                                width: parent.width * QbzBridge.npVolume
                                height: parent.height
                                radius: 2
                                color: theme.accent
                            }
                            Rectangle {
                                width: 16
                                height: 16
                                radius: 8
                                color: theme.textPrimary
                                x: parent.width * QbzBridge.npVolume - width / 2
                                anchors.verticalCenter: parent.verticalCenter
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onPressed: QbzBridge.setVolume(Math.min(Math.max(mouseX / width, 0), 1))
                            onPositionChanged: if (pressed)
                                QbzBridge.setVolume(Math.min(Math.max(mouseX / width, 0), 1))
                        }
                    }

                    Item { width: 4; height: 1 }

                    // Queue panel toggle.
                    BarIconBtn {
                        name: "list-ordered"
                        active: QbzBridge.queueOpen
                        onClicked: QbzBridge.toggleQueue()
                    }
                }
            }
        }
    }

    // The Now-Playing-view mode menu (phase 18 — shared with PlayerBar).
    ViewModeMenu { id: smallViewMenu }
}
