// Shared dark-theme constants for the POC — the values the Slint app
// pushes into its ThemeColors struct for the dark theme
// (crates/qbz-ui/ui/foundation/theme.slint, dark block), plus the spacing /
// radius / typography tokens the login screen uses.
//
// Kept as a plain instantiable QtObject (NOT a pragma Singleton — the
// cxx-qt-build generated qmldir cannot mark singletons).

import QtQuick

QtObject {
    readonly property color surfaceMain: "#0f0f0f"
    readonly property color surfaceCard: "#1a1a1a"
    readonly property color surfaceElevated: "#2a2a2a"
    readonly property color textPrimary: "#ffffff"
    readonly property color textSecondary: "#cccccc"
    readonly property color textMuted: "#888888"
    readonly property color accent: "#4285f4"
    // #ffffff14 / #00000066 in the Slint ARGB notation.
    readonly property color borderSubtle: "#14ffffff"
    readonly property color cardShadow: "#66000000"

    readonly property int spacingSm: 8
    readonly property int spacingMd: 16
    readonly property int spacingLg: 20
    readonly property int spacingXl: 32
    readonly property int cardPadding: 52

    readonly property int radiusSm: 8
    readonly property int radiusLg: 16

    readonly property int fontLegal: 13
    readonly property int fontBody: 15
    readonly property int fontSubtitle: 15
    readonly property int fontWordmark: 29
    readonly property int weightMedium: Font.Medium
    readonly property int weightSemibold: Font.DemiBold
}
