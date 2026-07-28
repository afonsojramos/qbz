// THE shared album card — QML replica of discover/AlbumCard.slint, used
// by BOTH the Home rails and the Library All grid (the Slint mounts the
// same component in both places).
//
// 200x246: 200px artwork (Radius.sm) + placeholder, hover scrim with
// genre/year meta, pin badge (top-right), favorite / play / more overlay
// buttons, award ribbon, source badge (opt-in), then the title/artist
// lines with the icon-only quality badge.
//
// Live wiring: play (art click + overlay play), favorite heart (optimistic
// + signal), pin badge (pinned store), ⋯ context menu (Play / Play next /
// Add to queue wired through playback_qt; favorite wired; Open album +
// Block inert — POC-NOTEs: no album page / blacklist store).

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    // --- card data (the ONE contract both hosts fill) --------------------
    property string albumId: ""
    property string title: ""
    property string artist: ""
    property string artistId: ""
    property string genre: ""
    property string year: ""
    property string qualityTier: ""
    property string ribbon: ""
    property string ribbonKind: ""
    // Artwork image source (file://… or "") — the host's cache lookup.
    property string artSource: ""
    property bool isFavorite: false
    property bool isPinned: false
    // Source badge (Library show-local): "local" | "plex" | "" (hidden).
    property string source: ""

    // --- Local Library mode (additive; default = the Qobuz behaviour) ----
    // The Local Library mounts THIS card, but its `albumId` is a group key
    // (folder or metadata identity), not a Qobuz catalog id — routing it
    // through QbzBridge.openAlbum / QbzPlayer.playAlbum would fire a
    // catalog fetch for a folder path. In localMode every action is emitted
    // to the host instead, and the catalog-only affordances (heart, pin,
    // "Block this album") are hidden. Nothing else about the card changes,
    // so the two surfaces stay pixel-identical.
    property bool localMode: false
    signal openRequested()
    signal playRequested()
    signal enqueueRequested(string mode)

    QbzTheme { id: theme }

    width: 200
    height: 246
    color: "transparent"

    readonly property bool overlayOn: artArea.containsMouse || pinArea.containsMouse
        || favBtn.hovered || playBtn.hovered || moreBtn.hovered

    function toggleFavorite() {
        root.isFavorite = !root.isFavorite
        QbzLibrary.libraryToggleFavorite("album", root.albumId)
    }
    function togglePin() {
        root.isPinned = !root.isPinned
        QbzLibrary.togglePin("album", root.albumId, root.title, root.artist, "")
    }

    Column {
        spacing: 0

        // --- Artwork + hover overlay -----------------------------------
        Rectangle {
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceElevated
            clip: true

            RoundedImage {
                anchors.fill: parent
                source: root.artSource
                radius: theme.radiusSm
            }

            // Hover scrim.
            Rectangle {
                anchors.fill: parent
                radius: theme.radiusSm
                color: "#000000"
                opacity: root.overlayOn ? 0.6 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
            }

            // Hover meta — genre + year, top-left.
            Column {
                x: 12
                y: 12
                spacing: 2
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                Text {
                    visible: root.genre !== ""
                    text: root.genre
                    height: 20
                    color: "#ebffffff"
                    font.pixelSize: 13
                    font.weight: theme.weightBold
                    verticalAlignment: Text.AlignVCenter
                }
                Text {
                    visible: root.year !== ""
                    text: root.year
                    height: 17
                    color: "#ccffffff"
                    font.pixelSize: 12
                    verticalAlignment: Text.AlignVCenter
                }
            }

            // Card-open + hover detector (declared before the action
            // buttons so those win the pointer).
            MouseArea {
                id: artArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                // Phase 8: the card opens the album view (the overlay play
                // button carries the play affordance).
                onClicked: root.localMode ? root.openRequested()
                                          : QbzBridge.openAlbum(root.albumId)
            }

            // Pin badge — top-right. Hover-revealed like the overlay
            // buttons (AlbumCard.slint: opacity follows overlay-on even
            // when pinned — the pinned state reads in the icon swap only:
            // filled accent pin vs outline). Always-mounted (opacity) so
            // its hover joins overlayOn.
            Rectangle {
                visible: !root.localMode
                x: parent.width - width - 8
                y: 8
                width: 26
                height: 26
                radius: 13
                color: pinArea.containsMouse ? "#cc000000" : "#99000000"
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                QbzIcon {
                    name: root.isPinned ? "pin-filled" : "pin"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    tintName: root.isPinned ? "accent" : "primary"
                }
                MouseArea {
                    id: pinArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.togglePin()
                }
            }

            // Hover action buttons — favorite / play / more (y=120, h=44,
            // centered, spacing 12).
            CardOverlayRow {
                y: 120
                width: parent.width
                shown: root.overlayOn

                CardOverlayButton {
                    id: favBtn
                    visible: !root.localMode
                    name: root.isFavorite ? "heart-filled" : "heart"
                    active: root.isFavorite
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.toggleFavorite()
                }
                CardOverlayButton {
                    id: playBtn
                    name: "play-fill"
                    primary: true
                    anchors.verticalCenter: parent.verticalCenter
                    onClicked: root.localMode ? root.playRequested()
                                              : QbzPlayer.playAlbum(root.albumId)
                }
                CardOverlayButton {
                    id: moreBtn
                    name: "ellipsis"
                    anchors.verticalCenter: parent.verticalCenter
                    // CardOverlayButton.clicked() carries no mouse payload,
                    // so fall back to the disc's centre — the menu still
                    // opens under the ⋯ (worst case 18px off the pointer).
                    // Stays correct if the signal ever forwards the event.
                    onClicked: function (mouse) {
                        albumMenu.openAtCursor(moreBtn,
                            mouse ? mouse.x : moreBtn.width / 2,
                            mouse ? mouse.y : moreBtn.height / 2)
                    }
                }
            }

            // Context menu (AlbumCard.slint's album-menu): 196px, items
            // 33px, icon 15 + label 13.
            QbzContextMenu {
                id: albumMenu
                menuWidth: 196
                    Repeater {
                        // localMode drops the two catalog-only rows (heart +
                        // blacklist); the four playback rows are identical.
                        model: {
                            var m = [
                                { "label": QbzSession.tr("Open album", QbzSession.trRev), "icon": "library-big", "action": "open" },
                                { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                                { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-plus", "action": "next" },
                                { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
                            ]
                            if (!root.localMode) {
                                m.push({ "label": root.isFavorite ? QbzSession.tr("Remove from Library", QbzSession.trRev) : QbzSession.tr("Add to Library", QbzSession.trRev), "icon": root.isFavorite ? "heart-filled" : "heart", "action": "favorite" })
                                m.push({ "label": QbzSession.tr("Block this album", QbzSession.trRev), "icon": "blind-eye", "action": "block" })
                            }
                            return m
                        }
                        delegate: Rectangle {
                            required property var modelData
                            width: parent ? parent.width : 0
                            height: 33
                            radius: 5
                            color: miArea.containsMouse ? theme.surfaceHover : "transparent"
                            Row {
                                anchors.fill: parent
                                anchors.leftMargin: 8
                                spacing: 8
                                QbzIcon {
                                    name: modelData.icon
                                    width: 15
                                    height: 15
                                    anchors.verticalCenter: parent.verticalCenter
                                    tintName: "secondary"
                                }
                                Text {
                                    height: parent.height
                                    width: parent.width - 23
                                    text: modelData.label
                                    color: theme.textSecondary
                                    font.pixelSize: 13
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                            }
                            MouseArea {
                                id: miArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    albumMenu.close()
                                    var a = modelData.action
                                    if (root.localMode) {
                                        if (a === "open") root.openRequested()
                                        else if (a === "play") root.playRequested()
                                        else if (a === "next") root.enqueueRequested("next")
                                        else if (a === "queue") root.enqueueRequested("later")
                                        return
                                    }
                                    if (a === "open") QbzBridge.openAlbum(root.albumId)
                                    else if (a === "play") QbzPlayer.playAlbum(root.albumId)
                                    else if (a === "next") QbzPlayer.enqueueAlbum(root.albumId, "next")
                                    else if (a === "queue") QbzPlayer.enqueueAlbum(root.albumId, "later")
                                    else if (a === "favorite") root.toggleFavorite()
                                    // "block" (no blacklist store): inert —
                                    // POC-NOTE.
                                }
                            }
                        }
                    }
                }

            // Award ribbon — content-width, capped at the card width.
            Rectangle {
                visible: root.ribbon !== ""
                x: 0
                y: parent.height - height - 8
                height: 20
                width: Math.min(ribbonRow.width, 200)
                color: root.ribbonKind === "press" ? "#d49511" : "#e0000000"
                topRightRadius: 3
                bottomRightRadius: 3
                clip: true
                Rectangle {
                    width: root.ribbonKind === "press" ? 0 : 3
                    height: parent.height
                    color: root.ribbonKind === "qobuzissime" ? "#8b5cf6" : "#eab308"
                }
                Row {
                    id: ribbonRow
                    height: parent.height
                    leftPadding: 10
                    rightPadding: 10
                    width: ribbonText.implicitWidth + 20
                    Text {
                        id: ribbonText
                        height: parent.height
                        text: root.ribbon
                        color: root.ribbonKind === "press" ? "#1f1407" : "#ffffff"
                        font.pixelSize: 9
                        font.weight: theme.weightSemibold
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
            }

            // Source badge (Library show-local): bottom-right of the art.
            Rectangle {
                visible: root.source === "local" || root.source === "plex"
                    || root.source === "offline"
                x: parent.width - width - 6
                y: parent.height - height - 6
                width: 24
                height: 24
                radius: 4
                color: "#b3000000"
                QbzIcon {
                    // The Local Library's third source: a Qobuz offline copy
                    // (LocalLibraryView.slint's `show-source-badge` triple).
                    name: root.source === "offline" ? "cloud-download" : "hard-drive"
                    width: 14
                    height: 14
                    anchors.centerIn: parent
                    tintName: root.source === "plex" ? "accent" : "primary"
                }
            }
        }
        Item { width: 1; height: 6 }

        // --- Title / artist + quality badge ------------------------------
        Row {
            width: 200
            height: 40
            spacing: theme.spacingSm
            Column {
                width: parent.width - (qBadge.visible ? qBadge.width + theme.spacingSm : 0)
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2
                Text {
                    width: parent.width
                    height: 20
                    text: root.title
                    color: titleArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: theme.fontBody - 2
                    font.weight: theme.weightMedium
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: titleArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.localMode ? root.openRequested()
                                                  : QbzBridge.openAlbum(root.albumId)
                    }
                }
                Text {
                    width: parent.width
                    height: 18
                    text: root.artist
                    color: root.artistId !== "" && artistArea.containsMouse
                        ? theme.textPrimary : theme.textMuted
                    font.pixelSize: theme.fontLink - 1
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                    MouseArea {
                        id: artistArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: root.artistId !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                        onClicked: if (root.artistId !== "") QbzBridge.openArtist(root.artistId)
                    }
                }
            }
            // Icon-only quality badge (QualityBadge.slint).
            QualityMini {
                id: qBadge
                tier: root.qualityTier
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
