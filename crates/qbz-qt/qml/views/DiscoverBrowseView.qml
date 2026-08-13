// Discover "View all" full-list page — QML port of
// discover/DiscoverBrowseView.slint. One /discover/<module> endpoint (New
// Releases, Press Accolades, Ideal Discography, Qobuzissimes, Albums of the
// Week, Most Streamed) as a paginated full grid or list.
//
// Layout, read off the .slint:
//   :44  fixed 56px header — title LEFT, search / genre / grid-list RIGHT
//   :139 body padding 32 / 32, top 8, bottom 100
//   :26  200x266 cards, gap 24; :29 64px list rows, gap 4
//   :126 infinite scroll: within 600px of the bottom, has-more, not already
//        loading, and NO active search (the filter is client-side, so paging
//        under one would bring back unfiltered rows)
//   :205 scrollbar below the header; :233 genre popup anchored y=56
//
// NAV BUTTONS: the .slint draws a NavButtons pair at x=32 and starts the
// title 16px after it. In this port back/forward live in the shell header
// (HeaderBar.qml:235), so the title starts at x=48 — the 32 + 0 + 16 the
// HomeView toolbar already uses (HomeView.qml:744).
//
// POC-NOTEs: no scroll restore (the port has no counterpart to
// NavState.report-scroll / restore-scope); `windowed` grid mounting is the
// AlbumCollection sampler rather than the .slint's own.

import QtQuick
import QtQuick.Window
import com.blitzfc.qbz
import "../controls"
import "../theme"

Rectangle {
    id: root

    color: ambientOn ? "transparent" : theme.surfaceMain
    readonly property bool ambientOn: theme.ambientOn
    radius: 12

    QbzTheme { id: theme }

    readonly property var doc: {
        try {
            return JSON.parse(QbzHome.discoverBrowseJson)
        } catch (e) {
            return {}
        }
    }
    readonly property var items: doc.items || []
    readonly property string query: doc.query || ""
    // Seeded to "grid" by the Rust opener — an empty mode renders nothing.
    readonly property string viewMode: doc.viewMode || "grid"

    // --- Shared genre selection ------------------------------------------
    // `genre_filter_qt::after_selection_change` only knows how to reload
    // Home, so this page watches the published selection itself and asks for
    // a re-fetch from offset 0. The signature is the selected NAMES, not the
    // count: swapping one genre for another leaves the count unchanged.
    readonly property string genreSig: {
        try {
            var d = JSON.parse(QbzBridge.genreFilterJson)
            return JSON.stringify((d.names || {})["discover"] || [])
        } catch (e) {
            return ""
        }
    }
    property string lastGenreSig: ""
    onGenreSigChanged: {
        if (root.lastGenreSig !== "" && root.lastGenreSig !== root.genreSig)
            QbzHome.discoverBrowseGenreChanged()
        root.lastGenreSig = root.genreSig
    }
    Component.onCompleted: root.lastGenreSig = root.genreSig

    // ============================ offline gate ============================
    QbzOfflinePlaceholder {
        visible: QbzSession.offline
        anchors.centerIn: parent
        showSettingsAction: true
        onSettingsClicked: QbzShell.navigateTo("settings")
    }

    Column {
        anchors.fill: parent
        spacing: 0
        visible: !QbzSession.offline

        // --- Fixed 56px header -------------------------------------------
        Item {
            id: header
            width: parent.width
            height: 56

            Rectangle {
                anchors.fill: parent
                color: root.ambientOn ? theme.surfaceMainA30 : theme.surfaceMain
            }

            Text {
                x: 48
                y: 25 - height / 2
                width: Math.max(0, tools.x - 48 - 16)
                text: root.doc.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontSection
                font.weight: theme.weightBold
                elide: Text.ElideRight
            }

            Row {
                id: tools
                x: parent.width - width - 32
                y: 25 - height / 2
                spacing: 8

                QbzLineEdit {
                    searchMode: true
                    width: 200
                    placeholder: QbzSession.tr("Search…", QbzSession.trRev)
                    text: root.query
                    onEdited: function (v) { QbzHome.discoverBrowseSearch(v) }
                }
                BrowseGenreButton {
                    context: "discover"
                    onClicked: genrePopup.toggle()
                }
                ViewModeToggle {
                    mode: root.viewMode
                    onSetMode: function (m) { QbzHome.discoverBrowseSetViewMode(m) }
                }
            }
        }

        // --- Scrolling list ----------------------------------------------
        Item {
            width: parent.width
            height: parent.height - 56

            Flickable {
                id: flick
                anchors.fill: parent
                clip: true
                contentWidth: width
                contentHeight: page.implicitHeight
                boundsBehavior: Flickable.StopAtBounds

                // Infinite scroll, guarded exactly as the .slint is.
                onContentYChanged: {
                    if (contentY + height >= contentHeight - 600
                        && root.doc.hasMore === true
                        && !QbzHome.discoverBrowseLoadingMore
                        && !QbzHome.discoverBrowseLoading
                        && root.query === "")
                        QbzHome.discoverBrowseLoadMore()
                }

                Column {
                    id: page
                    width: parent.width
                    leftPadding: 32
                    rightPadding: 32
                    topPadding: 8
                    bottomPadding: 100
                    spacing: 0

                    QbzSpinner {
                        visible: QbzHome.discoverBrowseLoading
                        size: 36
                        anchors.horizontalCenter: parent.horizontalCenter
                    }

                    // Search filtered everything out.
                    Text {
                        visible: !QbzHome.discoverBrowseLoading
                            && root.items.length === 0 && root.query !== ""
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: QbzSession.tr("No results found.", QbzSession.trRev)
                        color: theme.textMuted
                        font.pixelSize: 14
                    }

                    AlbumCollection {
                        visible: !QbzHome.discoverBrowseLoading
                        width: parent.width - 64
                        albums: root.items
                        viewMode: root.viewMode
                        cardWidth: 200
                        cardHeight: 266
                        cardGap: 24
                        listRowGap: 4
                        flick: flick
                        contentOffset: 8
                    }

                    Item { visible: QbzHome.discoverBrowseLoadingMore; width: 1; height: 24 }
                    QbzSpinner {
                        visible: QbzHome.discoverBrowseLoadingMore
                        size: 28
                        anchors.horizontalCenter: parent.horizontalCenter
                    }
                }
            }

            // Tracks only the scrolling list (the header is a sibling above).
            QbzScrollBar {
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.top: parent.top
                anchors.bottom: parent.bottom
                target: flick
            }
        }
    }

    // Declared LAST — declaration order IS z-order, so the popup keeps its
    // own presses instead of the toolbar swallowing them.
    GenreFilterPopup {
        id: genrePopup
        anchors.fill: parent
        context: "discover"
        // The .slint anchors this page's popup at y=56 (not HomeView's 52).
        anchorTop: 56
        anchorRight: 32
    }
}
