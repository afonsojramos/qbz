// Albums quality/format/source filter popup (LocalLibraryView.slint:2470 —
// the component at the end of the file). Floats OVER the content: a
// click-out backdrop plus a 372px card pinned 36px from the right edge,
// 92px down, radius 10, surface-card, 1px subtle border.
//
// Three sections of FilterChips: Quality (Hi-Res / CD / Lossy), Format
// (FLAC ALAC APE WAV, then MP3 AAC Other), Source (Local / Offline cache /
// Plex). "Clear" appears in the title row only when something is active.
//
// The Slint card carries a 28px drop shadow; QML shadows need the graphical
// effects path, which renders nothing on this port's software renderer
// (QbzIcon.qml documents the same trade), so the border carries the edge.

import QtQuick
import com.blitzfc.qbz
import "../../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    // Click-out backdrop.
    MouseArea {
        anchors.fill: parent
        onClicked: root.view.filterOpen = false
    }

    Rectangle {
        x: parent.width - width - 36
        y: 92
        width: 372
        height: col.height + 32
        color: theme.surfaceCard
        radius: 10
        border.width: 1
        border.color: theme.borderSubtle

        // Swallow clicks so the backdrop does not close the card.
        MouseArea { anchors.fill: parent }

        Column {
            id: col
            x: 16
            y: 16
            width: parent.width - 32
            spacing: 10

            Row {
                width: parent.width
                height: 28
                Text {
                    width: parent.width - (root.view.filterCount > 0 ? clearBtn.width : 0)
                    height: parent.height
                    text: QbzSession.tr("Filter", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontHeading
                    font.weight: theme.weightSemibold
                    verticalAlignment: Text.AlignVCenter
                }
                Rectangle {
                    id: clearBtn
                    visible: root.view.filterCount > 0
                    width: clearText.implicitWidth + 18
                    height: 28
                    radius: 6
                    color: clearArea.containsMouse ? theme.surfaceHover : "transparent"
                    Text {
                        id: clearText
                        anchors.centerIn: parent
                        text: QbzSession.tr("Clear", QbzSession.trRev)
                        color: theme.accent
                        font.pixelSize: theme.fontLegal
                        font.weight: theme.weightMedium
                    }
                    MouseArea {
                        id: clearArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.view.clearFilter()
                    }
                }
            }

            // ---- Quality ----
            Text {
                text: QbzSession.tr("Quality", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                font.weight: theme.weightSemibold
            }
            Row {
                spacing: 8
                FilterChip {
                    label: QbzSession.tr("Hi-Res", QbzSession.trRev)
                    active: root.view.filter.hires === true
                    onToggled: root.view.toggleFilter("hires")
                }
                FilterChip {
                    label: QbzSession.tr("CD", QbzSession.trRev)
                    active: root.view.filter.cd === true
                    onToggled: root.view.toggleFilter("cd")
                }
                FilterChip {
                    label: QbzSession.tr("Lossy", QbzSession.trRev)
                    active: root.view.filter.lossy === true
                    onToggled: root.view.toggleFilter("lossy")
                }
            }

            // ---- Format ----
            Text {
                text: QbzSession.tr("Format", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                font.weight: theme.weightSemibold
            }
            Row {
                spacing: 8
                FilterChip { label: "FLAC"; active: root.view.filter.flac === true; onToggled: root.view.toggleFilter("flac") }
                FilterChip { label: "ALAC"; active: root.view.filter.alac === true; onToggled: root.view.toggleFilter("alac") }
                FilterChip { label: "APE"; active: root.view.filter.ape === true; onToggled: root.view.toggleFilter("ape") }
                FilterChip { label: "WAV"; active: root.view.filter.wav === true; onToggled: root.view.toggleFilter("wav") }
            }
            Row {
                spacing: 8
                FilterChip { label: "MP3"; active: root.view.filter.mp3 === true; onToggled: root.view.toggleFilter("mp3") }
                FilterChip { label: "AAC"; active: root.view.filter.aac === true; onToggled: root.view.toggleFilter("aac") }
                FilterChip {
                    label: QbzSession.tr("Other", QbzSession.trRev)
                    active: root.view.filter.other === true
                    onToggled: root.view.toggleFilter("other")
                }
            }

            // ---- Source ----
            Text {
                text: QbzSession.tr("Source", QbzSession.trRev)
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                font.weight: theme.weightSemibold
            }
            Row {
                spacing: 8
                FilterChip {
                    label: QbzSession.tr("Local", QbzSession.trRev)
                    active: root.view.filter.local === true
                    onToggled: root.view.toggleFilter("local")
                }
                FilterChip {
                    label: QbzSession.tr("Offline cache", QbzSession.trRev)
                    active: root.view.filter.offline === true
                    onToggled: root.view.toggleFilter("offline")
                }
                FilterChip {
                    label: "Plex"
                    active: root.view.filter.plex === true
                    onToggled: root.view.toggleFilter("plex")
                }
            }
        }
    }
}
