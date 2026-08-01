// Vertical A-Z jump strip (favorites/FavoritesView.slint:398 `AlphaStrip`;
// LocalLibraryView.slint:577 is a 1:1 lift of the same component). 18px
// gutter, 15px rows, 9px semibold muted letters that go accent on hover.
// Emits jump(ordinal, index); the host scrolls its own view — exactly like
// the Slint, which scrolls proportionally on the grouped grids (:1996) and by
// row index on the uniform track list (:1687).
//
// PROMOTED from views/local/AlphaStrip.qml, unchanged except for the import
// depth — same move QbzMultiSelectBar made and for the same reason: the Slint
// component was always shared (Local Library albums/artists/tracks AND the
// Library favorites tabs), so the "local" folder was a naming accident. The
// old views/local/AlphaStrip.qml is GONE; this file is the only copy.
// Call sites: views/local/{LocalAlbumsTab,LocalArtistsTab,LocalTracksTab}.qml,
// views/library/{LibraryAlbumsList}.qml and views/LibraryView.qml.

import QtQuick
import com.blitzfc.qbz
import "../theme"

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
