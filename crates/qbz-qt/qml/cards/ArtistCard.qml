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
//  - followMode: "none" (default) | "toggle" — asks for the overlay follow
//    button + the menu Follow row. There is NO artist-follow invokable on
//    the Qt bridge, so `hasFollowSeam` below force-collapses the arm: the
//    button and the row cannot render inert even if a host sets "toggle".
//  - subtitle: meta switches to 1-line name + muted subtitle (the Slint
//    "Similar to…"/search arm); empty = wrap-2 name.
//
// --- Menu inventory vs discover/ArtistGridCard.slint -------------------
//   Open artist · Play · Follow/Following (show-follow) · Not interested
// The first two are live. The last two are gated off (`hasFollowSeam`,
// `hasNotInterestedSeam`) — both used to render and do nothing at all.
// ⋯ and a right press on the portrait or the name open the same menu.

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

    // --- Seams the Qt bridge does NOT have (menu-parity round) -----------
    //   hasFollowSeam        -> artist/label follow (no invokable; the
    //                           .slint routes to media-action(kind, id,
    //                           "follow"))
    //   hasNotInterestedSeam -> the reco-scoped dismissal store is not
    //                           opened by the Qt port
    // Both entries stay out of the menu (and the follow BUTTON stays out of
    // the overlay) until their invokable lands.
    readonly property bool hasFollowSeam: false
    readonly property bool hasNotInterestedSeam: false

    color: "transparent"

    QbzTheme { id: theme }

    readonly property bool showFollow: followMode !== "none" && hasFollowSeam
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
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onClicked: function (mouse) {
                        if (mouse.button === Qt.RightButton)
                            agMenu.openAtCursor(agArea, mouse.x, mouse.y)
                        else
                            QbzArtist.openArtist(root.item.id)
                    }
                }
                // Hover overlay — follow? / play / more (y=113).
                CardOverlayRow {
                    y: 113
                    width: parent.width
                    shown: root.overlayOn
                    CardOverlayButton {
                        id: agFollow
                        // showFollow already folds in hasFollowSeam, so this
                        // button never renders while the seam is missing.
                        visible: root.showFollow
                        name: root.item.following ? "check" : "user-plus"
                        active: root.item.following === true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: root.menuAction("follow")
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
                    entries: root.menuModel()
                    onPicked: function (a) { root.menuAction(a) }
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
                    // On a #99000000/#cc000000 scrim — dark under every theme.
                    tintName: root.isPinned ? "accent" : "white"
                }
                MouseArea {
                    id: pinArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.isPinned = !root.isPinned
                        QbzLibrary.togglePin("artist", root.item.id, root.item.title,
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
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onClicked: function (mouse) {
                        if (mouse.button === Qt.RightButton)
                            agMenu.openAtCursor(agNameArea, mouse.x, mouse.y)
                        else
                            QbzArtist.openArtist(root.item.id)
                    }
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

    // ArtistGridCard.slint's artist-menu, in its order.
    function menuModel() {
        var t = QbzSession.tr
        var r = QbzSession.trRev
        var m = [
            { "label": t("Open artist", r), "icon": "user", "action": "open" },
            { "label": t("Play", r), "icon": "play-fill", "action": "play" },
        ]
        if (root.showFollow)
            m.push({ "label": root.item.following ? t("Following", r) : t("Follow", r),
                     "icon": root.item.following ? "check" : "user-plus", "action": "follow" })
        // Reco-scoped dismissal (NOT the blacklist) — the artist leaves the
        // Recommendations rails only.
        if (root.hasNotInterestedSeam)
            m.push({ "label": t("Not interested", r), "icon": "thumbs-down", "action": "not-interested" })
        return m
    }

    function menuAction(a) {
        if (a === "open") QbzArtist.openArtist(root.item.id)
        else if (a === "play") QbzPlayer.playArtistCard(root.item.id)
        // "follow" / "not-interested" are unreachable while their
        // has*Seam constant is false — the entries are not built.
    }
}
