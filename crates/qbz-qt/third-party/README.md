# Vendored third-party assets

## circle-flags (MIT)

`qml/assets/flags/*.svg` — 265 circular country flags, ISO 3166-1 alpha-2,
from https://github.com/HatScripts/circle-flags (gh-pages branch).

Only the two-letter country files are vendored; the upstream `language/` and
`other/` subdirectories are not used. Licence: `LICENSE-circle-flags.md`.

**Why bundled instead of fetched.** The Tauri reference built the flag URL
against the upstream CDN at runtime
(`ArtistDetailView`'s scene view, `hatscripts.github.io/circle-flags/flags/{cc}.svg`),
which meant a third-party HTTPS request per scene, a blank flag with no
network, and nothing at all in offline mode. Owner ruling R2 (2026-08-14,
`qbz-nix-docs/qt-frontend/2026-08-14-artist-scene-musician/00-CONTRACT.md`)
bundles them instead. The whole set is ~159 KB.

`build.rs` sweeps `qml/assets/` into the qrc automatically, so nothing needs
registering — dropping a file in is enough. That is also why this licence
lives HERE and not beside the SVGs: the sweep takes every file it finds, and
a markdown file has no business being a QML resource.
