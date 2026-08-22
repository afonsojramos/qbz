// FontPreload — registers the bundled typefaces BEFORE any Text exists.
//
// WHY THIS FILE IS A SEPARATE DOCUMENT, loaded before Main.qml.
//
// A plain `Text` does NOT inherit a font from its parents, and it does NOT
// follow `ApplicationWindow.font` — that property propagates through Qt Quick
// CONTROLS only. Proven on this Qt build with a standalone scene: with a
// family set on the ApplicationWindow, a `Label` resolved to it and a `Text`
// sibling stayed on the APPLICATION font. This tree has 1082 plain `Text`
// items and 13 that name a family, so "the app's typeface" IS the application
// font, and nothing else.
//
// And the application font is only read when an item is CONSTRUCTED. Also
// proven, with PySide6 against this same Qt: calling QGuiApplication::setFont
// at runtime updated `QGuiApplication::font()` and left an already-built
// `Text` on its old family. So the font has to be set before the UI is built,
// which means the families have to be REGISTERED before that — and a family
// is only registered once something loads its file.
//
// Hence this document: `QQmlApplicationEngine::load` returns with its
// FontLoaders already in the font database (verified: the family is present
// the instant load() returns), so main.rs can set the application font in the
// gap between this document and Main.qml.
//
// It is deliberately NON-VISUAL — a QtObject, not a Window — so loading it
// adds a root object and nothing else.
//
// The five faces are settings_qt::APP_FONT_VALUES, in that order. Inter is
// here too: it is the app's own face, named by Main.qml for the Controls, and
// it needs registering for exactly the same reason.

import QtQuick

QtObject {
    readonly property FontLoader inter: FontLoader { source: "assets/fonts/Inter_18pt-Regular.ttf" }
    readonly property FontLoader interMedium: FontLoader { source: "assets/fonts/Inter_18pt-Medium.ttf" }
    readonly property FontLoader interSemiBold: FontLoader { source: "assets/fonts/Inter_18pt-SemiBold.ttf" }
    readonly property FontLoader interBold: FontLoader { source: "assets/fonts/Inter_18pt-Bold.ttf" }
    readonly property FontLoader lineSeed: FontLoader { source: "assets/fonts/LINESeedJP-Regular.ttf" }
    readonly property FontLoader montserrat: FontLoader { source: "assets/fonts/Montserrat-VariableFont_wght.ttf" }
    readonly property FontLoader notoSans: FontLoader { source: "assets/fonts/NotoSans-VariableFont_wdth,wght.ttf" }
    readonly property FontLoader sourceSans3: FontLoader { source: "assets/fonts/SourceSans3-VariableFont_wght.ttf" }
}
