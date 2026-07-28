// ArtistCard — THE artist card (discover/ArtistGridCard.slint), shared by
// every surface since phase 21: Home/Editor/ForYou artist rails, the
// Pinned rail, the Library artist grids + All feed, the Search all-tab
// carousels + artists grid + most-popular artist hero. (The Slint mounts
// ArtistGridCard at 200x246 in ALL of these — the POC's old 160x220
// Home/Search cards replicated the legacy ArtistCard the Slint only uses
// on surfaces the POC does not have.)
//
// 200x246: 200px surface-card frame + 190px circle (gradient + user glyph
// until the artwork resolves), hover scrim 0.55 CLIPPED TO THE CIRCLE,
// overlay row at y=113 (follow? / play / more), pin badge top-right of
// the frame, then the meta block (centered name, optional subtitle).
//
// item contract: { id, title, subtitle, following } plus the scalar
// artSource / artworkUrl / isPinned props (the AlbumCard pattern).
//
// Arms:
//  - followMode: "none" (default) | "toggle" — mounts the overlay follow
//    button + the menu Follow row. POC-NOTE: there is NO artist-follow
//    API in the Qt bridge, so every surface mounts "none" today; the
//    "toggle" arm renders but its action is inert.
//  - subtitle: meta switches to 1-line name + muted subtitle (the Slint
//    "Similar to…"/search arm); empty = wrap-2 name.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    property var item: ({})
    // Host-resolved artwork path (the AlbumCard artSource pattern).
    property string artSource: ""
    // Remote cover URL for the pin payload ("" when the host has none).
    property string artworkUrl: ""
    // Pinned state (AlbumCard pattern: scalar prop, optimistic flip on
    // click — the model re-publish re-creates the delegate).
    property bool isPinned: false
    property string followMode: "none"

    color: "transparent"

    QbzTheme { id: theme }

    readonly property bool showFollow: followMode !== "none"
    readonly property bool overlayOn: agArea.containsMouse || pinArea.containsMouse
        || agFollow.hovered || agPlay.hovered || agMore.hovered

    implicitWidth: 200
    implicitHeight: 246

    Column {
        spacing: 0
        Rectangle {
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceCard

            // 190px round portrait, centered (5px surround = the
            // album-tile frame). All overlay content lives inside so it
            // clips to the circle.
            Rectangle {
                x: 5
                y: 5
                width: 190
                height: 190
                radius: 95
                clip: true
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0.0; color: theme.surfaceElevated }
                    GradientStop { position: 1.0; color: theme.surfaceCard }
                }
                // Placeholder glyph until the artwork resolves on top.
                QbzIcon {
                    name: "user"
                    width: 54
                    height: 54
                    anchors.centerIn: parent
                    tintName: "muted"
                }
                RoundedImage {
                    anchors.fill: parent
                    source: root.artSource
                    radius: 95
                }
                // Hover scrim (clipped to the circle by the parent's clip).
                Rectangle {
                    anchors.fill: parent
                    radius: 95
                    color: "#000000"
                    opacity: root.overlayOn ? 0.55 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }
                // Card-open + hover detector (before the buttons so they
                // win the pointer).
                MouseArea {
                    id: agArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.openArtist(root.item.id)
                }
                // Hover overlay — follow? / play / more (y=113).
                CardOverlayRow {
                    y: 113
                    width: parent.width
                    shown: root.overlayOn
                    CardOverlayButton {
                        id: agFollow
                        visible: root.showFollow
                        name: root.item.following ? "check" : "user-plus"
                        active: root.item.following === true
                        anchors.verticalCenter: parent.verticalCenter
                        // POC-NOTE: no artist-follow API in the Qt bridge.
                        onClicked: { }
                    }
                    CardOverlayButton {
                        id: agPlay
                        name: "play-fill"
                        primary: true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: QbzPlayer.playArtistCard(root.item.id)
                    }
                    CardOverlayButton {
                        id: agMore
                        name: "ellipsis"
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: function (mouse) { agMenu.openAtCursor(agMore, mouse.x, mouse.y) }
                    }
                }
                CardMenu {
                    id: agMenu
                    menuWidth: 196
                    entries: {
                        var m = [
                            { "label": QbzSession.tr("Open artist", QbzSession.trRev), "icon": "user", "action": "open" },
                            { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
                        ]
                        if (root.showFollow) m.push({
                            "label": root.item.following ? QbzSession.tr("Following", QbzSession.trRev) : QbzSession.tr("Follow", QbzSession.trRev),
                            "icon": root.item.following ? "check" : "user-plus",
                            "action": "follow" })
                        m.push({ "label": QbzSession.tr("Not interested", QbzSession.trRev), "icon": "thumbs-down", "action": "not-interested" })
                        return m
                    }
                    onPicked: function (a) {
                        if (a === "open") QbzBridge.openArtist(root.item.id)
                        else if (a === "play") QbzPlayer.playArtistCard(root.item.id)
                        // "follow": no artist-follow API (POC-NOTE — inert).
                        // "not-interested": the reco dismissal store is not
                        // open in the POC (POC-NOTE — inert).
                    }
                }
            }
            // Pin badge — top-right of the FRAME (outside the circle clip),
            // opacity follows overlay-on (the AlbumCard convention).
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
                    onClicked: {
                        root.isPinned = !root.isPinned
                        QbzBridge.togglePin("artist", root.item.id, root.item.title,
                            root.item.subtitle || "", root.artworkUrl)
                    }
                }
            }
        }
        Item { width: 1; height: 6 }
        // Meta: centered name; subtitle arm switches to 1-line + muted row.
        Column {
            width: 200
            height: 40
            spacing: 2
            Text {
                width: parent.width
                height: root.item.subtitle ? 20 : 40
                text: root.item.title || ""
                color: agNameArea.containsMouse ? theme.accent : theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: root.item.subtitle ? Text.NoWrap : Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                MouseArea {
                    id: agNameArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.openArtist(root.item.id)
                }
            }
            Text {
                visible: !!root.item.subtitle
                width: parent.width
                height: 16
                text: root.item.subtitle || ""
                color: theme.textMuted
                font.pixelSize: theme.fontLink - 1
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
    }
}
