// QualityBadge — 1:1 port of the Tauri build's QualityBadge.svelte +
// QualityBadgeStatic.svelte (qbz-worktrees/legacy-tauri/src/lib/components/).
//
// Every number here comes from those two files' CSS. Do not "improve" them:
// the owner's requirement is visual parity with Tauri, and the port had
// previously reinvented this badge.
//
// FOUR MODES (the union of both Svelte components):
//   "full"     — 136x36 pill: icon + [TIER / bit-rate], state border, tooltip.
//   "compact"  — narrow bar variant: icon + "24/96", no tier row (issue #303).
//   "bare"     — full's content with the pill chrome stripped, for toolbars.
//   "iconOnly" — Qobuz's title-row mark: the Hi-Res logo alone, or a framed
//                square for CD/MP3 so both line up in a grid.
//
// STATIC vs REACTIVE: QualityBadgeStatic is this component with the four
// state flags left false — it never polls, so list/catalog rows advertise
// their own stored quality without tracking playback. Only the now-playing
// surfaces set downgraded/adjusted/plughw/bitPerfect.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    // ---- Source data ------------------------------------------------------
    property string quality: ""
    property int bitDepth: 0
    property real samplingRate: 0
    property string format: ""
    /// A tier already resolved in Rust (np_quality_tier). Wins over the
    /// derivation below, which exists for rows that only carry raw numbers.
    property string tierOverride: ""

    /// "full" | "compact" | "bare" | "iconOnly"
    property string mode: "full"

    // ---- Reactive state (now-playing surfaces only) -----------------------
    /// The system/pipewire path is resampling: hardware rate < decoded rate.
    property bool downgraded: false
    /// QBZ deliberately fetched a lower quality for hardware compatibility —
    /// the hardware then plays it natively. Informational, not a fault.
    property bool adjusted: false
    /// ALSA plughw fallback: the resample happens inside ALSA because the DAC
    /// refuses the source rate (issue #288). Its own failure mode.
    property bool plughw: false
    /// Confirmed bit-perfect (BitPerfectMode::DirectHardware).
    property bool bitPerfect: false
    /// Effective output rate in kHz, for the tooltip's "output" row.
    property real hardwareRateKHz: 0

    // ---- Tier derivation (QualityBadge.svelte's `tier`, same order) -------
    readonly property string tier: {
        if (tierOverride !== "") return tierOverride
        var fmt = (format || quality || "").toLowerCase()
        // MP3 is checked FIRST — it must never resolve as "CD".
        if (fmt.indexOf("mp3") >= 0) return "mp3"
        if (bitDepth >= 24 && samplingRate > 96) return "max"
        if (bitDepth >= 24) return "hires"
        if (bitDepth === 16 || (samplingRate >= 44.1 && samplingRate <= 48)) return "cd"
        var q = quality.toLowerCase()
        if (q.indexOf("mp3") >= 0 || q.indexOf("320") >= 0) return "lossy"
        if (q.indexOf("hi-res") >= 0 || q.indexOf("hires") >= 0 || q.indexOf("24") >= 0) return "hires"
        if (q.indexOf("cd") >= 0 || q.indexOf("flac") >= 0
            || q.indexOf("lossless") >= 0 || q.indexOf("16") >= 0) return "cd"
        if (samplingRate >= 44.1) return "cd"
        return "cd"
    }

    readonly property bool isHiRes: tier === "max" || tier === "hires"

    readonly property string tierLabel: {
        if (tier === "max" || tier === "hires") return "Hi-Res"
        if (tier === "mp3") return "MP3"
        return "CD"
    }

    readonly property string iconPath: {
        var base = "qrc:/qt/qml/com/blitzfc/qbz/qml/assets/"
        if (isHiRes) return base + "hi-res.svg"
        if (tier === "mp3") return base + "mp3.svg"
        return base + "cd.svg"
    }

    readonly property string displayText: {
        if (tier === "max") return (bitDepth || 24) + "-bit / " + (samplingRate || 192) + " kHz"
        if (tier === "hires") return (bitDepth || 24) + "-bit / " + (samplingRate || 96) + " kHz"
        if (tier === "cd") return (bitDepth || 16) + "-bit / " + (samplingRate || 44.1) + " kHz"
        if (tier === "mp3") return (samplingRate || 44.1) + " kHz"
        return "16-bit / 44.1 kHz"
    }

    // Short form for the narrow bar: drops the unit suffixes so the badge
    // fits ~70px. The tooltip still carries the full string.
    readonly property string compactText: {
        if (tier === "max" || tier === "hires")
            return (bitDepth || 24) + "/" + Math.round(samplingRate || (tier === "max" ? 192 : 96))
        if (tier === "cd") return (bitDepth || 16) + "/" + Math.round(samplingRate || 44.1)
        if (tier === "mp3") return "MP3"
        return QbzSession.tr("CD Quality", QbzSession.trRev)
    }

    readonly property bool isCompact: mode === "compact"
    readonly property bool isBare: mode === "bare"
    readonly property bool isIconOnly: mode === "iconOnly"
    readonly property bool anyIndicator: downgraded || adjusted || plughw

    readonly property string fontFamily: "LINE Seed JP"

    implicitWidth: isIconOnly ? (isHiRes ? 44 : 34) : shell.implicitWidth
    implicitHeight: isIconOnly ? (isHiRes ? 29 : 30) : shell.implicitHeight
    width: implicitWidth
    height: implicitHeight

    // ---- icon-only ---------------------------------------------------------
    // Hi-Res renders the brand mark bare (its own rounded rectangle is part of
    // the artwork); CD/MP3 sit in a small framed square of matching outer
    // size, plus the 4px right margin that compensates the Hi-Res SVG's
    // built-in transparent gutter so both variants terminate at the same X.
    Loader {
        active: root.isIconOnly
        sourceComponent: Item {
            width: root.width
            height: root.height
            Rectangle {
                visible: !root.isHiRes
                width: 30
                height: 30
                radius: 3
                color: theme.alphaTier(8)
                border.width: 1
                border.color: theme.alphaTier(12)
            }
            Image {
                source: root.iconPath
                width: root.isHiRes ? 44 : 16
                height: root.isHiRes ? 29 : 16
                x: root.isHiRes ? 0 : (30 - width) / 2
                y: root.isHiRes ? 0 : (30 - height) / 2
                fillMode: Image.PreserveAspectFit
                sourceSize: Qt.size(Math.round(width * 2), Math.round(height * 2))
            }
            ToolTip.visible: iconHover.containsMouse
            ToolTip.text: root.tierLabel + ": " + root.displayText
            MouseArea {
                id: iconHover
                anchors.fill: parent
                hoverEnabled: true
            }
        }
    }

    // ---- full / compact / bare --------------------------------------------
    Rectangle {
        id: shell
        visible: !root.isIconOnly
        // full: 136x36 pill. compact: hugs its content. bare: no chrome.
        implicitWidth: root.isBare
            ? contentRow.implicitWidth
            : Math.max(root.isCompact ? 0 : 136, contentRow.implicitWidth + 2 * hPad)
        implicitHeight: root.isBare ? contentRow.implicitHeight
            : (root.isCompact ? contentRow.implicitHeight + 2 * vPad : 36)
        readonly property int hPad: root.isBare ? 0 : (root.isCompact ? 6 : 10)
        readonly property int vPad: root.isBare ? 0 : (root.isCompact ? 4 : 5)
        radius: root.isBare ? 0 : 4
        color: root.isBare ? "transparent" : theme.alphaTier(6)
        border.width: root.isBare ? 0 : 1
        // The state borders: each failure mode gets its own hue so the user
        // knows where to look for the fix.
        border.color: root.downgraded ? "#4ceab308"
            : root.plughw ? "#4ca855f7"
            : root.bitPerfect ? "#4022c55e"
            : theme.alphaTier(10)

        Row {
            id: contentRow
            x: shell.hPad
            y: shell.vPad
            spacing: root.isCompact ? 4 : 6

            // Icon container — fixed width so the text column never shifts
            // between tiers.
            Item {
                width: root.isCompact ? 20 : 24
                height: root.isCompact ? 20 : 24
                anchors.verticalCenter: undefined
                Image {
                    anchors.centerIn: parent
                    source: root.iconPath
                    // Hi-Res fills the container; CD/MP3 are the smaller glyph.
                    width: root.isHiRes ? (root.isCompact ? 20 : 24) : (root.isCompact ? 14 : 16)
                    height: width
                    fillMode: Image.PreserveAspectFit
                    sourceSize: Qt.size(Math.round(width * 2), Math.round(height * 2))
                }
            }

            // Text block
            Column {
                spacing: 0

                // Compact drops the tier row entirely.
                Row {
                    visible: !root.isCompact
                    spacing: 2
                    Text {
                        text: root.tierLabel.toUpperCase()
                        font.family: root.fontFamily
                        font.pixelSize: 8
                        font.weight: Font.Thin
                        font.letterSpacing: 0.5
                        color: "#b0b0b0"
                    }
                    Text {
                        visible: root.anyIndicator
                        text: "↓"
                        font.family: root.fontFamily
                        font.pixelSize: 8
                        font.bold: true
                        color: root.adjusted ? "#60a5fa" : root.plughw ? "#a855f7" : "#eab308"
                    }
                    Text {
                        visible: root.bitPerfect
                        text: "✓"
                        font.family: root.fontFamily
                        font.pixelSize: 8
                        font.bold: true
                        color: "#22c55e"
                    }
                }

                Row {
                    spacing: 2
                    Text {
                        text: root.isCompact ? root.compactText : root.displayText
                        font.family: root.fontFamily
                        font.pixelSize: root.isCompact ? 11 : 9
                        font.weight: root.isCompact ? Font.Medium : Font.Thin
                        font.letterSpacing: root.isCompact ? 0.2 : 0
                        color: root.isCompact ? theme.textPrimary : "#999999"
                    }
                    // Compact carries the indicators on the value row, since
                    // it has no tier row to hang them from.
                    Text {
                        visible: root.isCompact && root.anyIndicator
                        text: "↓"
                        font.family: root.fontFamily
                        font.pixelSize: 11
                        font.bold: true
                        color: root.adjusted ? "#60a5fa" : root.plughw ? "#a855f7" : "#eab308"
                    }
                    Text {
                        visible: root.isCompact && root.bitPerfect
                        text: "✓"
                        font.family: root.fontFamily
                        font.pixelSize: 11
                        font.bold: true
                        color: "#22c55e"
                    }
                }
            }
        }

        MouseArea {
            id: shellHover
            anchors.fill: parent
            hoverEnabled: true
            // `cursor: help` in the CSS; Qt's nearest is WhatsThis.
            cursorShape: Qt.WhatsThisCursor
        }

        // Plain hover title when there is nothing to explain. The explained
        // states get the rich tooltip below instead.
        ToolTip.visible: shellHover.containsMouse && !root.anyIndicator
        ToolTip.text: root.tierLabel + ": " + root.displayText
    }

    // ---- Rich tooltip for the three explained states -----------------------
    // Source/output rows, then the reason and the fix. Same precedence as the
    // Svelte: plughw wins, then adjusted, then downgraded.
    Loader {
        active: !root.isIconOnly && shellHover.containsMouse && root.anyIndicator
        sourceComponent: Rectangle {
            parent: root
            z: 9999
            x: (root.width - width) / 2
            y: -height - 8
            width: Math.max(180, Math.min(240, col.implicitWidth + 24))
            height: col.implicitHeight + 16
            color: theme.surfaceCard
            border.width: 1
            border.color: theme.alphaTier(10)
            radius: 6
            opacity: 0
            Component.onCompleted: opacity = 1
            Behavior on opacity { NumberAnimation { duration: 150 } }

            Column {
                id: col
                x: 12
                y: 8
                width: parent.width - 24
                spacing: 4

                Column {
                    spacing: 1
                    Text {
                        text: QbzSession.tr("SOURCE", QbzSession.trRev)
                        font.pixelSize: 9
                        font.letterSpacing: 0.5
                        color: theme.textMuted
                    }
                    Text {
                        text: root.adjusted && root.samplingRate > 0
                            ? root.samplingRate + " kHz"
                            : root.displayText
                        font.pixelSize: 12
                        font.weight: Font.Medium
                        color: theme.textPrimary
                    }
                }
                Column {
                    spacing: 1
                    Text {
                        text: QbzSession.tr("OUTPUT", QbzSession.trRev)
                        font.pixelSize: 9
                        font.letterSpacing: 0.5
                        color: theme.textMuted
                    }
                    Text {
                        text: root.hardwareRateKHz > 0
                            ? root.hardwareRateKHz + " kHz"
                            : root.displayText
                        font.pixelSize: 12
                        font.weight: Font.Medium
                        color: theme.textPrimary
                    }
                }
                Rectangle {
                    width: parent.width
                    height: 1
                    color: theme.alphaTier(10)
                }
                Text {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    font.pixelSize: 11
                    font.weight: Font.Medium
                    color: root.adjusted ? "#60a5fa" : "#eab308"
                    text: root.plughw
                        ? QbzSession.tr("ALSA is resampling this stream", QbzSession.trRev)
                        : root.adjusted
                        ? QbzSession.tr("Quality adjusted for your hardware", QbzSession.trRev)
                        : QbzSession.tr("This stream is being resampled", QbzSession.trRev)
                }
                Text {
                    visible: !root.adjusted
                    width: parent.width
                    wrapMode: Text.WordWrap
                    font.pixelSize: 10
                    lineHeight: 1.3
                    color: theme.textMuted
                    text: root.plughw
                        ? QbzSession.tr("Your DAC refused the source rate, so ALSA converted it. Pick a rate the device supports in Settings > Audio.", QbzSession.trRev)
                        : QbzSession.tr("Check that exclusive mode is on and no other app holds the device.", QbzSession.trRev)
                }
            }
        }
    }
}
