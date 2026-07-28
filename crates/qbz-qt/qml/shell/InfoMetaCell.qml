// InfoMetaCell — one metadata cell of the Track Info modal: an 11px semibold
// muted label (letter-spacing 0.5) over a value supplied as a child. 1:1 with
// `MetaCell` in crates/qbz-ui/ui/album/TrackInfoModal.slint, which exports it
// for the immersive split panel to reuse the same way.
//
// The value goes in as a plain child — QML appends use-site children to the
// Column's default `data`, so they land under the label:
//
//   InfoMetaCell {
//       cellWidth: 180
//       label: QbzSession.tr("Duration", QbzSession.trRev)
//       Text { text: "3:45" }
//   }

import QtQuick
import "../theme"

Column {
    id: mc
    property string label: ""
    property int cellWidth: 0

    QbzTheme { id: theme }

    width: cellWidth
    spacing: 6

    Text {
        width: mc.cellWidth
        text: mc.label
        color: theme.textMuted
        font.pixelSize: 11
        font.weight: theme.weightSemibold
        font.letterSpacing: 0.5
        elide: Text.ElideRight
    }
}
