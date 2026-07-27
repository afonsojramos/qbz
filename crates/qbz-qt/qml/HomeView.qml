// Content view stub for the only mounted route ("home" = Discover > Home).
// Phase 3 fills this with the real Discover view; for now it proves the
// content frame, theme and i18n wiring.

import QtQuick
import com.blitzfc.qbz

Rectangle {
    color: theme.surfaceMain

    QbzTheme { id: theme }

    Column {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: theme.spacingXl
        spacing: theme.spacingSm

        Text {
            text: QbzBridge.tr("Discover") + " / " + QbzBridge.tr("Home")
            color: theme.textPrimary
            font.pixelSize: theme.fontSection
            font.weight: theme.weightSemibold
        }
        Text {
            text: "QBZ Qt POC — phase 3 fills this view"
            color: theme.textMuted
            font.pixelSize: theme.fontLegal
        }
    }
}
