// Bottom now-playing / transport bar — QML port of
// crates/qbz-ui/ui/shell/PlayerBarSmall.slint (Mode C "Small"). Mounted by
// the NowPlayingBar seam when npbMode == 2 (phase 18).
//
// 3px full-width top seekbar (3-color: background / cache / progress),
// then the symmetric 3-column controls row (~39px):
//   LEFT   — the SHARED shell/SongCard.qml in its text-center (Small) arm
//            (37px bordered cover + 13px title over the [layers] · artist ·
//            album meta row) + the fixed 40px time column
//   CENTER — transport cluster (info · shuffle · prev · play · next ·
//            repeat · +)
//   RIGHT  — AudioStamp · Connect · Cast · Settings · ViewMode · Lyrics ·
//            Volume · Queue
// Total height 42px (ShellState.npb-small-height; npb-small-extra is 0 on
// Linux).
//
// BUTTON SET (phase 24): identical to PlayerBar's, in the Small bar's own
// order — the transport is LITERALLY the shared cluster since phase 25
// (shell/TransportControls.qml: info · shuffle · prev · play · next · repeat ·
// "+"), and the right cluster keeps PlayerBarSmall's mount order (Connect ·
// Cast · Settings · ViewMode · Lyrics · Volume · Queue).
// The "+" opens the same "Add to…" flyout as the full bar; the quality stamp
// is AudioStamp.qml — the inline 2-row stamp (quality line over the backend /
// mode LEDs) that PlayerBarSmall.slint mounts, shared with the Large bar.
//
// Connect / Settings / ViewMode are still inert visual replicas where no Qt
// invokable exists (TODO comments at the call sites); Volume, Queue, Shuffle,
// Repeat, Mute, Lyrics and the add flyout are live. Cast opens
// shell/CastPicker.qml (QbzCast — discovery, connect/disconnect, the
// per-device quality cap) and lights while a renderer is connected. Track
// Info (the (i) button and the song-card title) opens
// shell/TrackInfoModal.qml — it needs the QbzAlbum.openTrackInfo glue, see
// that file's header.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root
    // surface-card @ 0.5 while the ambient background is active (phase 14,
    // PlayerBarSmall.slint).
    color: ambientOn ? theme.surfaceCardA50 : theme.surfaceCard
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
       

    QbzTheme { id: theme }

    // Responsive side fraction — IDENTICAL to PlayerBarSmall's mechanism:
    // LEFT and RIGHT columns share this fraction so the CENTER transport
    // column lands PLAY on the window centre. The breakpoints (39/30/25 at
    // 1366 / 1920) are the SHARED theme token, so this bar and PlayerBar can
    // never drift apart — see QbzTheme.npbSideFrac.
    // `root.width` IS the window width: the bar is anchored left-to-right on
    // the shell root, exactly like PlayerBarSmall.slint reads its own root.
    property real sideFrac: theme.npbSideFrac(root.width)
    property real colCentre: 1.0 - 2.0 * root.sideFrac
    // WIDE = inline horizontal volume slider; NARROW = button-only.
    // Same 1366 edge as the side fraction (PlayerBarSmall.slint:152).
    property bool volumeWide: root.width >= theme.npbBreakMid

    // Favorite state of the now-playing track (Slint:
    // QueueState.now-playing-favorite) — the queue document carries it on its
    // `current` row; re-parsed only when that document changes.
    readonly property bool npFavorite: {
        try {
            var d = JSON.parse(QbzQueue.queueJson)
            return !!(d && d.current && d.current.isFavorite)
        } catch (e) {
            return false
        }
    }

    function fmt(secs) {
        var m = Math.floor(secs / 60)
        var s = Math.floor(secs % 60)
        return m + ":" + (s < 10 ? "0" : "") + s
    }

    // --- Track Info (album/TrackInfoModal.slint) -------------------------
    // The (i) button and the song-card title open the MODAL (scrim + centered
    // card) — PlayerBarSmall.slint fires media-action("track", id,
    // "track-info") and AppShell mounts TrackInfoModal. Slint gates it on
    // !NowPlayingState.is-ephemeral (no metadata page for local / Plex /
    // ephemeral rows); the Qt port publishes no source flag, so a numeric
    // track id stands in for it. TODO(glue): publish np_source.
    readonly property bool qobuzTrack: QbzPlayer.npHasTrack
        && /^[0-9]+$/.test(QbzPlayer.npTrackId)
    function openTrackInfo() {
        if (!root.qobuzTrack)
            return
        trackInfo.openFor(QbzPlayer.npTrackId)
    }

    TrackInfoModal { id: trackInfo }

    // --- Cast picker (shell/CastPicker.slint) ----------------------------
    // Same mount reasoning as TrackInfoModal above: a Popup parented to
    // Overlay.overlay, so AppShell.qml needs no mount, and the NowPlayingBar
    // Loader guarantees exactly ONE bar (hence one instance) is alive.
    // Visibility follows QbzCast.pickerOpen; the right-cluster cast button
    // raises it through QbzCast.openPicker(), which also arms discovery.
    CastPicker { }

    // Square compact icon button (BarControls.IconButton): disabled = dim
    // 0.3 + non-clickable; active = accent tint.


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
                    width: parent.width * Math.min(Math.max(QbzPlayer.npCacheProgress, 0), 1)
                    height: parent.height
                    radius: 2
                    color: theme.textMuted
                    opacity: 0.35
                }
                // Playback progress line.
                Rectangle {
                    width: parent.width * Math.min(Math.max(QbzPlayer.npProgress, 0), 1)
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
                    x: parent.width * Math.min(Math.max(QbzPlayer.npProgress, 0), 1) - width / 2
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
                enabled: QbzPlayer.npHasTrack
                onClicked: QbzPlayer.seek(Math.min(Math.max(mouseX / width, 0), 1))
            }
        }

        // === B. CONTROLS ROW — symmetric 3 columns =======================
        Item {
            width: parent.width
            height: parent.height - 3

            // ---- LEFT column: SongCard + Sep + time column --------------
            Item {
                anchors.left: parent.left
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                width: (root.width - 6) * root.sideFrac
                clip: true

                Row {
                    anchors.fill: parent
                    spacing: 0

                    // Song card: the SHARED shell/SongCard.qml in its
                    // text-center arm — 37px cover (3px surface-card border,
                    // flush to the window's left edge) + the 13px title over
                    // the [layers] · artist · album meta row. 1:1 with
                    // PlayerBarSmall.slint:238-267, which mounts the very same
                    // SongCard with these exact knobs.
                    //
                    // This REPLACES a hand-rolled 2-line copy (art + title +
                    // artist Text) that silently dropped three things living
                    // only inside SongCard: the context glyph, the album link
                    // and the floating cover preview (SongCard's artHover is
                    // the ONLY writer of QbzShell.artPreviewShow, so the
                    // shell's ArtPreviewOverlay was simply never armed in
                    // Small). Rule 5 — reuse, don't fork.
                    //
                    // Width = the column minus the Sep + time column, so the
                    // title and the meta budget elide at exactly the free
                    // space. Height = the 39px controls row: BOTH must be
                    // explicit, because SongCard's own defaults are 200x74 and
                    // a 74px card would push the 37px cover off-centre.
                    SongCard {
                        height: parent.height
                        width: parent.width - (QbzPlayer.npHasTrack ? 59 : 0)
                        artSize: 37
                        artBorderWidth: 3
                        artBorderColor: theme.surfaceCard
                        artTextGap: 9
                        titleFontSize: 13
                        showBadges: false
                        textCenter: true
                        onTrackInfoRequested: root.openTrackInfo()
                    }

                    // Separator: song-card | time column (8px toward the
                    // card, 10px toward the timer).
                    Sep {
                        visible: QbzPlayer.npHasTrack
                        gapLeft: 8
                        gapRight: 10
                    }

                    // Time column — fixed 40px 2-row block (elapsed /
                    // total), monospace for stable digit positions.
                    Column {
                        visible: QbzPlayer.npHasTrack
                        width: 40
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1
                        Text {
                            text: root.fmt(QbzPlayer.npElapsedSecs)
                            font.family: "monospace"
                            font.pixelSize: 10
                            color: theme.textMuted
                        }
                        Text {
                            text: root.fmt(QbzPlayer.npDurationSecs)
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

                // The SHARED cluster (shell/TransportControls.qml) — the
                // Small bar's own copy was a byte-for-byte twin of the full
                // bar's at playCircle:false / classicActions:false.
                TransportControls {
                    anchors.centerIn: parent
                    height: 34
                    playCircle: false
                    favorite: root.npFavorite
                    onAddRequested: function (anchorItem) { smallAddMenu.openBelowRight(anchorItem) }
                    onTrackInfoRequested: root.openTrackInfo()
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

                    // AudioStamp — the inline 2-row stamp (quality line over
                    // the backend/mode LEDs), FIRST in the right set.
                    // PlayerBarSmall.slint clamps it at 150px, widening to
                    // 280px when a QConnect renderer name is shown (it grows
                    // leftward into the empty space before the centre
                    // transport; the right zone's clip is the real guard).
                    AudioStamp {
                        visible: QbzPlayer.npHasTrack
                        anchors.verticalCenter: parent.verticalCenter
                        maxWidth: (QbzPlayer.npIsRemote && QbzPlayer.npCastTarget !== "")
                            ? 280 : 150
                    }

                    // Separator: AudioStamp | icon cluster (10px toward the
                    // stamp, 8px toward the buttons).
                    Sep {
                        visible: QbzPlayer.npHasTrack
                        gapLeft: 10
                        gapRight: 8
                    }

                    // Qobuz Connect.
                    // TODO(qt-bridge): no qconnect state/toggle exposed
                    // (Slint: NowPlayingState.qconnect-connected +
                    // qconnect-toggle()). Rendered 1:1, inert.
                    QbzIconButton { name: "monitor-speaker" }
                    // Cast (Chromecast / DLNA) — opens the picker modal and,
                    // with it, device discovery (PlayerBarSmall.slint:596-611:
                    // picker-open = true + CastActions.open()); lit while a
                    // renderer is connected.
                    QbzIconButton {
                        name: "cast"
                        active: QbzPlayer.npCastActive
                        onClicked: QbzCast.openPicker()
                    }
                    // Audio settings — Tauri's normalization toggle (2.0
                    // groups normalization + gapless behind this button).
                    // TODO(glue): `normalization` is settable through
                    // settingsBool but NOT published in settingsJson, so the
                    // state can only be tracked locally; TODO(qt-bridge): the
                    // Slint two-toggle flyout is not ported — the click
                    // toggles normalization, exactly like the Tauri button.
                    QbzIconButton {
                        id: smallNormBtn
                        property bool normOn: false
                        name: "settings-2"
                        active: normOn
                        onClicked: {
                            normOn = !normOn
                            QbzBridge.settingsBool("normalization", normOn)
                        }
                    }
                    QbzIconButton {
                        id: smallViewBtn
                        name: "layout-grid"
                        active: smallViewMenu.opened
                        onClicked: smallViewMenu.openBelowRight(smallViewBtn)
                    }
                    QbzIconButton {
                        name: "mic-vocal"
                        active: QbzShell.lyricsOpen
                        onClicked: QbzShell.toggleLyrics()
                    }

                    Item { width: 4; height: 1 }

                    // Volume — mute icon + (WIDE) inline horizontal slider.
                    QbzIconButton {
                        name: QbzPlayer.npMuted ? "volume-x" : "volume-2"
                        btnEnabled: !QbzPlayer.npVolumeLocked
                        active: QbzPlayer.npMuted
                        onClicked: QbzPlayer.toggleMute()
                    }
                    // WIDE: the shared QbzSlider (PlayerBarSmall mounts the
                    // same primitive at 72px on the 0..1000 scale).
                    QbzSlider {
                        enabled: !QbzPlayer.npVolumeLocked
                        visible: root.volumeWide
                        width: 72
                        anchors.verticalCenter: parent.verticalCenter
                        minimum: 0
                        maximum: 1000
                        value: Math.round(QbzPlayer.npVolume * 1000)
                        onChanged: function (v) { QbzPlayer.setVolume(v / 1000.0) }
                    }

                    Item { width: 4; height: 1 }

                    // Queue panel toggle.
                    QbzIconButton {
                        name: "list-ordered"
                        active: QbzShell.queueOpen
                        onClicked: QbzShell.toggleQueue()
                    }
                }
            }
        }
    }

    // The Now-Playing-view mode menu (phase 18 — shared with PlayerBar).
    ViewModeMenu { id: smallViewMenu }

    // "Add to…" flyout behind the transport "+" (TransportControls.slint's
    // add-menu) — same seven entries, order and icons as the full bar.
    CardMenu {
        id: smallAddMenu
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
