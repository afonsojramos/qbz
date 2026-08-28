# Third-party licences shipped with QBZ for Windows

QBZ's own licence is in `LICENSE` at the repository root. This directory
carries the notices that must travel **with the binaries**, because the Windows
artifacts bundle their dependencies rather than resolving them from a system
package manager the way the Linux packages do.

## Qt 6 — LGPL v3

The Windows MSI and the portable zip ship the Qt runtime. QBZ links against it
**dynamically** and does not modify it, which is what the LGPL requires for the
arrangement below; `LGPL-3.0.txt` is the full text, verbatim from
<https://www.gnu.org/licenses/lgpl-3.0.txt>.

**Version:** Qt 6.9.3, `win64_msvc2022_64`, installed by
[aqtinstall](https://github.com/miurahr/aqtinstall) in
`.github/workflows/release-windows.yml`. The version is pinned there
(`QT_VERSION`) rather than floating, so the notice below always names what
actually shipped.

**Modules present in a release build**, as reported by `windeployqt` on the
deployed tree:

Core, Gui, Network, OpenGL, Svg,
Qml, QmlMeta, QmlModels, QmlWorkerScript, LabsQmlModels,
Quick, QuickControls2, QuickControls2Impl, QuickEffects, QuickLayouts,
QuickShapes, QuickTemplates2,
and the Controls style implementations Basic, Fusion, Imagine, Material,
Universal, Windows and FluentWinUI3.

Qt SQL, Qt TLS and Qt NetworkInformation are deliberately **removed** after
`windeployqt` runs: nothing in QBZ uses them, and shipping a plugin that is
never loaded only widens what has to be tracked and notified.

**Where to get the sources.** Qt 6.9.3 sources are published by The Qt Company
at <https://download.qt.io/archive/qt/6.9/6.9.3/single/> and the development
history is at <https://code.qt.io/cgit/qt/qt5.git/>. The binaries QBZ ships are
the unmodified upstream `win64_msvc2022_64` builds; no Qt patch is applied
anywhere in this repository.

**Relinking.** The LGPL's core practical requirement is that a user can replace
the library. QBZ satisfies it by the shape of the build: Qt is dynamically
linked and the DLLs sit beside `qbz.exe` in the install directory, so replacing
them with a compatible Qt 6.9 build of your own is a file copy. No static Qt
build and no `qt.conf` prefix rewrite is used, both of which would make that
harder.

## Rust crates

The Rust dependency tree is MIT/Apache-2.0 dual-licensed with a small number of
BSD and Unicode-DFS crates, all permissive and all requiring attribution rather
than source availability. `cargo tree` and `Cargo.lock` are the authoritative
list; the SBOM step in the release workflow is where a generated manifest
belongs when that lands.
