// Shared theme tokens (phase 19: LIVE) — the color tokens now bind to
// QbzShell.themeJson, the serialized qbz-theme ThemeColors for the
// persisted ui_prefs `theme` slug (theme_qt.rs — the Qt equivalent of the
// Slint theme::push_colors: switching the theme republishes the document
// and every consumer repaints, no restart). Until the first publish lands
// the baked dark fallback below applies (the bridge seeds themeJson at
// construction, so the fallback is purely defensive).
//
// The non-color tokens (spacing / radius / typography / layout from
// foundation/*.slint) stay static regardless of theme.
//
// Kept as a plain instantiable QtObject (NOT a pragma Singleton — the
// cxx-qt-build generated qmldir cannot mark singletons).

import QtQuick
import com.blitzfc.qbz

QtObject {
    // Baked dark fallback (pre-publish only) — the old static POC tokens.
    readonly property var _fallback: ({
        "surfaceMain": "#0f0f0f", "surfaceCard": "#1a1a1a",
        "surfaceElevated": "#2a2a2a", "surfaceHover": "#10ffffff",
        "bgHover": "#2a2a2a",
        "textPrimary": "#ffffff", "textSecondary": "#cccccc",
        "textMuted": "#888888", "textDisabled": "#555555",
        "accent": "#4285f4", "accentHover": "#5a9bf4",
        "accentPressed": "#3275e4", "accentText": "#ffffff",
        "danger": "#e0564f", "dangerBg": "#26e0564f",
        "dangerBorder": "#59e0564f", "dangerHover": "#e87670",
        "warning": "#fbbf24", "warningBg": "#26fbbf24",
        "warningBorder": "#59fbbf24", "warningHover": "#fdcb47",
        "success": "#3fae6a", "successBg": "#263fae6a",
        "successBorder": "#593fae6a", "successHover": "#55bd7e",
        "borderSubtle": "#14ffffff", "borderMuted": "#38ffffff",
        "borderStrong": "#59ffffff", "focusRing": "#4285f4",
        "favorite": "#ef4444", "cardShadow": "#66000000",
        "surfaceCardA50": "#801a1a1a", "surfaceMainA22": "#380f0f0f",
        "surfaceMainA30": "#4d0f0f0f", "frostBorder": "#1affffff",
        "isDark": true
    })

    readonly property var _doc: {
        if (QbzShell.themeJson === "") return _fallback
        try {
            return JSON.parse(QbzShell.themeJson)
        } catch (e) {
            return _fallback
        }
    }
    function _c(key) {
        const v = _doc[key]
        return v === undefined ? _fallback[key] : v
    }

    // --- Theme colors (live; ThemeColors contract) ----------------------
    readonly property bool isDark: _doc.isDark === undefined ? true : _doc.isDark
    readonly property color surfaceMain: _c("surfaceMain")
    readonly property color surfaceCard: _c("surfaceCard")
    readonly property color surfaceElevated: _c("surfaceElevated")
    readonly property color surfaceHover: _c("surfaceHover")
    readonly property color bgHover: _c("bgHover")
    readonly property color textPrimary: _c("textPrimary")
    readonly property color textSecondary: _c("textSecondary")
    readonly property color textMuted: _c("textMuted")
    readonly property color textDisabled: _c("textDisabled")
    readonly property color accent: _c("accent")
    readonly property color accentHover: _c("accentHover")
    readonly property color accentPressed: _c("accentPressed")
    readonly property color accentText: _c("accentText")
    readonly property color danger: _c("danger")
    readonly property color dangerBg: _c("dangerBg")
    readonly property color dangerBorder: _c("dangerBorder")
    readonly property color dangerHover: _c("dangerHover")
    readonly property color warning: _c("warning")
    readonly property color warningBg: _c("warningBg")
    readonly property color warningBorder: _c("warningBorder")
    readonly property color warningHover: _c("warningHover")
    readonly property color success: _c("success")
    readonly property color successBg: _c("successBg")
    readonly property color successBorder: _c("successBorder")
    readonly property color successHover: _c("successHover")
    readonly property color borderSubtle: _c("borderSubtle")
    readonly property color borderMuted: _c("borderMuted")
    readonly property color borderStrong: _c("borderStrong")
    readonly property color focusRing: _c("focusRing")
    readonly property color favorite: _c("favorite")
    readonly property color cardShadow: _c("cardShadow")
    // 24 alpha tiers ([4..95]%, white-based dark / black-based light).
    readonly property var alphaTiers: _doc.alpha === undefined ? [] : _doc.alpha
    // The ramp's percents (theme_qt.rs ALPHA_PCTS) — the index table for
    // alphaTier(), so a caller asks for the percentage the design specifies
    // ("--alpha-6") instead of a magic array index.
    readonly property var alphaPcts: [4, 5, 6, 8, 10, 12, 15, 18, 20, 25, 30, 35,
        40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95]
    /// One alpha tier by percentage. Theme-correct in both polarities (the
    /// ramp is white-based on dark themes, black-based on light).
    function alphaTier(pct) {
        var i = alphaPcts.indexOf(pct)
        return (i >= 0 && i < alphaTiers.length) ? alphaTiers[i] : "transparent"
    }
    // Ambient-background layering (phase 14; theme-derived since phase 19):
    // chrome surface-card @50%, frosted content panel surface-main @22%,
    // thin bars surface-main @30%, hairline = alpha tier 10%. The live
    // values for the QBZ_BG_* env knobs still ride QbzBridge
    // .ambientSurfaceAlpha / .ambientBarAlpha.
    readonly property color surfaceCardA50: _c("surfaceCardA50")
    readonly property color surfaceMainA22: _c("surfaceMainA22")
    readonly property color surfaceMainA30: _c("surfaceMainA30")
    readonly property color frostBorder: _c("frostBorder")

    // --- Spacing (spacing.slint) --------------------------------------
    readonly property int spacingXs: 4
    readonly property int spacingSm: 8
    readonly property int spacingMd: 16
    readonly property int spacingLg: 20
    readonly property int spacingXl: 32
    readonly property int cardPadding: 52

    // --- Radius (radius.slint) ----------------------------------------
    readonly property int radiusSm: 8
    readonly property int radiusMd: 12
    readonly property int radiusLg: 16

    // --- Typography (typography.slint, boost = 1.0) -------------------
    readonly property int fontLegal: 13
    readonly property int fontLink: 14
    readonly property int fontBody: 15
    readonly property int fontSubtitle: 15
    readonly property int fontButton: 17
    readonly property int fontSection: 18
    readonly property int fontHeading: 19
    readonly property int fontTitle: 25
    readonly property int fontWordmark: 29
    readonly property int weightRegular: Font.Normal
    readonly property int weightMedium: Font.Medium
    readonly property int weightSemibold: Font.DemiBold
    readonly property int weightBold: Font.Bold

    // --- Shell layout (layout.slint + state.slint) --------------------
    readonly property int headerHeight: 42
    // Three-state sidebar rendered widths (ShellState.sidebar-rendered-width).
    readonly property int sidebarOpenWidth: 240
    // Mini rail — DELIBERATE deviation from state.slint:4075 (64px), owner
    // request: the 64px rail read as a wide empty gutter around a 32px row.
    // rail = spacingSm + row + spacingSm, so the square mini row exactly
    // fills the 8px-padded track (Sidebar.qml root padding = Sidebar.slint:
    // 716-718). REVERT is TWO tokens, not one: putting 64 back here also
    // makes sidebarMiniRow 48, so the rows would grow from 34 to 48 and stop
    // matching Slint — set sidebarMiniRow to a literal 34 at the same time.
    // (Adversarial review caught this comment claiming a one-token revert.)
    readonly property int sidebarMiniWidth: 50
    // The square row inside that rail (34px). Rows are `width: parent.width`,
    // so this is only their HEIGHT; it exists as a token so the rail and the
    // row can never drift apart.
    readonly property int sidebarMiniRow: sidebarMiniWidth - 2 * spacingSm
    // Right queue/lyrics column width (AppShell).
    readonly property int queuePanelWidth: 300
    // Small now-playing bar total height (ShellState.npb-small-height,
    // npb-small-extra is 0 on Linux).
    readonly property int npbSmallHeight: 42
    // Every other bar mode (New / Classic / Large) — AppShell.slint:396.
    // The Large dock's sidebar reservation subtracts this, so it must be the
    // SAME value the shell pins the bar to.
    readonly property int npbLargeHeight: 112
}
