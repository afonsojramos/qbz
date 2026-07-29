// LabelCard — THE label card (discover/LabelCard.slint), promoted from
// LibraryView.LibLabelCard in phase 21: Library Labels grid + All feed.
//
// 200x246, the ArtistCard shape: 200px surface-card frame + 190px ROUND
// container, hover scrim CLIPPED TO THE CIRCLE, overlay row at y=113.
// The circle is the owner's directive ("a label should look like an
// artist, not like an album") — the Slint LabelCard is a 200x200 SQUARE;
// everything else here is the Slint 1:1 (see the divergence note below).
//
// From the Slint reference, kept verbatim:
//  - no logo  -> indigo(#6366f1)->violet(#8b5cf6) gradient + disc-3 glyph;
//  - a logo   -> surface-elevated behind a CONTAIN-fit logo, never cropped
//    (label logos have varied/transparent backgrounds, unlike album art);
//  - "a label is opened, not played" — the ONLY action a label exposes
//    anywhere in the Slint is open ("Go to label", disc-3 icon,
//    FavoritesActions.open-label). There is NO play, NO favorite, NO pin
//    and NO ⋯ menu on a label card, so the overlay row carries exactly one
//    disc (open) instead of the artist's follow/play/more triplet.
//
// The label landing page is out of scope — there is no openLabel on the Qt
// bridge. Rather than keep an open affordance that does nothing, the card
// DISABLES it while `hasLabelViewSeam` is false: no overlay disc, no
// pointing-hand cursor, no hover scrim, no accent on the title. The tile
// still renders 1:1; it just does not pretend to be clickable. Flip the
// constant and fill `openLabel()` when the label view lands.
//
// item contract: { id, title, subtitle, imageUrl } plus the
// host-resolved artSource string prop. The artwork is driven by
// artSource ALONE (it used to be gated on item.imageUrl, which hid the
// logo on any mount that resolves covers without republishing the item);
// imageUrl only selects the gradient-vs-elevated placeholder arm.

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
    // Declared for contract parity with the other cards; a label cannot be
    // pinned (the pinned store carries album/artist/playlist only).
    property string artworkUrl: ""

    color: "transparent"

    QbzTheme { id: theme }

    // No label view on the Qt bridge — see the header. Everything that would
    // navigate is gated on this.
    readonly property bool hasLabelViewSeam: false

    // No logo URL -> the indigo/violet glyph arm (the Slint condition).
    readonly property bool noLogo: !root.item.imageUrl
    readonly property bool overlayOn: root.hasLabelViewSeam
        && (lblArea.containsMouse || lblOpen.hovered)

    // Single funnel for every open path (body, overlay disc, title).
    function openLabel() {
        if (!root.hasLabelViewSeam)
            return
        // GLUE NEEDED: QbzBridge/QbzArtist has no openLabel(labelId).
    }

    implicitWidth: 200
    implicitHeight: 246

    Column {
        spacing: 0
        Rectangle {
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceCard

            // 190px round logo container, centered (5px surround = the
            // album-tile frame, ArtistCard). All overlay content lives
            // inside so it clips to the circle.
            Rectangle {
                x: 5
                y: 5
                width: 190
                height: 190
                radius: 95
                clip: true
                // Logo arm background (the Slint Theme.surface-elevated).
                color: theme.surfaceElevated

                // No logo: indigo->violet gradient + disc glyph.
                Rectangle {
                    visible: root.noLogo
                    anchors.fill: parent
                    radius: 95
                    gradient: Gradient {
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0.0; color: "#6366f1" }
                        GradientStop { position: 1.0; color: "#8b5cf6" }
                    }
                    QbzIcon {
                        name: "disc-3"
                        width: 56
                        height: 56
                        anchors.centerIn: parent
                        tintName: "primary"
                    }
                }

                // The logo itself — contain-fit inside the square INSCRIBED
                // in the circle (190 / sqrt(2) ~= 134, so margin 28), which
                // is what keeps a wide wordmark from being cut by the round
                // container. radius 0: the inset box never reaches the edge,
                // so it needs no clip of its own.
                RoundedImage {
                    anchors.fill: parent
                    anchors.margins: 28
                    source: root.artSource
                    radius: 0
                    // Logos are contain-fit, never cropped (LabelCard).
                    fit: "contain"
                }

                // Hover scrim (clipped to the circle by the parent's clip).
                // 0.55, not the Slint's 0.25: the reference has no buttons
                // over it, this one does and 0.25 leaves them unreadable
                // over a light logo.
                Rectangle {
                    anchors.fill: parent
                    radius: 95
                    color: "#000000"
                    opacity: root.overlayOn ? 0.55 : 0.0
                    Behavior on opacity { NumberAnimation { duration: 150 } }
                }

                // Card-open + hover detector (before the disc so it wins
                // the pointer).
                MouseArea {
                    id: lblArea
                    anchors.fill: parent
                    enabled: root.hasLabelViewSeam
                    hoverEnabled: root.hasLabelViewSeam
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openLabel()
                }

                // Hover overlay — open only (y=113, the card-family row).
                CardOverlayRow {
                    y: 113
                    width: parent.width
                    shown: root.overlayOn
                    CardOverlayButton {
                        id: lblOpen
                        visible: root.hasLabelViewSeam
                        name: "disc-3"
                        primary: true
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: root.openLabel()
                    }
                }
            }
        }
        Item { width: 1; height: 6 }
        // Meta: centered under the circle (the ArtistCard block — the Slint
        // LabelCard left-aligns because its art is a square; a centered
        // name is what pairs with a round container, and it keeps labels
        // aligned with artists in the mixed Library All grid).
        Column {
            width: 200
            height: 40
            spacing: 2
            Text {
                width: parent.width
                height: root.item.subtitle ? 20 : 40
                text: root.item.title || ""
                color: lblTitleArea.containsMouse ? theme.accent : theme.textPrimary
                font.pixelSize: theme.fontBody - 2
                font.weight: theme.weightMedium
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
                wrapMode: root.item.subtitle ? Text.NoWrap : Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                MouseArea {
                    id: lblTitleArea
                    anchors.fill: parent
                    enabled: root.hasLabelViewSeam
                    hoverEnabled: root.hasLabelViewSeam
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.openLabel()
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
