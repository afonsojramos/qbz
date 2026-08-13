// Radio / Top-Tracks card — QML port of discover/RadioCard.slint.
//
// A 200x246 tile matching AlbumCard / PlaylistCard: a 200px "display" panel
// with the seed artwork centered (126px square, nudged 8px up) and a wordmark
// ("RADIO" / "TOP TRACKS") over it, then a title + subtitle below. Hover dims
// the panel (#000000 @ 0.45, 150ms) and reveals a 44px white play disc.
//
// Numbers come from RadioCard.slint, not from taste:
//   panel 200x200 radius Radius.sm · display 126x126 radius 4, centered,
//   y offset -8 · wordmark 15px bold letter-spacing 2 at bottom+16 ·
//   play disc 44px, glyph 18px black · meta: body/medium then legal/muted,
//   both elided at 200px.
//
// WHY THIS IS A CARD AND NOT A PARAMETER ON AlbumCard (track rule 5): the
// two share the 200x246 footprint and nothing else. The album card is
// artwork-first with a hover action ROW (play / favourite / pin / more), a
// quality badge, an award ribbon and a context menu keyed on an album id;
// this one has no id, no menu, no badges, and its artwork is a small inset
// "display" on a panel rather than the tile itself. Bending AlbumCard into
// it would mean disabling five features and re-laying out the art.
//
// DELTA vs the .slint, inherited from it: the Tauri original derives the
// panel colour from the artwork's dominant palette. The Slint port stands a
// neutral elevated panel in its place and says so; this follows the Slint.

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Item {
    id: root

    /// The line under the panel (the seed's name).
    property string seedTitle: ""
    /// The muted second line ("" hides it, like the .slint `if`).
    property string seedSubtitle: ""
    /// The wordmark over the display.
    property string label: QbzSession.tr("RADIO", QbzSession.trRev)
    /// file:// path to the seed artwork (already resolved by Rust).
    property string artSource: ""

    /// Whole-card activation (the .slint `clicked`).
    signal activated()
    /// The hover play disc (the .slint `play`) — the same action in both
    /// call sites today, kept separate so a future caller can split them.
    signal playRequested()

    QbzTheme { id: theme }

    width: 200
    height: 246

    Column {
        anchors.fill: parent
        spacing: 8

        // --- display panel ------------------------------------------------
        Rectangle {
            id: panel
            width: 200
            height: 200
            radius: theme.radiusSm
            color: theme.surfaceElevated
            // No clip: with the wordmark bounded below, nothing in this tile
            // can cross its edge.

            // Centered artwork "display", nudged up 8px to leave room for
            // the wordmark.
            Rectangle {
                width: 126
                height: 126
                x: Math.round((parent.width - width) / 2)
                y: Math.round((parent.height - height) / 2) - 8
                radius: 4
                color: theme.surfaceCard
                // No clip: RoundedImage confines itself on both arms; a clip is an
                // unconditional batch root, one per visible card.
                RoundedImage {
                    anchors.fill: parent
                    source: root.artSource
                    radius: 4
                }
            }

            // Wordmark over the display, bottom.
            Text {
                text: root.label
                color: "#e6ffffff"
                font.pixelSize: 15
                font.weight: theme.weightBold
                font.letterSpacing: 2
                // Bounded + centred-in-box instead of centred-by-width: this
                // wordmark is the ONLY thing that could leave the tile (the PT
                // string "PRINCIPAIS FAIXAS" at 15px bold with 2px letter
                // spacing reaches the 200px edge), and bounding it is what
                // lets both of this card's clips go. Worst case is now a right
                // ellipsis instead of today's chop at both edges.
                width: parent.width - 24
                x: 12
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                y: parent.height - height - 16
            }

            // Hover scrim + play disc.
            Rectangle {
                anchors.fill: parent
                color: "#000000"
                opacity: panelArea.containsMouse || playArea.containsMouse ? 0.45 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
            }
            Rectangle {
                width: 44
                height: 44
                radius: 22
                color: "#ffffff"
                anchors.centerIn: parent
                opacity: panelArea.containsMouse || playArea.containsMouse ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
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
                    // The disc is invisible until the panel is hovered, so it
                    // must not swallow the first press before it appears.
                    enabled: parent.opacity > 0
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.playRequested()
                }
            }

            MouseArea {
                id: panelArea
                anchors.fill: parent
                // BELOW the play disc in declaration order would eat its
                // clicks; the disc is declared first, so this one is last and
                // therefore ON TOP. z pushes it back under.
                z: -1
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.activated()
            }
        }

        // --- meta ------------------------------------------------------
        Text {
            width: 200
            text: root.seedTitle
            color: theme.textPrimary
            font.pixelSize: theme.fontBody
            font.weight: theme.weightMedium
            elide: Text.ElideRight
        }
        Text {
            visible: root.seedSubtitle !== ""
            width: 200
            text: root.seedSubtitle
            color: theme.textMuted
            font.pixelSize: theme.fontLegal
            elide: Text.ElideRight
        }
    }
}
