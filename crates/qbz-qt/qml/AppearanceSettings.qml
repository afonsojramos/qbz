// Settings > Appearance (phase 19) — the QML port of
// crates/qbz-ui/ui/settings/AppearanceSettings.slint. Row order, group
// headers and gating mirror the Slint 1:1; every control rides the single
// settingsJson document (root.doc) and the settingsBool/Select/String
// invokables — never local truth.
//
// The THEME row is live: QbzBridge.themeSlug / themeListJson / themeFilter
// (theme_qt.rs) and themeSet() repaints the whole app through QbzTheme.qml
// (the Slint theme::push_colors equivalent).
//
// POC-NOTEs (deliberate cuts):
// - "Auto (dynamic)" rows: the Source dropdown persists, Regenerate
//   re-resolves via AutoSource::System; the "Select Image..." flow
//   (file dialog + AutoSource::Image) is not ported.
// - "Custom" rows: the Slint token editor is not ported (owner: read-only
//   apply of custom_theme.json) — an info row stands in.
// - The "system" registry theme maps to the Dark palette (Slint reads the
//   OS palette).
// - Language persists but applies on next launch (live switch = phase 20).
// - UI scale / renderer / preferred GPU persist with restart semantics;
//   the Slint restart toasts are logs here.
// - Preferred GPU: no adapter enumeration — Auto + the persisted adapter.
// - The commented-out Slint blocks (immersive background/panels, title
//   template) stay out, 1:1 the owner's cut.

import QtQuick
import com.blitzfc.qbz

Column {
    id: root

    property var doc: ({})

    QbzTheme { id: theme }

    spacing: 4

    // --- Theme row state (theme_qt.rs via the bridge) --------------------
    readonly property var themeEntries: {
        try {
            return JSON.parse(QbzBridge.themeListJson)
        } catch (e) {
            return []
        }
    }
    // 0 All / 1 Dark / 2 Light; the Auto/Custom synthetics only list under
    // All (theme.rs:217-227).
    readonly property var filteredThemes: {
        const f = QbzBridge.themeFilter
        const out = []
        for (let i = 0; i < themeEntries.length; i++) {
            const e = themeEntries[i]
            const synthetic = e.slug === "auto" || e.slug === "custom"
            if (f === 1 && (e.isLight || synthetic)) continue
            if (f === 2 && (!e.isLight || synthetic)) continue
            out.push(e)
        }
        return out
    }
    readonly property var filteredLabels: filteredThemes.map(function (e) { return e.label })
    readonly property int currentThemeIndex: {
        for (let i = 0; i < filteredThemes.length; i++) {
            if (filteredThemes[i].slug === QbzBridge.themeSlug) return i
        }
        return -1
    }
    readonly property bool tbLocked: doc.hideTitleBar === true || doc.useSystemTitleBar === true

    // ============================ THEME ==================================
    GroupHeader { text: QbzBridge.tr("THEME") }
    SettingRow {
        label: QbzBridge.tr("Theme")
        Row {
            spacing: 8
            // Filter cycle (All -> Dark -> Light): sun-moon / moon / sun.
            Rectangle {
                width: 34
                height: 34
                radius: theme.radiusSm
                border.width: 1
                border.color: theme.borderSubtle
                color: fArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated
                QbzIcon {
                    anchors.centerIn: parent
                    name: QbzBridge.themeFilter === 1 ? "moon"
                        : QbzBridge.themeFilter === 2 ? "sun" : "sun-moon"
                    width: 16
                    height: 16
                    tintName: "secondary"
                }
                MouseArea {
                    id: fArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: QbzBridge.themeSetFilter((QbzBridge.themeFilter + 1) % 3)
                }
            }
            QbzSelect {
                menuWidth: 220
                searchable: true
                options: root.filteredLabels
                currentIndex: root.currentThemeIndex
                onSelected: function (i) {
                    if (i >= 0 && i < root.filteredThemes.length)
                        QbzBridge.themeSet(root.filteredThemes[i].slug)
                }
            }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Album header gradient")
        description: QbzBridge.tr("Use artwork-derived blur as a backdrop in album and artist detail views.")
        QbzToggle {
            checked: root.doc.albumHeaderGradient === true
            onToggled: function (v) { QbzBridge.settingsBool("album-header-gradient", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Dynamic background")
        QbzSelect {
            menuWidth: 200
            options: root.doc.appBackgroundModes || []
            currentIndex: root.doc.appBackgroundIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("app-background", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Intelligent Search")
        QbzToggle {
            checked: root.doc.intelligentSearch === true
            onToggled: function (v) { QbzBridge.settingsBool("intelligent-search", v) }
        }
    }
    // Auto-theme rows (theme slug "auto" only).
    SettingRow {
        visible: QbzBridge.themeSlug === "auto"
        label: QbzBridge.tr("Source")
        QbzSelect {
            menuWidth: 200
            options: [QbzBridge.tr("System Colors"), QbzBridge.tr("Wallpaper Sync"), QbzBridge.tr("Custom Image")]
            currentIndex: root.doc.autoThemeSourceIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("auto-theme-source", i) }
        }
    }
    SettingRow {
        visible: QbzBridge.themeSlug === "auto"
        label: QbzBridge.tr("Regenerate")
        description: QbzBridge.tr("Rebuild the palette from the source (System colors in the POC).")
        SettingsButton {
            text: QbzBridge.tr("Regenerate")
            onClicked: QbzBridge.themeSet(QbzBridge.themeSlug)
        }
    }
    // Custom-theme rows (theme slug "custom" only) — no token editor.
    SettingRow {
        visible: QbzBridge.themeSlug === "custom"
        label: QbzBridge.tr("Custom theme")
        description: QbzBridge.tr("Applied read-only from custom_theme.json — the token editor is not ported to the Qt frontend (POC).")
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ===================== TYPOGRAPHY & LANGUAGE =========================
    GroupHeader { text: QbzBridge.tr("TYPOGRAPHY & LANGUAGE") }
    SettingRow {
        label: QbzBridge.tr("Language")
        QbzSelect {
            menuWidth: 200
            options: root.doc.languages || []
            currentIndex: root.doc.languageIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("language", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Interface size")
        description: QbzBridge.tr("Restart to apply.")
        QbzSelect {
            menuWidth: 200
            options: root.doc.uiScales || []
            currentIndex: root.doc.uiScaleIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("ui-scale", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Immersive search")
        QbzSelect {
            menuWidth: 220
            options: root.doc.immersiveSearchActions || []
            currentIndex: root.doc.immersiveSearchActionIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("immersive-search-action", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Immersive default view")
        QbzSelect {
            menuWidth: 220
            options: root.doc.immersiveDefaultViews || []
            currentIndex: root.doc.immersiveDefaultViewIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("immersive-default-view", i) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ======================= LIBRARY & VISUALS ===========================
    GroupHeader { text: QbzBridge.tr("LIBRARY & VISUALS") }
    SettingRow {
        label: QbzBridge.tr("Show navigation in sidebar")
        QbzToggle {
            checked: root.doc.navInSidebar === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-in-sidebar", v) }
        }
    }
    SettingRow {
        // ADR-010: only mounted when navigation is NOT in the sidebar.
        visible: root.doc.navInSidebar !== true
        label: QbzBridge.tr("Compact header navigation")
        QbzToggle {
            checked: root.doc.navHeaderCompact === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-header-compact", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("My QBZ")
        Row {
            spacing: 8
            RoundedImage {
                width: 34
                height: 34
                source: "assets/qbz-logo.png"
                radius: theme.radiusSm
            }
            QbzLineEdit {
                width: 150
                text: root.doc.myQbzLabel || ""
                placeholder: QbzBridge.tr("My QBZ")
                onCommitted: function (v) { QbzBridge.settingsString("myqbz-label", v) }
            }
            SettingsButton {
                iconName: "rotate-ccw"
                onClicked: QbzBridge.settingsString("myqbz-label", "")
            }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Show playlist cover collages in sidebar")
        QbzToggle {
            checked: root.doc.sidebarPlaylistCollage === true
            onToggled: function (v) { QbzBridge.settingsBool("sidebar-playlist-collage", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Show album artwork in Local Library track list")
        QbzToggle {
            checked: root.doc.localLibraryTrackArtwork === true
            onToggled: function (v) { QbzBridge.settingsBool("local-library-track-artwork", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Play indicator animation")
        QbzToggle {
            checked: root.doc.playIndicatorAnimation === true
            onToggled: function (v) { QbzBridge.settingsBool("play-indicator-animation", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Invert swipe navigation direction")
        QbzToggle {
            checked: root.doc.invertSwipeNavigation === true
            onToggled: function (v) { QbzBridge.settingsBool("invert-swipe-navigation", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================= NOTIFICATIONS =============================
    GroupHeader { text: QbzBridge.tr("NOTIFICATIONS") }
    SettingRow {
        label: QbzBridge.tr("In-app toasts notifications")
        QbzToggle {
            checked: root.doc.inAppToasts === true
            onToggled: function (v) { QbzBridge.settingsBool("in-app-toasts", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("System Notifications")
        QbzToggle {
            checked: root.doc.systemNotifications === true
            onToggled: function (v) { QbzBridge.settingsBool("system-notifications", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================= WINDOW TITLE ==============================
    GroupHeader { text: QbzBridge.tr("WINDOW TITLE") }
    SettingRow {
        label: QbzBridge.tr("Show track in window title")
        QbzToggle {
            checked: root.doc.windowTitleShow === true
            onToggled: function (v) { QbzBridge.settingsBool("window-title-show", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================== TITLE BAR ================================
    GroupHeader { text: QbzBridge.tr("TITLE BAR") }
    SettingRow {
        label: QbzBridge.tr("Use system title bar")
        description: QbzBridge.tr("Restart to apply.")
        QbzToggle {
            checked: root.doc.useSystemTitleBar === true
            onToggled: function (v) { QbzBridge.settingsBool("use-system-title-bar", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Hide title bar")
        rowEnabled: root.doc.useSystemTitleBar !== true
        QbzToggle {
            enabled: root.doc.useSystemTitleBar !== true
            checked: root.doc.hideTitleBar === true
            onToggled: function (v) { QbzBridge.settingsBool("hide-title-bar", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ======================= WINDOW CONTROLS =============================
    GroupHeader { text: QbzBridge.tr("WINDOW CONTROLS") }
    SettingRow {
        label: QbzBridge.tr("Window controls position")
        rowEnabled: !root.tbLocked
        QbzSelect {
            enabled: !root.tbLocked
            menuWidth: 160
            options: root.doc.wcPositions || []
            currentIndex: root.doc.wcPositionIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("wc-position", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Show window controls")
        rowEnabled: !root.tbLocked
        QbzToggle {
            enabled: !root.tbLocked
            checked: root.doc.showWindowControls === true
            onToggled: function (v) { QbzBridge.settingsBool("show-window-controls", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================= PLAYER VIEWS ==============================
    GroupHeader { text: QbzBridge.tr("PLAYER VIEWS") }
    SettingRow {
        label: QbzBridge.tr("Show volume +/- buttons")
        QbzToggle {
            checked: root.doc.showVolumeSteppers === true
            onToggled: function (v) { QbzBridge.settingsBool("show-volume-steppers", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Mini player default view")
        QbzSelect {
            menuWidth: 220
            options: root.doc.miniDefaultViews || []
            currentIndex: root.doc.miniDefaultViewIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("mini-default-view", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Startup page")
        QbzSelect {
            menuWidth: 220
            options: root.doc.startupPages || []
            currentIndex: root.doc.startupPageIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("startup-page", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Show Purchases")
        QbzToggle {
            checked: root.doc.showPurchases === true
            onToggled: function (v) { QbzBridge.settingsBool("show-purchases", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Purchases in title bar")
        rowEnabled: !root.tbLocked
        QbzToggle {
            enabled: !root.tbLocked
            checked: root.doc.navTbPurchases === true
            onToggled: function (v) { QbzBridge.settingsBool("nav-tb-purchases", v) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // ========================== SYSTEM TRAY ==============================
    GroupHeader { text: QbzBridge.tr("SYSTEM TRAY") }
    SettingRow {
        label: QbzBridge.tr("Enable tray icon")
        QbzToggle {
            checked: root.doc.trayEnable === true
            onToggled: function (v) { QbzBridge.settingsBool("tray-enable", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Close to tray")
        rowEnabled: root.doc.trayEnable === true
        QbzToggle {
            enabled: root.doc.trayEnable === true
            checked: root.doc.trayCloseToTray === true
            onToggled: function (v) { QbzBridge.settingsBool("tray-close-to-tray", v) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Tray icon variant")
        QbzSelect {
            menuWidth: 200
            options: root.doc.trayIconThemes || []
            currentIndex: root.doc.trayIconThemeIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("tray-icon-theme", i) }
        }
    }

    SettingsSpacer { }
    SettingsDivider { }
    SettingsSpacer { }

    // =========================== RENDERER ================================
    GroupHeader { text: QbzBridge.tr("RENDERER") }
    SettingRow {
        label: QbzBridge.tr("Rendering backend")
        description: QbzBridge.tr("Restart to apply.")
        QbzSelect {
            menuWidth: 220
            options: root.doc.renderers || []
            currentIndex: root.doc.rendererIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("renderer", i) }
        }
    }
    SettingRow {
        label: QbzBridge.tr("Preferred GPU")
        description: QbzBridge.tr("Restart to apply.")
        QbzSelect {
            menuWidth: 260
            options: root.doc.gpuPowers || []
            currentIndex: root.doc.gpuPowerIndex || 0
            onSelected: function (i) { QbzBridge.settingsSelect("gpu-power", i) }
        }
    }
}
