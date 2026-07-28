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

    QbzTheme { id: theme }

    width: 200
    height: 246
    color: "transparent"

    readonly property bool overlayOn: artArea.containsMouse || pinArea.containsMouse
        || favArea.containsMouse || playArea.containsMouse || moreArea.containsMouse

    function toggleFavorite() {
        root.isFavorite = !root.isFavorite
        QbzBridge.libraryToggleFavorite("album", root.albumId)
    }
    function togglePin() {
        root.isPinned = !root.isPinned
        QbzBridge.togglePin("album", root.albumId, root.title, root.artist, "")
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

            Image {
                anchors.fill: parent
                source: root.artSource
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }

            // Hover scrim.
            Rectangle {
                anchors.fill: parent
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
                onClicked: QbzBridge.openAlbum(root.albumId)
            }

            // Pin badge — top-right. Hover-revealed like the overlay
            // buttons (AlbumCard.slint: opacity follows overlay-on even
            // when pinned — the pinned state reads in the icon swap only:
            // filled accent pin vs outline). Always-mounted (opacity) so
            // its hover joins overlayOn.
            Rectangle {
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
            Row {
                y: 120
                width: parent.width
                height: 44
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 12
                opacity: root.overlayOn ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }

                Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
                Rectangle {
                    width: 36
                    height: 36
                    radius: 18
                    anchors.verticalCenter: parent.verticalCenter
                    color: favArea.containsMouse ? "#3dffffff" : "#24ffffff"
                    border.width: 1.5
                    border.color: "#ccffffff"
                    QbzIcon {
                        name: root.isFavorite ? "heart-filled" : "heart"
                        width: 16
                        height: 16
                        anchors.centerIn: parent
                        tintName: root.isFavorite ? "favorite" : "primary"
                    }
                    MouseArea {
                        id: favArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.toggleFavorite()
                    }
                }
                Rectangle {
                    width: 44
                    height: 44
                    radius: 22
                    color: playArea.containsMouse ? "#d6ffffff" : "#ffffff"
                    QbzIcon {
                        name: "play-fill"
                        width: 18
                        height: 18
                        anchors.centerIn: parent
                        tintName: "black"
                    }
                    MouseArea {
                        id: playArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: QbzBridge.playAlbum(root.albumId)
                    }
                }
                Rectangle {
                    width: 36
                    height: 36
                    radius: 18
                    anchors.verticalCenter: parent.verticalCenter
                    color: moreArea.containsMouse ? "#3dffffff" : "#24ffffff"
                    border.width: 1.5
                    border.color: "#ccffffff"
                    QbzIcon {
                        name: "ellipsis"
                        width: 16
                        height: 16
                        anchors.centerIn: parent
                        tintName: "primary"
                    }
                    MouseArea {
                        id: moreArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: albumMenu.open()
                    }
                }
                Item { width: (200 - 44 - 2 * 36 - 2 * 12) / 2; height: 1 }
            }

            // Context menu (AlbumCard.slint's album-menu): 196px, items
            // 33px, icon 15 + label 13.
            Popup {
                id: albumMenu
                x: 150
                y: 140
                width: 196
                padding: 5
                closePolicy: Popup.CloseOnPressOutside
                background: Rectangle {
                    color: theme.surfaceMain
                    radius: theme.radiusSm
                    border.width: 1
                    border.color: theme.borderMuted
                }
                contentItem: Column {
                    Repeater {
                        model: [
                            { "label": QbzBridge.tr("Open album"), "icon": "library-big", "action": "open" },
                            { "label": QbzBridge.tr("Play"), "icon": "play-fill", "action": "play" },
                            { "label": QbzBridge.tr("Play next"), "icon": "list-plus", "action": "next" },
                            { "label": QbzBridge.tr("Add to queue"), "icon": "list-end", "action": "queue" },
                            { "label": root.isFavorite ? QbzBridge.tr("Remove from Library") : QbzBridge.tr("Add to Library"), "icon": root.isFavorite ? "heart-filled" : "heart", "action": "favorite" },
                            { "label": QbzBridge.tr("Block this album"), "icon": "blind-eye", "action": "block" },
                        ]
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
                                    if (a === "open") QbzBridge.openAlbum(root.albumId)
                                    else if (a === "play") QbzBridge.playAlbum(root.albumId)
                                    else if (a === "next") QbzBridge.enqueueAlbum(root.albumId, "next")
                                    else if (a === "queue") QbzBridge.enqueueAlbum(root.albumId, "later")
                                    else if (a === "favorite") root.toggleFavorite()
                                    // "block" (no blacklist store): inert —
                                    // POC-NOTE.
                                }
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
                x: parent.width - width - 6
                y: parent.height - height - 6
                width: 24
                height: 24
                radius: 4
                color: "#b3000000"
                QbzIcon {
                    name: "hard-drive"
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
                        onClicked: QbzBridge.openAlbum(root.albumId)
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
            Item {
                id: qBadge
                visible: root.qualityTier !== ""
                width: root.qualityTier === "hires" ? 42 : 30
                height: 30
                anchors.verticalCenter: parent.verticalCenter
                Image {
                    visible: root.qualityTier === "hires"
                    source: "assets/hi-res.svg"
                    width: 42
                    height: 28
                    anchors.centerIn: parent
                    sourceSize: Qt.size(84, 56)
                    fillMode: Image.PreserveAspectFit
                }
                Rectangle {
                    visible: root.qualityTier === "cd"
                    width: 30
                    height: 30
                    radius: 3
                    color: theme.surfaceElevated
                    border.width: 1
                    border.color: theme.borderSubtle
                    QbzIcon { name: "cd"; width: 16; height: 16; anchors.centerIn: parent; tintName: "muted" }
                }
            }
        }
    }
}
