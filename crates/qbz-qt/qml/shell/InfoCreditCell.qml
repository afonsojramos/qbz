// InfoCreditCell — one credit cell of the Track Info modal: the role label
// (11px semibold muted, letter-spacing 0.5) over the performer names, ONE
// CLICKABLE NAME PER LINE. 1:1 with `CreditCell` in
// crates/qbz-ui/ui/album/TrackInfoModal.slint, including its reasoning: there
// is no flow layout, so a wrapping row of links would clip — a vertical stack
// keeps click-to-musician AND never overflows the column.
//
// `cell` is one entry of the trackInfo document's `credits` array:
//   { "role": "PRODUCER", "roleRaw": "Producer", "names": ["A", "B"] }

import QtQuick
import "../theme"

Column {
    id: cc
    property var cell: null
    /// Fixed cell width handed down by the modal so the two columns stay
    /// aligned and the value text wraps inside the column instead of
    /// dictating its width.
    property int colW: 280

    signal nameClicked(string name, string roleRaw)

    QbzTheme { id: theme }

    width: colW
    spacing: 6

    Text {
        width: cc.colW
        text: cc.cell ? (cc.cell.role || "") : ""
        color: theme.textMuted
        font.pixelSize: 11
        font.weight: theme.weightSemibold
        font.letterSpacing: 0.5
        wrapMode: Text.WordWrap
    }

    Column {
        spacing: 2
        Repeater {
            model: cc.cell ? (cc.cell.names || []) : []
            delegate: Item {
                id: nameRow
                required property string modelData
                width: cc.colW
                height: nameText.implicitHeight
                Text {
                    id: nameText
                    width: cc.colW
                    text: nameRow.modelData
                    color: nameArea.containsMouse ? theme.accent : theme.textPrimary
                    font.pixelSize: 14
                    wrapMode: Text.WordWrap
                }
                MouseArea {
                    id: nameArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: cc.nameClicked(nameRow.modelData,
                                              cc.cell ? (cc.cell.roleRaw || "") : "")
                }
            }
        }
    }
}
