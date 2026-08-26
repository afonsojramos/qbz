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
import "../controls"
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
        // Shared with Album Quick View; AudioStamp retains ownership of the
        // delivered/source decision and supplies only the resolved display.
        QualityInline {
            id: qualityRow
            anchors.right: parent.right
            maxWidth: root.maxWidth
            tierLabel: root.showDelivered ? root.deliveredTierLabel : root.tierLabel
            detail: root.showDelivered
                ? QbzPlayer.npQualityTrueDetail : QbzPlayer.npQualityDetail
            downgraded: QbzPlayer.npQualityDowngraded
            tooltipText: root.qualityTooltip
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
