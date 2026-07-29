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
//                RIGHT inline AudioStamp + cluster.
//
// QUALITY STACK (which bar mounts which component — SongCard.slint /
// PlayerBar.slint / PlayerBarSmall.slint):
//   New (0), Classic (1) -> shell/SongCardStamp.qml, mounted INSIDE the song
//                           card: QualityBadgeFull (icon + stacked
//                           tier/detail) over the dot LEDs.
//   Large (3), Small (2) -> shell/AudioStamp.qml, the inline 2-row stamp.
// They are DIFFERENT components in Slint and they stay different here.
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
// normalization, open album/artist via the SongCard meta links, and Track
// Info — the (i) button and the song-card title open shell/TrackInfoModal.qml
// (needs the QbzAlbum.openTrackInfo glue; see that file's header). Cast opens
// shell/CastPicker.qml (QbzCast — discovery, connect/disconnect, the
// per-device quality cap) and lights while a renderer is connected.
// Inert (TODO comments at the call sites): Connect (the QConnect device
// flyout), add-to-playlist, add-to-mixtape. The volume LOCK (ALSA hw /
// remote) is still not enforced.
//
// SIZE (project rule): the inline SongCard / TransportControls / FavToggle
// components moved out to shell/SongCard.qml, shell/TransportControls.qml and
// shell/FavToggle.qml in phase 25 — TransportControls is now shared with the
// Small bar instead of being duplicated there.

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
    // columns EQUAL so the centre column stays dead centre. The breakpoints
    // are the SHARED theme token (QbzTheme.npbSideFrac) — the Small bar reads
    // the same one, so the two bars cannot drift apart.
    // `root.width` IS the window width: the bar is anchored left-to-right on
    // the shell root, exactly like PlayerBar.slint reads its own root.
    property real sideFrac: theme.npbSideFrac(root.width)
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
    // (The old backendPipewire / dacPassthrough LED sources are gone: the
    // stamp's LEDs now read np_output_backend_* / np_output_mode_* off
    // QbzPlayer, which is where Slint reads them too — the settings document
    // never knew whether the backend was actually ENGAGED.)
    // AppearanceState.show-volume-steppers (PlayerBar.slint gates the −/+
    // pair on it; Tauri always showed them).
    readonly property bool showVolumeSteppers: settingsDoc.showVolumeSteppers === true

    // Favorite state of the now-playing track (Slint:
    // QueueState.now-playing-favorite). The queue document carries it on its
    // `current` row; this re-parses ONLY when that document changes.
    //
    // The bar's OWN heart settles fine on its own — `queue_qt::toggle_favorite`
    // republishes the queue document straight after the write. What it could
    // not see was a heart flipped ANYWHERE ELSE for the same track (the album
    // page's row, a search result, a card): nothing republishes the queue for
    // those, so the bar sat on a stale glyph for the rest of the track. The
    // one-slot override below closes it, keyed on the track id so it expires
    // by itself the moment playback moves on — no map to keep clean.
    property string favOverrideId: ""
    property bool favOverrideValue: false
    readonly property bool npFavorite: {
        if (root.favOverrideId !== "" && root.favOverrideId === QbzPlayer.npTrackId)
            return root.favOverrideValue
        try {
            var d = JSON.parse(QbzQueue.queueJson)
            return !!(d && d.current && d.current.isFavorite)
        } catch (e) {
            return false
        }
    }
    Connections {
        target: QbzLibrary
        function onLibraryFavoriteChanged(key, value) {
            var id = QbzPlayer.npTrackId
            if (id !== "" && key === "track:" + id) {
                root.favOverrideId = id
                root.favOverrideValue = value
            }
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

    function fmt(secs) {
        var m = Math.floor(secs / 60)
        var s = Math.floor(secs % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    // --- Track Info (album/TrackInfoModal.slint) ---------------------------
    // The (i) button and the song-card title open the MODAL (scrim + centered
    // card), which is what the .slint does: the bars fire
    // media-action("track", id, "track-info") and AppShell mounts
    // TrackInfoModal gated on TrackInfoState.open. Slint gates the trigger on
    // NowPlayingState.source == "qobuz" (no DB row / metadata page for local /
    // Plex / ephemeral); the Qt port publishes no np_source, so a numeric
    // track id stands in for it. TODO(glue): publish np_source.
    function openTrackInfo() {
        if (!QbzPlayer.npHasTrack)
            return
        var id = QbzPlayer.npTrackId
        if (!/^[0-9]+$/.test(id))
            return
        trackInfo.openFor(id)
    }

    TrackInfoModal { id: trackInfo }

    // --- Cast picker (shell/CastPicker.slint) ------------------------------
    // Mounted here for the same reason TrackInfoModal is: it is a Popup
    // parented to Overlay.overlay, so AppShell.qml needs no mount, and the
    // NowPlayingBar Loader guarantees exactly ONE bar (hence one instance) is
    // alive. Visibility follows QbzCast.pickerOpen — the cast button in the
    // right cluster raises it through QbzCast.openPicker(), which is also
    // what arms device discovery.
    CastPicker { }

    // --- Shared bits -------------------------------------------------------
    // SongCard / TransportControls / FavToggle were inline components here
    // until phase 25; they now live in their own files (shell/SongCard.qml,
    // shell/TransportControls.qml, shell/FavToggle.qml) and TransportControls
    // is shared with the Small bar.

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
                // Buffered / cache line — SeekBar.slint:58 paints it
                // text-muted @0.35, NOT border-muted: border-muted is an
                // alpha-over-surface token (white-based on dark themes,
                // black-based on light), so it inverted against the
                // surface-elevated rail on light themes. The Small bar
                // already used the right pair.
                Rectangle {
                    width: parent.width * Math.min(Math.max(QbzPlayer.npCacheProgress, 0), 1)
                    height: parent.height
                    radius: 2
                    color: theme.textMuted
                    opacity: 0.35
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
                    onTrackInfoRequested: root.openTrackInfo()
                }
                // Classic: transport cluster hugging the left edge (plain
                // play glyph + inline favorite, the Tauri arrangement).
                TransportControls {
                    visible: root.isClassic
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    playCircle: false
                    classicActions: true
                    favorite: root.npFavorite
                    onAddRequested: function (anchorItem) { addMenu.openBelowRight(anchorItem) }
                    onTrackInfoRequested: root.openTrackInfo()
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
                    favorite: root.npFavorite
                    onAddRequested: function (anchorItem) { addMenu.openBelowRight(anchorItem) }
                    onTrackInfoRequested: root.openTrackInfo()
                }
                // Classic: the contained glass song card (<=560px cap).
                SongCard {
                    visible: root.isClassic
                    glass: true
                    width: Math.min(parent.width, 560)
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.verticalCenter: parent.verticalCenter
                    onTrackInfoRequested: root.openTrackInfo()
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

                    // Large: the inline 2-row AudioStamp just LEFT of the icon
                    // cluster at a fixed 12px gap (the badges move OUT of the
                    // song card in Large). PlayerBar.slint mounts it with
                    // max-width: 140px.
                    AudioStamp {
                        visible: root.largeActive && QbzPlayer.npHasTrack
                        maxWidth: 140
                        anchors.verticalCenter: parent.verticalCenter
                    }
                    Item { visible: root.largeActive; width: 12; height: 1 }

                    // Cast (Chromecast / DLNA) — Tauri's first right-cluster
                    // button. Opens the picker modal and, with it, discovery
                    // (PlayerBar.slint:646-664: picker-open = true +
                    // CastActions.open()); lit while a renderer is connected.
                    QbzIconButton {
                        name: "cast"
                        active: QbzPlayer.npCastActive
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzCast.openPicker()
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
                        btnEnabled: !QbzPlayer.npVolumeLocked
                        active: QbzPlayer.npMuted
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzPlayer.toggleMute()
                    }
                    QbzSlider {
                        enabled: !QbzPlayer.npVolumeLocked
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
                        btnEnabled: !QbzPlayer.npVolumeLocked
                        onClicked: QbzPlayer.setVolume(Math.max(0.0, QbzPlayer.npVolume - 0.05))
                    }
                    QbzIconButton {
                        visible: root.showVolumeSteppers
                        name: "plus"
                        iconSize: 15
                        anchors.verticalCenter: parent.verticalCenter
                        btnEnabled: !QbzPlayer.npVolumeLocked
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
                    QbzLibrary.libraryToggleFavorite("album", QbzPlayer.npAlbumId)
            }
            // TODO(qt-bridge): "playlist" (add-to-playlist modal) and
            // "mixtape" have no invokable in the Qt port yet — the rows are
            // rendered 1:1 with the Slint flyout and do nothing for now.
        }
    }
}
