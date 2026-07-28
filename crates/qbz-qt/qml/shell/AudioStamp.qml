// AudioStamp — 1:1 port of crates/qbz-ui/ui/shell/AudioStamp.slint.
//
// The inline 2-row now-playing stamp mounted by the SMALL and LARGE bars (the
// New and Classic bars mount the song card's QualityBadgeFull stamp instead):
//
//   Row 1 (focus):     [TIER] [↓] [detail]      e.g.  HI-RES  24-bit / 96 kHz
//   Row 2 (secondary): [BACKEND] [MODE]         the two LEDs, smaller, below
//
// Row 1 is the brighter line (detail = text-primary 9px/600, tier =
// text-secondary 9px/700) so the 7px LEDs never steal focus. No pills
// (ADR-008): the backend LED goes blue #5b8def when active, the mode LED green
// #3fae6a, both text-muted otherwise. Everything is right-aligned and clamped
// by `maxWidth` so it can never blow out the control row.
//
// Row 2 has three mutually exclusive variants, exactly as in the .slint:
//   local  — backend + mode LEDs (only while neither remote nor casting)
//   cast   — one purple #a855f7 "DLNA"/"CAST" LED
//   remote — amber #e0b341 monitor-speaker icon + the QConnect renderer name
//
// While the stream is downgraded, row 1 reports the DELIVERED tier/detail with
// an amber #eab308 down arrow and the catalog max moves into the tooltip
// ("Source" / "Output" + the cause line).
//
// The tier label is built MANUALLY (hires→HI-RES, mp3→MP3, lossless→LOSSLESS,
// else CD) — this component deliberately does NOT use QualityBadgeFull, which
// carries the format icon and stacks tier over detail.

import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    QbzTheme { id: theme }

    /// Width clamp (Slint: the VerticalLayout's max-width at the call site).
    property int maxWidth: 150

    // Manual tier label: hires→HI-RES, mp3→MP3, lossless→LOSSLESS, else→CD.
    readonly property string tierLabel: QbzPlayer.npQualityTier === "hires" ? "HI-RES"
        : (QbzPlayer.npQualityTier === "mp3" ? "MP3"
        : (QbzPlayer.npQualityTier === "lossless" ? "LOSSLESS" : "CD"))

    // DELIVERED-first main line: while downgraded, row 1 reports what is
    // actually playing and the catalog max moves to the tooltip's "Source".
    readonly property bool showDelivered: QbzPlayer.npQualityDowngraded
        && QbzPlayer.npQualityTrueDetail !== ""
    readonly property string deliveredTierLabel: QbzPlayer.npQualityEffectiveTier === "hires"
        ? "HI-RES"
        : (QbzPlayer.npQualityEffectiveTier === "mp3" ? "MP3" : "CD")

    // WHY the stream is below the catalog max, keyed by the QualityLimit
    // discriminant. Informative, not alarming: a downgrade caused by the
    // user's own setting is the app doing what it was told. Cause 3 only ever
    // fires while casting, so it NAMES the renderer (its sibling cause 2 says
    // "device" for the local DAC).
    readonly property string causeLine: {
        var c = QbzPlayer.npQualityLimitCause
        if (c === 1) return QbzSession.tr("Limited by your streaming quality setting", QbzSession.trRev)
        if (c === 2) return QbzSession.tr("Limited by your output device's limit", QbzSession.trRev)
        if (c === 3) return QbzSession.tr("Limited by the quality cap for {}", QbzSession.trRev)
            .replace("{}", QbzPlayer.npCastTarget)
        if (c === 4) return QbzSession.tr("Qobuz has no higher quality for this track", QbzSession.trRev)
        return ""
    }

    // Tooltip for row 1: the catalog max ("Source") vs the truly delivered
    // stream ("Output") plus the cause when known; the plain tier + detail
    // otherwise. Empty when no detail exists (never a bare "CD: ").
    readonly property string qualityTooltip: {
        if (QbzPlayer.npQualityDetail === "") return ""
        if (!showDelivered) return tierLabel + ": " + QbzPlayer.npQualityDetail
        var t = QbzSession.tr("Source: {}", QbzSession.trRev).replace("{}", QbzPlayer.npQualityDetail)
            + "\n"
            + QbzSession.tr("Output: {}", QbzSession.trRev).replace("{}", QbzPlayer.npQualityTrueDetail)
        return causeLine === "" ? t : t + "\n" + causeLine
    }

    readonly property bool localOutput: !QbzPlayer.npIsRemote && !QbzPlayer.npCastActive
    readonly property bool castOnly: QbzPlayer.npCastActive && !QbzPlayer.npIsRemote
    readonly property bool remoteNamed: QbzPlayer.npIsRemote && QbzPlayer.npCastTarget !== ""

    implicitWidth: Math.min(maxWidth, Math.max(qualityRow.implicitWidth,
        ledRow.visible ? ledRow.implicitWidth : 0,
        castRow.visible ? castRow.implicitWidth : 0,
        remoteRow.visible ? remoteRow.implicitWidth : 0))
    implicitHeight: stack.implicitHeight
    width: implicitWidth
    height: implicitHeight

    Column {
        id: stack
        width: root.width
        spacing: 1

        // === Row 1 — quality, INLINE + BRIGHT (the focus) ================
        // Wrapped in an Item so the hover MouseArea can `anchors.fill` it —
        // an anchored Item inside a Row is a QML error ("Cannot specify
        // anchors for items inside Row").
        Item {
            id: qualityRow
            anchors.right: parent.right
            implicitWidth: qualityLine.implicitWidth
            implicitHeight: qualityLine.implicitHeight
            width: implicitWidth
            height: implicitHeight

            Row {
                id: qualityLine
                anchors.right: parent.right
                spacing: 5

                Text {
                    id: tierText
                    // Delivered tier while downgraded; catalog tier otherwise.
                    text: root.showDelivered ? root.deliveredTierLabel : root.tierLabel
                    font.pixelSize: 9
                    font.weight: Font.Bold
                    font.letterSpacing: 0.3
                    color: theme.textSecondary
                    anchors.verticalCenter: parent.verticalCenter
                }
                // Downgrade arrow (glyph, not a pill — ADR-008). Amber so it
                // reads as "adjusted", not as an error.
                Text {
                    id: arrowText
                    visible: QbzPlayer.npQualityDowngraded
                    text: "↓"
                    font.pixelSize: 9
                    font.weight: Font.Bold
                    color: "#eab308"
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    // Delivered detail while downgraded; catalog otherwise.
                    text: root.showDelivered ? QbzPlayer.npQualityTrueDetail : QbzPlayer.npQualityDetail
                    font.pixelSize: 9
                    font.weight: Font.DemiBold
                    color: theme.textPrimary
                    elide: Text.ElideRight
                    // Elide needs an explicit width. Derived from the clamp
                    // minus the sibling widths — none of which depend on this
                    // one, so there is no binding loop.
                    width: Math.max(0, Math.min(implicitWidth,
                        root.maxWidth - tierText.implicitWidth - 5
                            - (arrowText.visible ? arrowText.implicitWidth + 5 : 0)))
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            MouseArea {
                id: qualityHover
                anchors.fill: parent
                hoverEnabled: true
            }

            ToolTip {
                id: qualityTip
                visible: qualityHover.containsMouse && root.qualityTooltip !== ""
                text: root.qualityTooltip
                delay: 0
                timeout: -1
                padding: 0
                x: qualityRow.width - width
                y: -height - 6
                contentItem: Text {
                    text: qualityTip.text
                    color: theme.textSecondary
                    font.pixelSize: 11
                    font.weight: theme.weightMedium
                    lineHeight: 1.25
                    leftPadding: 9
                    rightPadding: 9
                    topPadding: 6
                    bottomPadding: 6
                }
                background: Rectangle {
                    color: theme.surfaceElevated
                    radius: theme.radiusSm
                    border.width: 1
                    border.color: theme.borderSubtle
                }
            }
        }

        // === Row 2a — the LEDs (local output) ============================
        Row {
            id: ledRow
            visible: root.localOutput
            anchors.right: parent.right
            spacing: 5

            Text {
                text: QbzPlayer.npOutputBackendLabel
                font.pixelSize: 7
                font.weight: Font.Bold
                color: QbzPlayer.npOutputBackendActive ? "#5b8def" : theme.textMuted
                elide: Text.ElideRight
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: QbzPlayer.npOutputModeLabel
                font.pixelSize: 7
                font.weight: Font.Bold
                color: QbzPlayer.npOutputModeActive ? "#3fae6a" : theme.textMuted
                elide: Text.ElideRight
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        // === Row 2b — local-network cast (Chromecast / DLNA) =============
        Row {
            id: castRow
            visible: root.castOnly
            anchors.right: parent.right

            Text {
                text: QbzPlayer.npCastProtocol === "dlna" ? "DLNA" : "CAST"
                font.pixelSize: 7
                font.weight: Font.Bold
                color: "#a855f7"
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        // === Row 2c — QConnect renderer-name caption =====================
        Row {
            id: remoteRow
            visible: root.remoteNamed
            anchors.right: parent.right
            spacing: 3

            QbzIcon {
                name: "monitor-speaker"
                // Baked #e0b341 variant (assets/icons/amber/) — the software
                // path cannot recolour at render time.
                tintName: "amber"
                width: 10
                height: 10
                anchors.verticalCenter: parent.verticalCenter
            }
            Text {
                text: QbzPlayer.npCastTarget
                font.pixelSize: 8
                font.weight: Font.DemiBold
                color: "#e0b341"
                elide: Text.ElideRight
                width: Math.min(implicitWidth, 220)
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
