// Per-source glyph — the QML port of `primitives/SourceGlyph.slint`, used by
// the version picker, the MyQBZ collection-detail rows and the Discography
// Builder rows.
//
// ── THE LOAD-BEARING RULE (SourceGlyph.slint:4-8) ─────────────────────────
// The full-colour Qobuz and Plex logos render **UNTINTED**; only the
// monochrome local hard-drive glyph is tinted, and to MUTED text, never
// accent. Slint states the reason out loud: routing the logos through a
// tinting primitive "would turn the logos into a flat accent silhouette — the
// bug this primitive replaces". `QbzIcon` floods every pixel with its tint, so
// the logo arms deliberately do NOT go through it — they are plain `Image`s.
//
// ── WHY THIS FILE USED TO LIE ─────────────────────────────────────────────
// The port shipped with neither logo: `qobuz-logo-filled.svg` and
// `plex-logo.svg` were never carried over from the Slint tree, so this
// component substituted `cloud-download` for every Qobuz kind and a tinted
// `hard-drive` for Plex — i.e. it drew "downloaded" and "local file" where the
// design calls for two brand marks. Both assets now live in
// `qml/assets/brand/`, which is where the untinted marks belong (the Discogs /
// Last.fm / MusicBrainz marks are already there for the same reason).
//
// `qobuz_purchase` reuses the Qobuz mark GOLD (#eab308) — same glyph as a
// download, the colour is what says "purchased". That is the ONE arm that
// needs a tint on a multi-colour asset, so it is the one arm that uses an
// effect; on a software renderer it degrades to the untinted mark rather than
// to nothing.

import QtQuick
import QtQuick.Effects
import com.blitzfc.qbz
import "../../theme"

Item {
    id: root

    /// "local" | "qobuz" | "qobuz_download" | "qobuz_purchase" | "offline" | "plex"
    /// (anything else = local). Plain "qobuz" is what the MyQBZ collection
    /// items carry; `SourceGlyph.slint:23` groups it with the downloads.
    property string kind: "local"

    /// 14 = `album/LocalAlbumView.slint:24` (the version picker, the default
    /// here because those are the call sites that set nothing).
    /// 15 = `primitives/SourceGlyph.slint:29-30` (every row glyph).
    /// SourceGlyph gives the three Qobuz kinds 16px to compensate the
    /// wordmark's proportions (`:29`) — now reproducible, because the wordmark
    /// is finally the asset being drawn.
    property int glyphSize: 14
    /// Tint of the MONOCHROME local glyph. "secondary" = LocalAlbumView.slint:36;
    /// "muted" = SourceGlyph.slint:28, whose header calls the
    /// muted-never-accent rule load-bearing.
    property string localTint: "secondary"

    readonly property bool isQobuz: kind === "qobuz" || kind === "qobuz_download"
        || kind === "qobuz_purchase" || kind === "offline"
    readonly property bool isPlex: kind === "plex"
    readonly property bool isPurchase: kind === "qobuz_purchase"

    // SourceGlyph.slint:31 — the Qobuz marks run one pixel larger.
    width: root.isQobuz ? root.glyphSize + 2 : root.glyphSize
    height: width

    readonly property string brandDir: "qrc:/qt/qml/com/blitzfc/qbz/qml/assets/brand/"

    // --- The brand marks: UNTINTED, by rule -------------------------------
    Image {
        id: mark
        anchors.fill: parent
        visible: (root.isQobuz && !root.isPurchase) || root.isPlex
        source: root.isPlex ? root.brandDir + "plex-logo.svg"
                            : root.brandDir + "qobuz-logo-filled.svg"
        fillMode: Image.PreserveAspectFit   // SourceGlyph's `image-fit: contain`
        // Decode at the size actually drawn: these are a 512x512 and a
        // ~1000x1006 source rendered into 15-16px.
        sourceSize.width: root.width
        sourceSize.height: root.height
        smooth: true
        asynchronous: true
    }

    // --- Purchased: the same mark, gold ------------------------------------
    // The only arm that tints a multi-colour asset, so the only one that needs
    // an effect. `_noShaders` degrades to the plain mark — the source is still
    // legible, it just loses the "purchased" colour, which beats an empty cell.
    readonly property bool _noShaders: GraphicsInfo.api === GraphicsInfo.Software
        || GraphicsInfo.api === GraphicsInfo.Null
    Image {
        id: purchaseMark
        anchors.fill: parent
        visible: root.isPurchase
        source: root.brandDir + "qobuz-logo-filled.svg"
        fillMode: Image.PreserveAspectFit
        sourceSize.width: root.width
        sourceSize.height: root.height
        smooth: true
        asynchronous: true
        layer.enabled: root.isPurchase && !root._noShaders
        layer.effect: MultiEffect {
            colorization: 1.0
            // SourceGlyph.slint:27 — the §8.7 purchase gold.
            colorizationColor: "#eab308"
        }
    }

    // --- Local / anything else: the monochrome glyph, tinted ---------------
    QbzIcon {
        anchors.fill: parent
        visible: !root.isQobuz && !root.isPlex
        name: "hard-drive"
        tintName: root.localTint
    }
}
