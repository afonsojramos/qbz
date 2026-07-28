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
// BUTTON SET (phase 24 — 1:1 with the Tauri NowPlayingBar.svelte, via the
// Slint TransportControls/PlayerBar that descend from it):
//   transport: info · shuffle · prev · play · next · repeat · "+" (Add to…)
//              (+ the inline favorite heart in Classic, like Tauri)
//   right:     Cast · Connect · Lyrics · Now-Playing view · Audio settings
//              (normalization) · [6px] · Mute · slider · −/+ steppers ·
//              [6px] · Queue
// Tauri's Miniplayer + Full screen buttons live inside the Now-Playing view
// flyout (ViewModeMenu) since 2.0 — that ONE button holds both.
//
// Wired: transport (shuffle/prev/play/next/repeat), the add flyout
// (library / queue / play next / play later / album), seek, volume + mute +
// steppers, lyrics, queue, the Now-Playing-view flyout (npbSetMode),
// normalization, open album/artist via the SongCard meta links.
// Inert (TODO comments at the call sites): Cast, Connect (device flyouts),
// track-info, add-to-playlist, add-to-mixtape. The volume LOCK (ALSA hw /
// remote) is still not enforced.

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
    // AppearanceState.show-volume-steppers (PlayerBar.slint gates the −/+
    // pair on it; Tauri always showed them).
    readonly property bool showVolumeSteppers: settingsDoc.showVolumeSteppers === true

    // Favorite state of the now-playing track (Slint:
    // QueueState.now-playing-favorite). The queue document carries it on its
    // `current` row; this re-parses ONLY when that document changes.
    readonly property bool npFavorite: {
        try {
            var d = JSON.parse(QbzQueue.queueJson)
            return !!(d && d.current && d.current.isFavorite)
        } catch (e) {
            return false
        }
    }

    // Audio settings / normalization (Tauri's normalization button; Slint's
    // "Audio settings" flyout). `normalization` IS settable through
    // QbzBridge.settingsBool but is NOT published in settingsJson yet, so the
    // toggle keeps its own value once the user touches it and falls back to
    // the (currently absent) published field before that.
    // TODO(glue): publish `normalization` in SettingsDoc — see the report.
    property bool normTouched: false
    property bool normLocal: false
    readonly property bool normalizationOn: normTouched ? normLocal
        : (settingsDoc.normalization === true)
    function toggleNormalization() {
        normLocal = !normalizationOn
        normTouched = true
        QbzBridge.settingsBool("normalization", normLocal)
    }

    // np_quality_label is "24-bit / 96 kHz" (playback_qt::quality_badge);
    // QualityBadge wants the raw numbers, so split the published string
    // instead of duplicating the tier logic.
    readonly property int npBitDepth: {
        var l = QbzPlayer.npQualityLabel
        var i = l.indexOf("-bit")
        return i > 0 ? parseInt(l.substring(0, i)) : 0
    }
    readonly property real npSampleRate: {
        var l = QbzPlayer.npQualityLabel
        var i = l.indexOf("/")
        var j = l.indexOf("kHz")
        return (i >= 0 && j > i) ? parseFloat(l.substring(i + 1, j)) : 0
    }

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

        // The badge itself is the Tauri QualityBadge in its narrow-bar
        // "compact" mode (icon + "24/96"), NOT a hand-drawn glyph+label pair.
        QualityBadge {
            visible: QbzPlayer.npQualityLabel !== ""
            mode: "compact"
            // The stamp shows the cd glyph for ANY non-hires tier with a
            // label (not just "cd") — map that exactly.
            tierOverride: QbzPlayer.npQualityTier === "hires" ? "hires"
                : (QbzPlayer.npQualityLabel !== "" ? "cd" : "")
            bitDepth: root.npBitDepth
            samplingRate: root.npSampleRate
            anchors.verticalCenter: parent.verticalCenter
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

    // Favorite toggle — the inline heart Classic mounts next to "+" (Tauri's
    // `.control-btn` heart with fill=currentColor when favorited). NOT a
    // QbzIconButton: that control only tints accent/primary/secondary/muted
    // and the favorited heart must take the theme's `favorite` red (Slint
    // TransportControls uses Theme.favorite the same way).
    component FavToggle: Rectangle {
        width: 32
        height: 32
        radius: theme.radiusSm
        opacity: QbzPlayer.npHasTrack ? 1.0 : 0.3
        color: (favArea.containsMouse && QbzPlayer.npHasTrack) ? theme.surfaceHover : "transparent"
        QbzIcon {
            name: root.npFavorite ? "heart-filled" : "heart"
            width: 16
            height: 16
            anchors.centerIn: parent
            tintName: root.npFavorite ? "favorite"
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

    // Transport cluster (TransportControls.slint): track-info · shuffle ·
    // prev · play · next · repeat · "+" (Add to… flyout), plus the inline
    // favorite heart in Classic. 2px spacing on 32px buttons = the same 34px
    // pitch as Tauri's 30px `.control-btn` + 4px gap.
    // play-circle = the filled accent disc (New/Large); Classic/Small get the
    // plain 34px glyph, 20px in BOTH modes exactly like Tauri's size={20}.
    component TransportControls: Row {
        id: tc
        property bool playCircle: true
        // Classic ADDS the inline favorite toggle (Tauri parity).
        property bool classicActions: false
        /// Emitted by "+" with the button as anchor, so the flyout is owned
        /// by the bar (one menu, both mount points).
        signal addRequested(var anchorItem)
        spacing: 2
        height: 44

        // Track info — the first control, left of shuffle (1:1 with the
        // other views; it also restores PLAY's symmetric centring).
        QbzIconButton {
            name: "info"
            btnEnabled: QbzPlayer.npHasTrack
            anchors.verticalCenter: parent.verticalCenter
            // TODO(qt-bridge): no track-info panel in the Qt port yet
            // (Slint: media-action("track", id, "track-info")). Inert.
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
                tintName: tc.playCircle ? "black"
                    : (playArea.containsMouse ? "accent" : "primary")
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
            anchors.verticalCenter: parent.verticalCenter
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
                // play glyph + inline favorite, the Tauri arrangement).
                TransportControls {
                    visible: root.isClassic
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    playCircle: false
                    classicActions: true
                    onAddRequested: function (anchorItem) { addMenu.openBelowRight(anchorItem) }
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
                    onAddRequested: function (anchorItem) { addMenu.openBelowRight(anchorItem) }
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

                    // Cast (Chromecast / DLNA) — Tauri's first right-cluster
                    // button.
                    QbzIconButton {
                        name: "cast"
                        anchors.verticalCenter: parent.verticalCenter
                        // TODO(qt-bridge): no cast picker in the Qt port
                        // (Slint: CastState.picker-open + CastActions.open()).
                        // Rendered 1:1, inert.
                    }

                    // Qobuz Connect — inert (device flyout out of scope).
                    // monitor-speaker is the icon both Slint bars use.
                    QbzIconButton {
                        name: "monitor-speaker"
                        anchors.verticalCenter: parent.verticalCenter
                        // TODO(qt-bridge): no qconnect state/toggle exposed
                        // (Slint: NowPlayingState.qconnect-connected +
                        // qconnect-toggle()). Rendered 1:1, inert.
                    }

                    // Lyrics.
                    QbzIconButton {
                        name: "mic-vocal"
                        active: QbzShell.lyricsOpen
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzShell.toggleLyrics()
                    }

                    // Now-Playing view — the mode flyout (phase 18). Tauri's
                    // Miniplayer + Full screen buttons live inside it.
                    QbzIconButton {
                        id: viewBtn
                        name: "layout-grid"
                        active: viewMenu.opened
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: viewMenu.openBelowRight(viewBtn)
                    }

                    // Audio settings — Tauri's normalization toggle (2.0
                    // groups normalization + gapless behind this button).
                    // TODO(qt-bridge): the Slint two-toggle flyout is not
                    // ported; the click toggles normalization directly, which
                    // is exactly what the Tauri button did.
                    QbzIconButton {
                        name: "settings-2"
                        active: root.normalizationOn
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: root.toggleNormalization()
                    }

                    Item { width: 6; height: 1 }

                    // Volume: mute · slider · −/+ steppers (the steppers are
                    // the Tauri volume-step buttons, gated by the appearance
                    // preference like the Slint bar).
                    QbzIconButton {
                        name: QbzPlayer.npMuted ? "volume-x" : "volume-2"
                        active: QbzPlayer.npMuted
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzPlayer.toggleMute()
                    }
                    QbzSlider {
                        width: 81
                        anchors.verticalCenter: parent.verticalCenter
                        minimum: 0
                        // 0..1000: whole steps on a 0..100 scale quantize the
                        // volume to 1%; 0.1% steps drag fluidly.
                        maximum: 1000
                        value: Math.round(QbzPlayer.npVolume * 1000)
                        onChanged: function (v) { QbzPlayer.setVolume(v / 1000.0) }
                    }
                    QbzIconButton {
                        visible: root.showVolumeSteppers
                        name: "minus"
                        iconSize: 15
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzPlayer.setVolume(Math.max(0.0, QbzPlayer.npVolume - 0.05))
                    }
                    QbzIconButton {
                        visible: root.showVolumeSteppers
                        name: "plus"
                        iconSize: 15
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzPlayer.setVolume(Math.min(1.0, QbzPlayer.npVolume + 0.05))
                    }

                    Item { width: 6; height: 1 }

                    // Queue panel toggle.
                    QbzIconButton {
                        name: "list-ordered"
                        active: QbzShell.queueOpen
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzShell.toggleQueue()
                    }
                }
            }
        }

        Item { width: 1; height: 6 }
    }

    // The Now-Playing-view mode menu (shared with the Small bar).
    ViewModeMenu { id: viewMenu }

    // "Add to…" flyout behind the transport "+" (TransportControls.slint's
    // add-menu), on the shared CardMenu surface. Same seven entries, same
    // order, same icons.
    CardMenu {
        id: addMenu
        menuWidth: 232
        entries: {
            var m = [
                {
                    "label": root.npFavorite
                        ? QbzSession.tr("Remove from library", QbzSession.trRev)
                        : QbzSession.tr("Add to library", QbzSession.trRev),
                    "icon": root.npFavorite ? "heart-filled" : "heart",
                    "action": "favorite"
                },
                { "label": QbzSession.tr("Add to playlist", QbzSession.trRev), "icon": "list-music", "action": "playlist" },
                { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
                { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
                { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
                { "label": QbzSession.tr("Add to mixtape", QbzSession.trRev), "icon": "cassette-tape", "action": "mixtape" },
            ]
            if (QbzPlayer.npAlbumId !== "") {
                m.push({
                    "label": QbzSession.tr("Add album to collection", QbzSession.trRev),
                    "icon": "disc-3",
                    "action": "album-favorite"
                })
            }
            return m
        }
        onPicked: function (a) {
            var id = QbzPlayer.npTrackId
            if (a === "favorite") {
                if (id !== "") QbzQueue.queueToggleFavorite("track", id)
            } else if (a === "queue" || a === "later" || a === "next") {
                if (id !== "") QbzPlayer.enqueueTrack(id, a)
            } else if (a === "album-favorite") {
                if (QbzPlayer.npAlbumId !== "")
                    QbzBridge.libraryToggleFavorite("album", QbzPlayer.npAlbumId)
            }
            // TODO(qt-bridge): "playlist" (add-to-playlist modal) and
            // "mixtape" have no invokable in the Qt port yet — the rows are
            // rendered 1:1 with the Slint flyout and do nothing for now.
        }
    }
}
