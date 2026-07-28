// Local album detail — QML port of album/LocalAlbumView.slint, a ROUTED
// PAGE (not a pane inside the Local Library view, which is how this port
// used to render it).
//
// Homologated 1:1 with the Qobuz AlbumView: whole-page Flickable, the same
// header proportions and action row, the toolbar (quality badge + track
// search), the column header, and source-aware track rows. The intentional
// differences vs the Qobuz page are the Slint's own: no label/awards
// sidebar, no Qobuz context menus, and the local-only version picker.
//
// Local actions ONLY: play all / shuffle / edit tags / add to playlist /
// add to Mixtape. A multi-artist album gets the "+N more artists" expander;
// a multi-disc album gets the disc dividers with their per-disc ⋯ menu.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"
import "local"

Rectangle {
    id: root

    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: QbzShell.ambientMode > 0 && QbzPlayer.npHasTrack
    radius: 12

    QbzTheme { id: theme }

    // ---------------------------- document -------------------------------
    function parseDoc(json, fallback) {
        if (json === "") return fallback
        try { return JSON.parse(json) } catch (e) { return fallback }
    }
    readonly property var doc: parseDoc(QbzLocal.localAlbumJson, null)
    readonly property var album: doc ? doc.album : null
    readonly property var tracks: doc ? doc.tracks : []
    readonly property var versions: album && album.versions ? album.versions : []
    readonly property var allArtists: {
        if (!album) return []
        var raw = (album.allArtists || "").split(",")
        var out = []
        for (var i = 0; i < raw.length; i++) {
            var n = raw[i].trim()
            if (n !== "" && out.indexOf(n) < 0) out.push(n)
        }
        return out
    }
    // The Slint's `info-line` — built in Rust there, derived here from the
    // fields the album row already carries.
    readonly property string infoLine: {
        if (!album) return ""
        var parts = []
        if ((album.year || "") !== "") parts.push(album.year)
        parts.push((album.trackCount || 0) + " "
                   + QbzSession.tr("tracks", QbzSession.trRev))
        if ((album.duration || "") !== "") parts.push(album.duration)
        if ((album.format || "") !== "") parts.push(album.format.toUpperCase())
        return parts.join("  •  ")
    }

    property string trackQuery: ""
    // Client-side track search: an album's track list is bounded, so the
    // Slint's LocalAlbumActions.search is a pure view filter here.
    readonly property var visibleTracks: {
        if (trackQuery === "") return tracks
        var q = trackQuery.toLowerCase()
        var out = []
        for (var i = 0; i < tracks.length; i++) {
            if ((tracks[i].title || "").toLowerCase().indexOf(q) >= 0) out.push(tracks[i])
        }
        return out
    }
    // Disc divider before the first row of each disc on a multi-disc album
    // (0 = flat list, as the Slint's disc-header-number).
    function discHeader(i) {
        var t = visibleTracks[i]
        if (!t) return 0
        var multi = false
        for (var j = 0; j < visibleTracks.length; j++) {
            if ((visibleTracks[j].disc || 1) > 1) { multi = true; break }
        }
        if (!multi) return 0
        if (i === 0) return t.disc || 1
        return (visibleTracks[i - 1].disc || 1) !== (t.disc || 1) ? (t.disc || 1) : 0
    }

    // Cover — the same id-keyed artwork channel every local surface uses.
    property var artMap: ({})
    Connections {
        target: QbzLocal
        function onLocalArtworkReady(key, path) {
            var m = root.artMap
            m[key] = path
            root.artMap = Object.assign({}, m)
        }
    }
    onAlbumChanged: {
        if (album && album.artKey) {
            QbzLocal.artworkWindow(JSON.stringify([album.artKey]))
        }
    }

    // ============================ page ===================================
    // Neutral header band (local albums have no artwork-derived tint yet).
    Rectangle {
        x: 0
        y: 0
        width: parent.width
        height: 340
        gradient: Gradient {
            GradientStop { position: 0.0; color: "#181820" }
            GradientStop { position: 0.16; color: "#181820" }
            GradientStop { position: 1.0; color: "#00181820" }
        }
    }

    Flickable {
        id: flick
        anchors.fill: parent
        clip: true
        contentWidth: width
        contentHeight: page.height + 100
        boundsBehavior: Flickable.StopAtBounds

        Column {
            id: page
            x: 32
            y: 11
            width: parent.width - 64
            spacing: 0

            // ---- History navigation ----
            Row {
                spacing: 6
                QbzNavButton {
                    name: "chevron-left"
                    btnEnabled: QbzShell.canBack
                    onClicked: QbzShell.navigateBack()
                }
                QbzNavButton {
                    name: "chevron-right"
                    btnEnabled: QbzShell.canForward
                    onClicked: QbzShell.navigateForward()
                }
            }
            Item { width: 1; height: 22 }

            // ---- Album header ----
            LocalAlbumHeader {
                width: parent.width
                album: root.album
                allArtists: root.allArtists
                infoLine: root.infoLine
                versions: root.versions
                coverSource: root.album ? (root.artMap[root.album.artKey] || "") : ""
                onOpenArtist: function (name) { root.openArtist(name) }
            }

            Item { width: 1; height: 20 }
            Rectangle { width: parent.width; height: 1; color: theme.borderSubtle }
            Item { width: 1; height: 8 }

            // ---- Loading ----
            Item {
                visible: QbzLocal.localAlbumLoading && root.tracks.length === 0
                width: parent.width
                height: visible ? 280 : 0
                QbzSpinner {
                    anchors.centerIn: parent
                    size: 36
                }
            }

            // ---- Toolbar: quality badge + track search ----
            Item {
                width: parent.width
                height: 52
                QualityBadgeFull {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    tier: root.album ? (root.album.qualityTier || "") : ""
                    detail: root.album ? (root.album.qualityDetail || "") : ""
                }
                Rectangle {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 280
                    height: 34
                    radius: 6
                    border.width: 1
                    border.color: theme.borderSubtle
                    color: theme.surfaceElevated
                    Row {
                        anchors.fill: parent
                        anchors.leftMargin: 10
                        anchors.rightMargin: 10
                        spacing: 7
                        QbzIcon {
                            name: "search"
                            width: 14
                            height: 14
                            anchors.verticalCenter: parent.verticalCenter
                            tintName: "muted"
                        }
                        Item {
                            width: parent.width - 21
                            height: parent.height
                            clip: true
                            TextInput {
                                id: searchInput
                                anchors.fill: parent
                                color: theme.textPrimary
                                font.pixelSize: 13
                                verticalAlignment: Text.AlignVCenter
                                selectByMouse: true
                                onTextEdited: root.trackQuery = text
                            }
                            Text {
                                visible: searchInput.text === ""
                                anchors.fill: parent
                                text: QbzSession.tr("Search tracks...", QbzSession.trRev)
                                color: theme.textMuted
                                font.pixelSize: 13
                                verticalAlignment: Text.AlignVCenter
                            }
                        }
                    }
                }
            }

            // ---- Column header ----
            Row {
                width: parent.width
                height: 40
                leftPadding: 12
                rightPadding: 12
                spacing: 16
                Text {
                    width: 32
                    height: parent.height
                    text: "#"
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    font.letterSpacing: 0.5
                    verticalAlignment: Text.AlignVCenter
                }
                Text {
                    width: parent.width - 24 - 32 - 80 - 80 - 28 - 32 - 5 * 16
                    height: parent.height
                    text: QbzSession.tr("Title", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    font.letterSpacing: 0.5
                    verticalAlignment: Text.AlignVCenter
                }
                Text {
                    width: 80
                    height: parent.height
                    text: QbzSession.tr("Duration", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    font.letterSpacing: 0.5
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                Text {
                    width: 80
                    height: parent.height
                    text: QbzSession.tr("Quality", QbzSession.trRev)
                    color: theme.textMuted
                    font.pixelSize: theme.fontLegal
                    font.letterSpacing: 0.5
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                // Source column + the per-row more-button slot.
                Item { width: 28; height: parent.height }
                Item { width: 32; height: parent.height }
            }

            // ---- Track list ----
            // An album's track count is bounded (the windowed views are for
            // the library-scale surfaces), so this mounts like the Slint.
            Column {
                width: parent.width
                spacing: 0
                Repeater {
                    model: root.visibleTracks
                    delegate: Column {
                        id: trackBlock
                        required property var modelData
                        required property int index
                        width: page.width
                        spacing: 0

                        // Disc divider + its per-disc ⋯ menu.
                        Item {
                            visible: root.discHeader(trackBlock.index) > 0
                            width: parent.width
                            height: visible ? 40 : 0
                            Text {
                                x: 12
                                anchors.verticalCenter: parent.verticalCenter
                                text: QbzSession.tr("Disc", QbzSession.trRev) + " "
                                    + root.discHeader(trackBlock.index)
                                color: theme.textMuted
                                font.pixelSize: theme.fontLegal
                                font.weight: theme.weightSemibold
                                font.letterSpacing: 0.5
                            }
                            Rectangle {
                                x: parent.width - 44
                                width: 32
                                height: 32
                                radius: theme.radiusSm
                                anchors.verticalCenter: parent.verticalCenter
                                color: discArea.containsMouse ? theme.surfaceElevated : "transparent"
                                QbzIcon {
                                    name: "ellipsis"
                                    width: 16
                                    height: 16
                                    anchors.centerIn: parent
                                    tintName: discArea.containsMouse ? "primary" : "muted"
                                }
                                MouseArea {
                                    id: discArea
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: function (mouse) {
                                        discMenu.openAtCursor(discArea, mouse.x, mouse.y)
                                    }
                                }
                                CardMenu {
                                    id: discMenu
                                    menuWidth: 200
                                    entries: [
                                        { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                                        { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
                                        { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
                                        { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
                                    ]
                                    onPicked: function (a) {
                                        QbzLocal.albumDiscAction(root.discHeader(trackBlock.index), a)
                                    }
                                }
                            }
                        }

                        LocalTrackRow {
                            width: page.width
                            item: trackBlock.modelData
                            number: trackBlock.modelData.number > 0
                                ? trackBlock.modelData.number : trackBlock.index + 1
                            showAlbum: false
                            showArtwork: false
                            onPlayRequested: QbzLocal.playAlbumTrack(
                                root.album.id, trackBlock.modelData.id)
                            onEnqueueRequested: function (m) {
                                QbzLocal.enqueue("track", trackBlock.modelData.id, m)
                            }
                        }
                    }
                }
            }
        }
    }

    QbzScrollBar {
        anchors.right: parent.right
        anchors.rightMargin: 4
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        target: flick
        visible: flick.contentHeight > flick.height
    }

    // Local/Plex artists have no catalog id, so "go to artist" is a NAME
    // route into the Local Library Artists tab (the Slint's source-aware
    // open-artist).
    function openArtist(name) {
        QbzLocal.openArtistByName(name)
        QbzShell.navigateTo("local")
    }
}
