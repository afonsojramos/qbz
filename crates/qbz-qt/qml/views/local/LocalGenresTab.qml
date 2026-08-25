// Local Library Genres — a retractable three-stage browser over the logical
// album set, surrounding grid/list/expanded-details results from any edge.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Item {
    id: root

    property var view: null
    QbzTheme { id: theme }

    readonly property string filterPosition: view ? view.genreFiltersPosition : "top"
    readonly property bool verticalFilters: filterPosition === "left" || filterPosition === "right"
    readonly property int sideWidth: 300   // ArtistView's Network sidebar width.
    readonly property int collapsedThickness: 24
    readonly property int horizontalHeight: 266

    function facetTitle(index) {
        if (index === 0) return QbzSession.tr("Genres", QbzSession.trRev)
        if (index === 1) return QbzSession.tr("Artists", QbzSession.trRev)
        return QbzSession.tr("Albums", QbzSession.trRev)
    }
    function facetAllLabel(index) {
        if (index === 0) return QbzSession.tr("All genres", QbzSession.trRev)
        if (index === 1) return QbzSession.tr("All artists", QbzSession.trRev)
        return QbzSession.tr("All albums", QbzSession.trRev)
    }
    function facetQuery(index) {
        if (index === 0) return root.view.genresSearch
        if (index === 1) return root.view.genreArtistsSearch
        return root.view.genreAlbumsSearch
    }
    function facetOptions(index) {
        if (index === 0) return root.view.genreNames
        if (index === 1) return root.view.genreArtistOptions
        return root.view.genreAlbumOptions
    }
    function facetSelected(index) {
        if (index === 0) return root.view.selectedGenres
        if (index === 1) return root.view.selectedGenreArtists
        return root.view.selectedGenreAlbums
    }
    function setFacetQuery(index, value) {
        if (index === 0) root.view.genresSearch = value
        else if (index === 1) root.view.genreArtistsSearch = value
        else root.view.genreAlbumsSearch = value
    }
    function toggleFacet(index, key, modifiers) {
        if (index === 0) root.view.toggleGenre(key, modifiers)
        else if (index === 1) root.view.toggleGenreArtist(key, modifiers)
        else {
            // The details Loader already exists while the third column is
            // visible. Ask it immediately instead of waiting for the filtered
            // ListView to rebuild and mount its replacement delegate.
            if (key !== "" && root.view.genresView === "details"
                    && resultsLoader.item && resultsLoader.item.ensure)
                resultsLoader.item.ensure(key)
            root.view.toggleGenreAlbum(key, modifiers)
        }
    }
    function facetSelectionLabel(index, key) {
        var options = root.facetOptions(index)
        for (var i = 0; i < options.length; i++)
            if (options[i].key === key) return options[i].label
        // A query can hide a selected option from its own column. Resolve
        // albums against the unfiltered logical set; genre/artist keys are
        // already readable names, merely normalized to lower case.
        if (index === 2) {
            for (var j = 0; j < root.view.albums.length; j++)
                if (root.view.albums[j].id === key)
                    return root.view.albums[j].title || key
        }
        return key
    }
    function buildActiveFilterChips() {
        if (!root.view) return []
        var out = []
        for (var facet = 0; facet < 3; facet++) {
            var query = root.facetQuery(facet).trim()
            if (query !== "")
                out.push({ kind: "query", facet: facet,
                           label: root.facetTitle(facet) + ": " + query })
            var selected = root.facetSelected(facet)
            var keys = Object.keys(selected)
            for (var i = 0; i < keys.length; i++) {
                if (selected[keys[i]] !== true) continue
                out.push({ kind: "selection", facet: facet, key: keys[i],
                           label: root.facetTitle(facet) + ": "
                               + root.facetSelectionLabel(facet, keys[i]) })
            }
        }

        var tr = QbzSession.trRev
        var filters = [
            { key: "favorite", label: QbzSession.tr("Favorites only", tr) },
            { key: "hires", label: QbzSession.tr("Hi-Res", tr) },
            { key: "cd", label: QbzSession.tr("CD", tr) },
            { key: "lossy", label: QbzSession.tr("Lossy", tr) },
            { key: "flac", label: "FLAC" }, { key: "alac", label: "ALAC" },
            { key: "ape", label: "APE" }, { key: "wav", label: "WAV" },
            { key: "mp3", label: "MP3" }, { key: "aac", label: "AAC" },
            { key: "other", label: QbzSession.tr("Other", tr) },
            { key: "local", label: QbzSession.tr("Local", tr) },
            { key: "offline", label: QbzSession.tr("Offline cache", tr) },
            { key: "plex", label: "Plex" }, { key: "jellyfin", label: "Jellyfin" },
            { key: "subsonic", label: "Subsonic" }
        ]
        for (i = 0; i < filters.length; i++)
            if (root.view.filter[filters[i].key] === true)
                out.push({ kind: "filter", key: filters[i].key,
                           label: filters[i].label })
        return out
    }
    function clearFilterChip(chip) {
        if (chip.kind === "query") root.setFacetQuery(chip.facet, "")
        else if (chip.kind === "selection")
            root.toggleFacet(chip.facet, chip.key, Qt.ControlModifier)
        else root.view.toggleFilter(chip.key)
    }
    function clearAllActiveFilters() {
        for (var i = 0; i < 3; i++) root.setFacetQuery(i, "")
        root.view.selectedGenres = ({})
        root.view.selectedGenreArtists = ({})
        root.view.selectedGenreAlbums = ({})
        root.view.clearFilter()
    }
    readonly property var activeFilterChips: buildActiveFilterChips()
    function caretName() {
        var collapsed = root.view && root.view.genresBrowserCollapsed
        if (filterPosition === "bottom") return collapsed ? "chevron-up" : "chevron-down"
        if (filterPosition === "left") return collapsed ? "chevron-right" : "chevron-left"
        if (filterPosition === "right") return collapsed ? "chevron-left" : "chevron-right"
        return collapsed ? "chevron-down" : "chevron-up"
    }

    component GenreFacet: LocalGenreColumn {
        required property int facetIndex
        title: root.facetTitle(facetIndex)
        allLabel: root.facetAllLabel(facetIndex)
        query: root.facetQuery(facetIndex)
        options: root.facetOptions(facetIndex)
        selected: root.facetSelected(facetIndex)
        // A title query can temporarily match hundreds of expanded albums.
        // Publishing after each fast keystroke rebuilds that result tree on
        // the GUI thread and makes the next keystroke feel stuck. The edit
        // itself remains immediate; only the derived result waits for the
        // short typing pause.
        debounceMs: facetIndex === 2 ? 140 : 90
        onQueryEdited: function(value) { root.setFacetQuery(facetIndex, value) }
        onToggled: function(key, modifiers) { root.toggleFacet(facetIndex, key, modifiers) }
    }

    Component {
        id: horizontalBrowser
        Row {
            spacing: 8
            Repeater {
                model: 3
                delegate: GenreFacet {
                    required property int index
                    facetIndex: index
                    width: (parent.width - 16) / 3
                    height: parent.height
                }
            }
        }
    }

    Component {
        id: verticalBrowser
        Column {
            spacing: 8
            Repeater {
                model: 3
                delegate: GenreFacet {
                    required property int index
                    facetIndex: index
                    width: parent.width
                    height: (parent.height - 16) / 3
                }
            }
        }
    }

    Item {
        id: browser
        readonly property bool collapsed: root.view && root.view.genresBrowserCollapsed
        x: root.verticalFilters
            ? (root.filterPosition === "left" ? 8 : root.width - width - 8)
            : 32
        y: root.verticalFilters
            ? 10
            : (root.filterPosition === "bottom" ? root.height - height - 10 : 10)
        width: root.verticalFilters
            ? (collapsed ? root.collapsedThickness : root.sideWidth)
            : Math.max(0, root.width - 64)
        height: root.verticalFilters
            ? Math.max(0, root.height - 20)
            : (collapsed ? root.collapsedThickness : root.horizontalHeight)

        Loader {
            visible: !browser.collapsed
            x: root.verticalFilters && root.filterPosition === "right"
                ? root.collapsedThickness : 0
            y: !root.verticalFilters && root.filterPosition === "bottom"
                ? root.collapsedThickness : 0
            width: root.verticalFilters
                ? Math.max(0, parent.width - root.collapsedThickness)
                : parent.width
            height: root.verticalFilters
                ? parent.height
                : Math.max(0, parent.height - root.collapsedThickness)
            sourceComponent: root.verticalFilters ? verticalBrowser : horizontalBrowser
        }

        Rectangle {
            x: root.verticalFilters
                ? (root.filterPosition === "left" ? parent.width - width : 0)
                : (parent.width - width) / 2
            y: root.verticalFilters
                ? (parent.height - height) / 2
                : (root.filterPosition === "bottom" ? 0 : parent.height - height)
            width: root.verticalFilters ? 20 : 42
            height: root.verticalFilters ? 42 : 20
            radius: 5
            color: theme.ambientOn
                ? theme.surfaceElevatedA50
                : (caretArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated)
            border.width: 1
            border.color: theme.ambientOn ? theme.frostBorder : theme.borderSubtle
            QbzIcon {
                anchors.centerIn: parent
                name: root.caretName()
                width: 13
                height: 13
                tintName: "secondary"
            }
            MouseArea {
                id: caretArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: root.view.genresBrowserCollapsed = !root.view.genresBrowserCollapsed
            }
        }
    }

    Item {
        id: results
        // Keep already-matching rows during refresh, but never leave an empty
        // filtered viewport blank while the album projection is rebuilding.
        readonly property bool initialLoading: QbzLocal.localAlbumsLoading
            && root.view.genreAlbumsVisible.length === 0
        x: root.verticalFilters && root.filterPosition === "left"
            ? browser.x + browser.width + 8 : 32
        y: !root.verticalFilters && root.filterPosition === "top"
            ? browser.y + browser.height + 8 : 10
        width: root.verticalFilters
            ? (root.filterPosition === "left"
                ? Math.max(0, root.width - x - 32)
                : Math.max(0, browser.x - x - 8))
            : Math.max(0, root.width - 64)
        height: root.verticalFilters
            ? Math.max(0, root.height - 20)
            : (root.filterPosition === "bottom"
                ? Math.max(0, browser.y - y - 8)
                : Math.max(0, root.height - y - 10))

        QbzSkeleton {
            visible: results.initialLoading && root.view.genresView === "grid"
            anchors.fill: parent
            variant: "cardGrid"
            cellW: 220
            cellH: 266
            phase: root.view.skelPhase
        }
        QbzSkeleton {
            visible: results.initialLoading && root.view.genresView !== "grid"
            anchors.fill: parent
            variant: "rowList"
            rowH: root.view.genresView === "details" ? 72 : 56
            rowGap: 0
            rowArtSize: root.view.genresView === "details" ? 48 : 40
            phase: root.view.skelPhase
        }

        Column {
            id: emptyPanel
            visible: !results.initialLoading
                && root.view.genreAlbumsVisible.length === 0
            anchors.centerIn: parent
            width: Math.min(Math.max(0, parent.width - 64), 720)
            spacing: 14

            QbzEmptyState {
                width: parent.width
                iconName: "list-filter"
                title: QbzSession.tr("No albums match these filters", QbzSession.trRev)
                body: QbzSession.tr("Try a different genre, artist, album, source or quality.",
                                    QbzSession.trRev)
            }

            Flow {
                visible: root.activeFilterChips.length > 0
                width: parent.width
                height: childrenRect.height
                spacing: 8

                Repeater {
                    model: root.activeFilterChips
                    delegate: Rectangle {
                        required property var modelData
                        width: chipRow.implicitWidth + 18
                        height: 28
                        radius: 6
                        color: chipArea.containsMouse
                            ? theme.surfaceHover
                            : (theme.ambientOn ? theme.surfaceElevatedA50
                                               : theme.surfaceElevated)
                        border.width: 1
                        border.color: theme.ambientOn ? theme.frostBorder : theme.borderSubtle

                        Row {
                            id: chipRow
                            anchors.centerIn: parent
                            spacing: 7
                            Text {
                                text: parent.parent.modelData.label
                                color: theme.textPrimary
                                font.pixelSize: theme.fontLegal
                                elide: Text.ElideRight
                            }
                            QbzIcon {
                                name: "x"
                                width: 11
                                height: 11
                                anchors.verticalCenter: parent.verticalCenter
                                tintName: chipArea.containsMouse ? "textPrimary" : "muted"
                            }
                        }
                        MouseArea {
                            id: chipArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.clearFilterChip(parent.modelData)
                        }
                    }
                }
            }

            SettingsButton {
                visible: root.activeFilterChips.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                text: QbzSession.tr("Clear", QbzSession.trRev)
                minWidth: 0
                btnHeight: 30
                onClicked: root.clearAllActiveFilters()
            }
        }

        Loader {
            id: resultsLoader
            anchors.fill: parent
            active: !results.initialLoading && root.view.genreAlbumsVisible.length > 0
            sourceComponent: root.view.genresView === "details" ? detailsComponent : collectionComponent
        }
    }

    Component {
        id: collectionComponent
        LocalAlbumCollection {
            view: root.view
            surface: "genres"
            scrollScope: "local:genres"
            rows: root.view.genreAlbumsVisible
            groups: []
            grouped: false
            viewMode: root.view.genresView
            showSource: true
            onOpenRequested: function(id) { root.view.openAlbum(id) }
            onPlayRequested: function(id) {
                QbzLocal.playAlbumFiltered(id, JSON.stringify(root.view.genresFilter || {}), false)
            }
            onEnqueueRequested: function(id, mode) {
                QbzLocal.enqueueAlbumFiltered(id, JSON.stringify(root.view.genresFilter || {}), mode)
            }
        }
    }

    Component {
        id: detailsComponent
        LocalGenreDetails {
            view: root.view
            albums: root.view.genreAlbumsVisible
        }
    }
}
