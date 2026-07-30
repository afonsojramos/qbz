// MyQbzDetailCard — ONE grid card of the My QBZ detail body, the port of the
// local `DetailCard` component in myqbz/MixtapeDetailView.slint:495-594.
//
// Geometry 1:1 with the .slint: `height = cardW + 8 + 18 + 16 + 16`
// (= cardW + 58, :501), radius 8, padding 8, spacing 8, a square artwork of
// `cardW - 16` at radius 6, title 14px/medium, subtitle 12px/muted. Selected
// = a 2px accent border plus the `surface-hover` fill the hover uses.
//
// The host GridView owns the column arithmetic (spec 01 §7.4) and hands the
// width down as `cardW`, exactly as the .slint passes `card-w` (:498).
//
// TWO deviations from the .slint, both deliberate:
//
//   * `cardArea` is declared FIRST and pinned `z: -1`. The .slint declares
//     `card-ta` LAST (:584), i.e. frontmost, which puts it over the artwork's
//     own play / checkbox TouchAreas — the identical defect
//     `MixtapeDetailView.slint:203-217` describes and fixes for the ROW but
//     never fixed here, so the card's hover scrim and its play affordance
//     cannot fire upstream. Fixed on the way over.
//   * `rev` is a dependency of every `item` read — the host patches the row
//     objects IN PLACE and a plain JS object mutation fires no notifier. See
//     the long note in MyQbzDetailRow.qml; the `>= 0` guard is always true
//     and exists only to capture the dependency.

import QtQuick
import com.blitzfc.qbz
import "../../theme"
import "../local"

Rectangle {
    id: root

    /// One `DetailRow` object out of `detailJson.items` (spec 02 §5.2).
    property var item: ({})
    /// Card width, computed by the host from the auto-fill arithmetic.
    property real cardW: 150
    property bool selectMode: false
    /// In-place-patch revision (see the header).
    property int rev: 0

    QbzTheme { id: theme }

    readonly property int itemPosition: root.rev >= 0 ? (root.item.position || 0) : 0
    readonly property string itemType: root.rev >= 0 ? (root.item.itemType || "") : ""
    readonly property string itemSource: root.rev >= 0 ? (root.item.source || "") : ""
    readonly property string sourceItemId: root.rev >= 0 ? (root.item.sourceItemId || "") : ""
    readonly property string rowTitle: root.rev >= 0 ? (root.item.title || "") : ""
    readonly property string rowSubtitle: root.rev >= 0 ? (root.item.subtitle || "") : ""
    readonly property string artPath: root.rev >= 0 ? (root.item.artPath || "") : ""
    readonly property bool selected: root.rev >= 0 && root.item.selected === true

    /// track → music, playlist → list-music, else disc (.slint :522-526).
    readonly property string typeGlyph: root.itemType === "track" ? "music"
        : (root.itemType === "playlist" ? "list-music" : "disc")

    readonly property bool hovered: cardArea.containsMouse || playArea.containsMouse

    width: root.cardW
    height: root.cardW + 58
    radius: 8
    color: (root.hovered || root.selected) ? theme.surfaceHover : "transparent"
    border.width: root.selected ? 2 : 0
    border.color: theme.accent

    // Card-wide target — FIRST + `z: -1` (see the header).
    MouseArea {
        id: cardArea
        anchors.fill: parent
        z: -1
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            if (root.selectMode) QbzMyQbz.detailToggleItemSelect(root.itemPosition)
            else QbzMyQbz.openItem(root.itemSource, root.itemType, root.sourceItemId)
        }
    }

    Column {
        x: 8
        y: 8
        width: parent.width - 16
        spacing: 8

        // --- Artwork ------------------------------------------------------
        Rectangle {
            width: parent.width
            height: width
            radius: 6
            color: theme.surfaceElevated
            clip: true

            RoundedImage {
                visible: root.artPath !== ""
                anchors.fill: parent
                source: root.artPath
                radius: 6
            }
            QbzIcon {
                visible: root.artPath === ""
                anchors.centerIn: parent
                name: root.typeGlyph
                width: 28
                height: 28
                tintName: "muted"
            }

            // Select-mode: the checkbox rides a dark disc so it stays legible
            // over a bright cover (.slint :536-549).
            Rectangle {
                visible: root.selectMode
                x: 6
                y: 6
                width: 22
                height: 22
                radius: 11
                color: root.selected ? "transparent" : "#8c000000"
                SelectCheck {
                    anchors.centerIn: parent
                    on: root.selected
                    onToggled: QbzMyQbz.detailToggleItemSelect(root.itemPosition)
                }
            }

            // Otherwise: the hover play scrim (.slint :550-566).
            Rectangle {
                visible: !root.selectMode
                anchors.fill: parent
                color: "#73000000"
                opacity: playArea.containsMouse ? 1.0 : 0.0
                Behavior on opacity { NumberAnimation { duration: 150 } }
                QbzIcon {
                    anchors.centerIn: parent
                    name: "play-fill"
                    width: 24
                    height: 24
                    tintName: "white"
                }
            }
            MouseArea {
                id: playArea
                anchors.fill: parent
                enabled: !root.selectMode
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: QbzMyQbz.playItem(root.sourceItemId)
            }
        }

        Text {
            width: parent.width
            text: root.rowTitle
            color: theme.textPrimary
            font.pixelSize: 14
            font.weight: theme.weightMedium
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            text: root.rowSubtitle
            color: theme.textMuted
            font.pixelSize: 12
            elide: Text.ElideRight
        }
    }
}
