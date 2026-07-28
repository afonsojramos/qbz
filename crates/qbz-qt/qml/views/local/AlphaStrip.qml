// Vertical A-Z jump strip (LocalLibraryView.slint:577 AlphaStrip, itself a
// 1:1 lift of FavoritesView's). 18px gutter, 15px rows, 9px semibold muted
// letters that go accent on hover. Emits jump(ordinal, index); the host
// scrolls its own view proportionally — exactly like the Slint (:1298).

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Item {
    id: root

    /// [{ letter, index }] — index = the row ordinal in the flat model.
    property var jumps: []
    signal jump(int ordinal, int index)

    QbzTheme { id: theme }

    width: 18

    Column {
        anchors.centerIn: parent
        spacing: 0
        Repeater {
            model: root.jumps
            delegate: Item {
                id: cell
                required property var modelData
                required property int index
                width: 18
                height: 15
                Text {
                    anchors.fill: parent
                    text: cell.modelData.letter
                    color: cellArea.containsMouse ? theme.accent : theme.textMuted
                    font.pixelSize: 9
                    font.weight: theme.weightSemibold
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                MouseArea {
                    id: cellArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.jump(cell.index, cell.modelData.index)
                }
            }
        }
    }
}
