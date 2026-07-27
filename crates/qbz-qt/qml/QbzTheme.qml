// Shared dark-theme constants for the POC — the values the Slint app
// pushes into its ThemeColors struct for the dark theme
// (crates/qbz-ui/ui/foundation/theme.slint, dark block), plus the spacing /
// radius / typography / layout tokens the shell uses (foundation/*.slint).
//
// Kept as a plain instantiable QtObject (NOT a pragma Singleton — the
// cxx-qt-build generated qmldir cannot mark singletons).

import QtQuick

QtObject {
    // --- Theme colors (dark) ------------------------------------------
    readonly property color surfaceMain: "#0f0f0f"
    readonly property color surfaceCard: "#1a1a1a"
    readonly property color surfaceElevated: "#2a2a2a"
    readonly property color surfaceHover: "#10ffffff"
    readonly property color textPrimary: "#ffffff"
    readonly property color textSecondary: "#cccccc"
    readonly property color textMuted: "#888888"
    readonly property color textDisabled: "#555555"
    readonly property color accent: "#4285f4"
    readonly property color accentHover: "#5a9bf4"
    readonly property color accentPressed: "#3275e4"
    readonly property color accentText: "#ffffff"
    readonly property color warning: "#fbbf24"
    readonly property color favorite: "#ef4444"
    // #ffffff14 / #ffffff38 / #00000066 in the Slint ARGB notation.
    readonly property color borderSubtle: "#14ffffff"
    readonly property color borderMuted: "#38ffffff"
    readonly property color cardShadow: "#66000000"

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
    readonly property int sidebarMiniWidth: 64
    // Right queue/lyrics column width (AppShell).
    readonly property int queuePanelWidth: 300
    // Small now-playing bar total height (ShellState.npb-small-height,
    // npb-small-extra is 0 on Linux).
    readonly property int npbSmallHeight: 42
}
