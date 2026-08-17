use std::path::Path;

use cxx_qt_build::{CxxQtBuilder, QmlModule};

/// Collect every file under `dir` (recursive), crate-root-relative — the
/// baked icon variants (qml/assets/icons/<tint>/<name>.svg) are too many to
/// list by hand.
fn collect_qrc_files(dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_qrc_files(&path, out);
        } else {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

/// Every RHI variant a Qt Quick shader should carry.
///
/// `qsb --qt6` is only `100 es,120,150` + HLSL 50 + MSL 12. This is that plus
/// the modern GLES levels and desktop 440, because the runtime asks for the
/// HIGHEST it can use and falls through: on a GLES 3.2 context Qt tries
/// 320, 310, 300, 100 in that order and gives up if none is present.
const SHADER_GLSL: &str = "100 es,300 es,310 es,320 es,120,150,440";
const SHADER_HLSL: &str = "50";
const SHADER_MSL: &str = "12";

/// The same list without `100 es`, for shaders that use unsigned integer
/// arithmetic. GLSL ES 1.00 (OpenGL ES 2.0) has NO uint type, so SPIRV-Cross
/// has to re-type the literals as int and refuses when that would make one
/// negative — `qsb` then fails the whole bake, not just that variant.
///
/// The one shader in this position is `ambient.frag`, and its hash CANNOT drop
/// to floats: the reference's own comment (ambient.wgsl) records that a float
/// hash lets the same lattice corner disagree between adjacent cells and draws
/// a straight seam through the warp field. So the ES 2.0 level is dropped
/// instead, and AmbientField's Canvas arm covers that hardware — which is what
/// the fallback exists for.
const SHADER_GLSL_NO_ES100: &str = "300 es,310 es,320 es,120,150,440";

/// Opt out of `100 es` by putting this marker anywhere in the shader source.
const NO_ES100_MARKER: &str = "QSB-SKIP-GLES100";

/// Opt out of the `-b` batching rewrite for VERTEX shaders by putting this
/// marker anywhere in the source. The rewrite is only valid for Qt Quick
/// scene-graph-batched items (ShaderEffect stages); a custom
/// `QQuickRhiItem` (linebed.vert, A4) owns its attribute layout and the
/// rewrite would corrupt it.
const NO_BATCH_MARKER: &str = "QSB-SKIP-BATCH";

/// Is `out` already a current bake of `src`? True only when it exists and is
/// no older than BOTH the shader source and this build script — the script
/// because the variant lists and the `100 es` opt-out live here, so editing
/// them has to invalidate every `.qsb` even though no `.frag` moved.
///
/// Anything unreadable answers false: a bake we cannot reason about is one we
/// redo. Equal mtimes count as current (a same-second write is the normal case
/// right after a bake, and treating it as stale would restart the churn).
fn up_to_date(src: &Path, out: &Path) -> bool {
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let (Some(out_t), Some(src_t)) = (mtime(out), mtime(src)) else {
        return false;
    };
    if out_t < src_t {
        return false;
    }
    match mtime(Path::new("build.rs")) {
        Some(script_t) => out_t >= script_t,
        None => true,
    }
}

/// Locate Qt's `qsb`. PATH first, then the usual install layouts (Linux
/// distro, Homebrew on the Mac mini).
fn find_qsb() -> Option<std::path::PathBuf> {
    if let Ok(explicit) = std::env::var("QSB") {
        let p = std::path::PathBuf::from(explicit);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join("qsb");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    for candidate in [
        "/usr/lib64/qt6/bin/qsb",
        "/usr/lib/qt6/bin/qsb",
        "/usr/lib/x86_64-linux-gnu/qt6/bin/qsb",
        "/opt/homebrew/opt/qt/bin/qsb",
        "/usr/local/opt/qt/bin/qsb",
    ] {
        let p = std::path::PathBuf::from(candidate);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Compile every `.frag` / `.vert` under `qml/assets/shaders/` to a `.qsb`
/// beside it, with the full variant set.
///
/// WHY THIS EXISTS. The `.qsb` files used to be baked BY HAND and committed,
/// with no record of the command. They carried SPIR-V + GLSL 440 + MSL and
/// **no GLES variants at all**, so the moment the app got a GLES context every
/// one of them failed with "No GLSL shader code found (versions tried: 320,
/// 310, 300, 100)" and the visualiser went blank (2026-08-11). A hand-baked
/// artifact cannot be audited and will be copied by the next person who adds a
/// shader — so the bake is part of the build now.
///
/// Missing `qsb` is a WARNING, not an error: the committed `.qsb` are still
/// in the tree and still load, so a box without Qt's shader tools can build.
///
/// THE BAKE MUST BE IDEMPOTENT, and that is not a nicety — it is the whole
/// reason `up_to_date` exists. `qsb` REWRITES its output unconditionally:
/// byte-identical content, brand-new mtime (measured — same md5, mtime moves).
/// The `.qsb` live under `qml/assets`, which `main` watches with
/// `cargo:rerun-if-changed=qml` plus one line per qrc file, so an
/// unconditional bake made every build touch a file cargo was watching. Cargo
/// then re-ran this script on the NEXT invocation, which baked again, which
/// dirtied the watch again: `cargo build` never reached a no-op and every
/// `qt-run.sh` recompiled the crate from the build script down. Introduced in
/// 21e941bdf together with the bake itself, and it is why the tree started
/// rebuilding on every run when nothing had changed.
fn build_shaders() {
    let dir = Path::new("qml/assets/shaders");
    if !dir.is_dir() {
        return;
    }
    let Some(qsb) = find_qsb() else {
        println!(
            "cargo:warning=qsb not found — shaders keep their committed .qsb. \
             Set QSB=/path/to/qsb to re-bake them."
        );
        return;
    };
    for entry in std::fs::read_dir(dir).expect("read shaders dir").flatten() {
        let src = entry.path();
        let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "frag" | "vert") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", src.display());
        let out = src.with_extension(format!("{ext}.qsb"));
        if up_to_date(&src, &out) {
            continue;
        }
        let text = std::fs::read_to_string(&src).unwrap_or_default();
        let glsl = if text.contains(NO_ES100_MARKER) {
            SHADER_GLSL_NO_ES100
        } else {
            SHADER_GLSL
        };
        let mut cmd = std::process::Command::new(&qsb);
        cmd.args(["--glsl", glsl, "--hlsl", SHADER_HLSL, "--msl", SHADER_MSL]);
        // Vertex shaders used by Qt Quick need the batching rewrite, or the
        // scene graph cannot batch the item and falls back per-node —
        // EXCEPT custom QQuickRhiItem stages, which own their attribute
        // layout (NO_BATCH_MARKER).
        if ext == "vert" && !text.contains(NO_BATCH_MARKER) {
            cmd.arg("-b");
        }
        cmd.arg("-o").arg(&out).arg(&src);
        match cmd.status() {
            Ok(s) if s.success() => {}
            Ok(s) => panic!("qsb failed ({s}) on {}", src.display()),
            Err(e) => panic!("could not run qsb on {}: {e}", src.display()),
        }
    }
}

/// Emit the two compile-time env vars the About modal's "Build" row reads
/// (`src/about_qt.rs`): the build DATE (`QBZ_BUILD_DATE`, `YYYY-MM-DD`) and the
/// short git COMMIT (`QBZ_BUILD_COMMIT`). Ported from `crates/qbz/build.rs:16-41`.
///
/// Both degrade gracefully and neither can fail the build:
/// - the date prefers `SOURCE_DATE_EPOCH` (reproducible builds; Flathub sets
///   it) and falls back to the wall clock;
/// - the commit shells out to `git rev-parse --short HEAD` and is simply EMPTY
///   in an offline source tarball with no `.git` (Flathub/Snap).
fn emit_build_stamp() {
    // Re-run when HEAD moves so the embedded commit stays fresh.
    //
    // Ask git where HEAD actually lives instead of assuming `../../.git/HEAD`:
    // in a git WORKTREE `.git` is a 70-byte file, not a directory, so that path
    // does not exist — and cargo treats a missing `rerun-if-changed` target as
    // always-dirty, which re-ran this whole script (per-shader qsb, the
    // recursive asset sweep, cxx-qt/moc codegen) on every single cargo
    // invocation in the tree. Emit nothing when git cannot answer.
    if let Some(head) = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && std::path::Path::new(s).exists())
    {
        println!("cargo:rerun-if-changed={head}");
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let epoch: i64 = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs() as i64)
        })
        .unwrap_or(0);

    println!("cargo:rustc-env=QBZ_BUILD_DATE={}", format_ymd(epoch));

    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=QBZ_BUILD_COMMIT={commit}");
}

/// Unix timestamp (seconds, UTC) -> `YYYY-MM-DD`, via Howard Hinnant's
/// days→civil algorithm. Copied as-is from `crates/qbz/build.rs:58-75`: it is
/// chrono-free BY DESIGN, and `qbz-qt` has no chrono dependency to lean on.
/// Empty string for a zero/invalid epoch.
fn format_ymd(epoch_secs: i64) -> String {
    if epoch_secs <= 0 {
        return String::new();
    }
    let days = epoch_secs.div_euclid(86_400);
    // days_from_civil inverse (Hinnant, "chrono-Compatible Low-Level Date Algorithms").
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

/// The hand-written C++ QQuickRhiItems: A4's `LineBedItem`, A2's
/// `PlasmaItem`, A3's `RibbonItem` and B1's `TunnelFlowItem` (`cxx/*_item.*`,
/// the scenes a ShaderEffect cannot express — line strips / feedback
/// ping-pong / persistent sub-rect texture writes — spec 01 §2.5/§2.1/§2.4,
/// spec 02 §3).
///
/// The crate had no non-cxx-qt C++ precedent (`src/macos_chrome.rs` is pure
/// Rust objc2), so this adds the minimal wiring: `moc` (the Q_OBJECT meta
/// object) and the Qt include paths come from `qt-build-utils` — already in
/// the lock as cxx-qt-build's own engine, same version — and the compile is
/// a plain `cc::Build` static lib that cargo links into the binary. The Qt
/// LINK libraries are already emitted by CxxQtBuilder below (Gui/Quick are
/// among its modules); this lib only adds objects.
///
/// The `<rhi/qrhi.h>` headers live in Qt's PRIVATE include tree
/// (`<headers>/Qt<Module>/<version>/Qt<Module>`), so those paths are added
/// explicitly. macOS note: the private-headers path exists in the same
/// layout under Homebrew Qt; NOT COMPILED HERE (this tree builds on Linux,
/// same caveat as the macOS objc2 arms).
fn build_rhi_items() {
    let mut qtbuild = qt_build_utils::QtBuild::new(vec![
        "Core".to_string(),
        "Gui".to_string(),
        "Qml".to_string(),
        "Quick".to_string(),
    ])
    .expect("Qt6 not found for the RHI items (cxx-qt-build would fail too)");

    let mut cc = cc::Build::new();
    cc.cpp(true)
        .std("c++17")
        .pic(true)
        .include("cxx"); // the moc outputs do `#include "<name>.h"`
    for item in ["linebed_item", "plasma_item", "ribbon_item", "tunnelflow_item"] {
        let header = format!("cxx/{item}.h");
        let source = format!("cxx/{item}.cpp");
        println!("cargo:rerun-if-changed={header}");
        println!("cargo:rerun-if-changed={source}");
        // moc: the Q_OBJECT meta-object. No QML_ELEMENT (the types are
        // registered by hand in the .cpps), so no uri/include-path
        // arguments are needed.
        let moc = qtbuild.moc(&header, qt_build_utils::MocArguments::default());
        cc.file(source).file(&moc.cpp);
    }
    for p in qtbuild.include_paths() {
        cc.include(p);
    }
    let headers = qtbuild.qmake_query("QT_INSTALL_HEADERS");
    let version = qtbuild.version().to_string();
    for module in ["QtCore", "QtGui", "QtQml", "QtQuick"] {
        let private = format!("{headers}/{module}/{version}/{module}");
        if Path::new(&private).is_dir() {
            cc.include(private);
        }
    }
    cc.compile("qbz_rhi_items");
}

fn main() {
    // The About modal's build stamp. Emitted before anything else so a failure
    // anywhere below cannot leave the env vars unset (env! is a compile error
    // when they are).
    emit_build_stamp();

    // Bake the shaders BEFORE the qrc sweep below collects qml/assets, so a
    // freshly compiled .qsb is the one that gets embedded.
    build_shaders();

    // The hand-written C++ QQuickRhiItems (cxx/). Static lib linked into
    // the binary; the QML types self-register at QGuiApplication
    // construction.
    build_rhi_items();

    // The WHOLE asset tree, recursively. This used to name the root-level
    // files by hand with only icons/ and fonts/ collected, so dropping a new
    // asset next to hi-res.svg compiled fine and then failed at runtime with
    // "QQuickImage: Cannot open" — invisible to cargo check and to the tests.
    let mut qrc_files: Vec<String> = Vec::new();
    collect_qrc_files(Path::new("qml/assets"), &mut qrc_files);
    // Non-QML resources land at qrc:/qt/qml/com/blitzfc/qbz/<path> — QML
    // files in qml/ reference them relatively (e.g.
    // "assets/icons/primary/plus.svg").
    let qrc_refs: Vec<&str> = qrc_files.iter().map(String::as_str).collect();
    // Cargo's default is to rerun build.rs only when a source file changes, so
    // dropping a new icon into qml/assets/ produced a binary whose qrc did not
    // contain it — the file was on disk and the app logged "Cannot open".
    // Watch the whole tree instead.
    println!("cargo:rerun-if-changed=qml");
    for f in &qrc_files {
        println!("cargo:rerun-if-changed={f}");
    }

    CxxQtBuilder::new()
        // Qt modules the bridge links against (Qt6 CMake in /usr/lib64/cmake).
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.qbz",
            // EVERY #[cxx_qt::bridge] file. A bridge missing here does not
            // fail the build: its QML singleton simply does not exist, and
            // every `QbzFoo.bar()` in QML becomes a runtime ReferenceError
            // that `cargo check` cannot see. The four MyQBZ/Blacklist
            // singletons are the newest arrivals; the domain CONTROLLERS
            // (myqbz_qt.rs, blacklist_qt.rs, toast_qt.rs, …) are plain
            // modules and must NOT be listed — only files that declare a
            // #[cxx_qt::bridge] mod belong in this array.
            rust_files: &["src/bridge.rs", "src/session_bridge.rs", "src/shell_bridge.rs", "src/player_bridge.rs", "src/queue_bridge.rs", "src/home_bridge.rs", "src/viz_bridge.rs", "src/immersive_bridge.rs", "src/shader_scene_bridge.rs", "src/suggestions_bridge.rs", "src/hotkeys_bridge.rs", "src/search_bridge.rs", "src/local_bridge.rs", "src/library_bridge.rs", "src/album_bridge.rs", "src/artist_bridge.rs", "src/scene_bridge.rs", "src/musician_bridge.rs", "src/lyrics_qt.rs", "src/icon_tint_qt.rs", "src/cast_bridge.rs", "src/myqbz_bridge.rs", "src/myqbz_add_bridge.rs", "src/disco_bridge.rs", "src/blacklist_bridge.rs", "src/playlist_picker_bridge.rs", "src/playlist_manager_bridge.rs", "src/playlist_import_bridge.rs", "src/dac_wizard_bridge.rs", "src/folder_edit_bridge.rs", "src/playlist_edit_bridge.rs", "src/qconnect_bridge.rs", "src/kiosk_nav_bridge.rs", "src/mini_bridge.rs", "src/tray_bridge.rs", "src/about_bridge.rs", "src/purchases_bridge.rs", "src/offline_manager_bridge.rs"],
            qml_files: &[
                "qml/LoginScreen.qml",
                "qml/Main.qml",
                "qml/cards/AlbumCard.qml",
                "qml/cards/ArtistCard.qml",
                "qml/cards/CollectionMosaic.qml",
                "qml/cards/LabelCard.qml",
                "qml/cards/MixArtwork.qml",
                "qml/cards/PlaylistCard.qml",
                "qml/cards/PlaylistCollage.qml",
                "qml/cards/RadioCard.qml",
                "qml/cards/SlimCard.qml",
                "qml/cards/TrackCard.qml",
                "qml/controls/AddToMixtapeModal.qml",
                "qml/controls/CardMenu.qml",
                "qml/controls/CommandBlock.qml",
                "qml/controls/CardOverlayButton.qml",
                "qml/controls/CardOverlayRow.qml",
                "qml/controls/FolderEditPanel.qml",
                "qml/controls/FolderModals.qml",
                "qml/controls/GroupHeader.qml",
                "qml/controls/MyQbzModals.qml",
                "qml/controls/PlaylistEditModal.qml",
                "qml/controls/PlaylistImportModal.qml",
                "qml/controls/PlaylistPickerModal.qml",
                "qml/controls/PmFolderIcon.qml",
                "qml/controls/QbzCircleAction.qml",
                "qml/controls/QbzColorPicker.qml",
                "qml/controls/QbzConfirmModal.qml",
                "qml/controls/QbzContextMenu.qml",
                "qml/controls/QbzEmptyState.qml",
                "qml/controls/QbzIconButton.qml",
                "qml/controls/QbzLineEdit.qml",
                "qml/controls/QbzLoadingDots.qml",
                "qml/controls/QbzLoadMore.qml",
                "qml/controls/QbzMultiSelectBar.qml",
                "qml/controls/QbzNavButton.qml",
                "qml/controls/QbzOfflinePlaceholder.qml",
                "qml/controls/QbzPrimaryButton.qml",
                "qml/controls/QbzRadioOption.qml",
                "qml/controls/QbzSectionHeader.qml",
                "qml/controls/QbzSegToggle.qml",
                "qml/controls/QbzSelect.qml",
                "qml/controls/QbzSlider.qml",
                "qml/controls/QbzTabBar.qml",
                "qml/controls/QbzTextArea.qml",
                "qml/controls/QbzToast.qml",
                "qml/controls/QbzToggle.qml",
                "qml/controls/QbzToolButton.qml",
                "qml/controls/QbzTooltip.qml",
                "qml/controls/QualityBadge.qml",
                "qml/controls/QualityMini.qml",
                "qml/controls/SettingRow.qml",
                "qml/controls/IconTextButton.qml",
                "qml/controls/SettingsButton.qml",
                "qml/controls/SettingsDivider.qml",
                "qml/controls/SettingsSpacer.qml",
                // Moved out of views/local/ on 2026-07-31: the album/track
                // CARD badges mount it too, and Slint keeps its counterpart in
                // primitives/ (SourceGlyph.slint).
                "qml/controls/SourceIcon.qml",
                "qml/rows/BlacklistRow.qml",
                "qml/rows/TrackCols.qml",
                "qml/rows/TrackListHeader.qml",
                "qml/rows/TrackRow.qml",
                "qml/settings/AppearanceSettings.qml",
                "qml/settings/CustomThemeEditor.qml",
                "qml/settings/IntegrationsSettings.qml",
                "qml/settings/SettingsView.qml",
                "qml/shell/AmbientField.qml",
                "qml/shell/AppShell.qml",
                "qml/shell/ArtPreviewOverlay.qml",
                "qml/shell/Cortinilla.qml",
                "qml/shell/HeaderBar.qml",
                "qml/shell/LyricsPanel.qml",
                "qml/shell/NowPlayingBar.qml",
                "qml/shell/NowPlayingBarSmall.qml",
                "qml/shell/PlayerBar.qml",
                "qml/shell/QueuePanel.qml",
                // Kiosk shell (2026-08-02 kiosk-port contract). The router is
                // SHARED with AppShell (contract D3) and lives here rather
                // than under kiosk/ for that reason.
                "qml/shell/ContentRouter.qml",
                "qml/shell/KioskShell.qml",
                "qml/kiosk/NavRail.qml",
                "qml/kiosk/KioskCard.qml",
                "qml/kiosk/KioskAlbumGrid.qml",
                "qml/kiosk/KioskTrackRow.qml",
                "qml/kiosk/KioskArtistCard.qml",
                "qml/kiosk/KioskSearch.qml",
                "qml/kiosk/KioskNowPlaying.qml",
                "qml/kiosk/KioskDiscover.qml",
                "qml/kiosk/KioskLibrary.qml",
                "qml/kiosk/KioskLocalLibrary.qml",
                "qml/kiosk/KioskMyQBZ.qml",
                "qml/kiosk/KioskArtist.qml",
                "qml/kiosk/KioskAlbum.qml",
                "qml/shell/Sidebar.qml",
                "qml/shell/SidebarFolderFlyout.qml",
                "qml/shell/SidebarRowMenu.qml",
                "qml/shell/SidebarNowPlayingDock.qml",
                "qml/shell/AudioSettingsMenu.qml",
                "qml/shell/ViewModeMenu.qml",
                // Hotkeys (2026-08-03 hotkeys-port contract §4.4/§4.5, block
                // B3): the read-only cheatsheet + the editable customize
                // editor, both self-gated on QbzHotkeys and mounted in
                // AppShell with the global overlays.
                "qml/shell/KeyboardShortcutsModal.qml",
                "qml/shell/CustomizeShortcutsModal.qml",
                // Qobuz Connect (2026-08-01 contract §2): the ONE shared
                // device flyout both bars mount + the diagnostics modal
                // AppShell mounts last.
                "qml/shell/QconnectFlyout.qml",
                "qml/shell/LogViewerModal.qml",
                "qml/shell/ReportIssueModal.qml",
                "qml/shell/QconnectDevModal.qml",
                // About QBZ + What's New (the header menu's last two rows),
                // both mounted at the AppShell overlay tail and self-gated on
                // QbzAbout's two documents.
                "qml/shell/AboutModal.qml",
                "qml/shell/WhatsNewModal.qml",
                // Immersive mode (2026-08-02 immersive-port contract §2) —
                // its own module directory like views/local/ and
                // views/playlistmanager/. B2 shipped the root overlay + the
                // header band; B3 adds the atmosphere underlay, the five
                // FOCUS panels and the song card / track meta / equalizer;
                // B4 adds the SPLIT panels (lyrics split mount lives in
                // ImmersiveView) + the two remaining FOCUS panels; B5 adds
                // the player bar + the search cortinilla.
                "qml/immersive/AlbumReactivePanel.qml",
                "qml/immersive/CoverflowPanel.qml",
                "qml/immersive/EqualizerBars.qml",
                "qml/immersive/ImmersiveAtmosphere.qml",
                "qml/immersive/ImmersiveHeader.qml",
                "qml/immersive/ImmersivePlayerBar.qml",
                "qml/immersive/ImmersiveSearchCortinilla.qml",
                "qml/immersive/ImmersiveSongCard.qml",
                "qml/immersive/ImmersiveTrackMeta.qml",
                "qml/immersive/ImmersiveView.qml",
                "qml/immersive/LyricsFocusPanel.qml",
                "qml/immersive/QueueTabsPanel.qml",
                // Shader scenes (2026-08-15 immersive-completion contract,
                // block A1): the bottom-most scene layer (Tunnel / Aurora /
                // Ambient) driven by QbzShaderScene.
                "qml/immersive/ShaderSceneLayer.qml",
                // Block A4: the Line Bed scene (mode 5) — the QML wrapper
                // for the C++ LineBedItem (cxx/linebed_item.*). Its own
                // file so the RHI item's contract is one screen of QML.
                "qml/immersive/LineBedScene.qml",
                // Block A2: Plasma (scene 1) — QML wrapper for PlasmaItem
                // (cxx/plasma_item.*), the WGSL->GLSL feedback port.
                "qml/immersive/PlasmaScene.qml",
                // Block A3: Spectral Ribbon (scene 4) — RibbonItem wrapper
                // plus the stream/FFT overlay ported from
                // ImmersiveSpectralOverlay.slint.
                "qml/immersive/RibbonScene.qml",
                "qml/immersive/SpectralOverlay.qml",
                // Block B1: Tunnel Flow (scene 8, Qt-only) — the QML wrapper
                // for the C++ TunnelFlowItem (cxx/tunnelflow_item.*), the
                // Tauri Canvas2D tunnel ported to a feedback fragment shader.
                "qml/immersive/TunnelFlowScene.qml",
                "qml/immersive/SpectrumPanel.qml",
                "qml/immersive/StaticPanel.qml",
                "qml/immersive/SuggestionsPanel.qml",
                "qml/immersive/TinyBar.qml",
                "qml/immersive/TrackInfoPanel.qml",
                "qml/immersive/VolumeBar.qml",
                "qml/immersive/WaveBedPanel.qml",
                "qml/theme/QbzIcon.qml",
                "qml/theme/QbzScrollBar.qml",
                "qml/theme/QbzSpinner.qml",
                "qml/theme/QbzTheme.qml",
                "qml/theme/RoundedImage.qml",
                "qml/views/AlbumView.qml",
                "qml/views/CoverLightbox.qml",
                "qml/views/ArtistReleasesView.qml",
                "qml/views/ArtistSceneView.qml",
                "qml/views/scene/SceneGenreFilter.qml",
                "qml/views/scene/AlphaIndexRail.qml",
                "qml/views/scene/ScenePanelMode.qml",
                "qml/views/MusicianPageView.qml",
                "qml/views/ArtistView.qml",
                "qml/views/BlacklistManagerView.qml",
                "qml/views/HomeView.qml",
                "qml/views/LibraryView.qml",
                "qml/views/LocalLibraryView.qml",
                "qml/views/PlaylistRail.qml",
                "qml/views/PlaylistView.qml",
                "qml/views/SearchView.qml",
                "qml/views/SectionRail.qml",
                "qml/shell/CastPicker.qml",
                "qml/controls/BrowseGenreButton.qml",
                "qml/controls/QbzToggleButton.qml",
                "qml/controls/ViewModeToggle.qml",
                "qml/views/AlbumCollection.qml",
                "qml/views/AlbumListHeader.qml",
                "qml/views/AlbumListRow.qml",
                "qml/views/DiscoverBrowseView.qml",
                "qml/views/LabelReleasesView.qml",
                "qml/views/LabelView.qml",
                "qml/views/MixView.qml",
                "qml/views/PlayHistoryView.qml",
                "qml/views/PlaylistBrowseView.qml",
                "qml/views/PlaylistListRow.qml",
                "qml/controls/DiscoverConfigModal.qml",
                "qml/controls/GenreFilterPopup.qml",
                "qml/shell/NavFlyout.qml",
                "qml/shell/NavSectionGlyph.qml",
                "qml/controls/HeaderGradient.qml",
                "qml/controls/QbzJumpNavBar.qml",
                "qml/shell/LyricsControlsFlyout.qml",
                "qml/shell/LyricsLineRow.qml",
                "qml/shell/LyricsLinesView.qml",
                "qml/shell/LyricsSyncEngine.qml",
                "qml/shell/NavGestureLayer.qml",
                "qml/controls/QbzCheckbox.qml",
                // Library lane (B1-B5): the promoted A-Z strip + the
                // per-surface bodies LibraryView.qml was split into.
                "qml/controls/QbzAlphaStrip.qml",
                "qml/views/library/FeedGridCell.qml",
                "qml/views/library/FeedListRow.qml",
                "qml/views/library/LibraryAlbumsList.qml",
                "qml/views/library/LibraryArtistsPanel.qml",
                "qml/views/library/LibraryToolbar.qml",
                "qml/controls/WarningBanner.qml",
                "qml/settings/AudioSettings.qml",
                "qml/settings/BlacklistSettings.qml",
                "qml/settings/DacWizardModal.qml",
                "qml/settings/DeveloperSettings.qml",
                "qml/settings/DiagnosticsPanel.qml",
                "qml/settings/LibFolderEditModal.qml",
                "qml/settings/LibraryFolderTable.qml",
                "qml/settings/LocalLibrarySettings.qml",
                "qml/settings/OfflineSettings.qml",
                "qml/settings/PlaybackSettings.qml",
                "qml/settings/PlexSettings.qml",
                "qml/settings/SandboxSettings.qml",
                "qml/settings/SettingsConfirmHost.qml",
                "qml/controls/QbzSkeleton.qml",
                "qml/shell/FavToggle.qml",
                "qml/shell/InfoCreditCell.qml",
                "qml/shell/InfoMetaCell.qml",
                "qml/shell/SongCard.qml",
                "qml/shell/TrackInfoBody.qml",
                "qml/shell/TrackInfoModal.qml",
                "qml/shell/AlbumInfoModal.qml",
                "qml/shell/MusicianModal.qml",
                "qml/shell/TransportControls.qml",
                "qml/views/local/LocalIconSelect.qml",
                "qml/views/local/LocalTip.qml",
                "qml/controls/QualityBadgeFull.qml",
                "qml/shell/AudioStamp.qml",
                "qml/shell/SongCardStamp.qml",
                "qml/shell/SpectrumBand.qml",
                "qml/shell/VizSettle.qml",
                "qml/views/LocalAlbumView.qml",
                "qml/views/local/FilterChip.qml",
                "qml/views/local/FolderSubcard.qml",
                "qml/views/local/LocalAlbumCollection.qml",
                "qml/views/local/LocalAlbumHeader.qml",
                "qml/views/local/LocalAlbumRow.qml",
                "qml/views/local/LocalAlbumsTab.qml",
                "qml/views/local/LocalArtistRow.qml",
                "qml/views/local/LocalArtistsTab.qml",
                "qml/views/local/LocalChrome.qml",
                "qml/views/local/LocalEphemeralPane.qml",
                "qml/views/local/LocalFilterPopup.qml",
                "qml/views/local/LocalFolderDetail.qml",
                "qml/views/local/LocalFoldersTab.qml",
                "qml/views/local/LocalNote.qml",
                "qml/views/local/LocalSearchBox.qml",
                "qml/views/local/LocalToolbar.qml",
                "qml/views/local/LocalTrackRow.qml",
                "qml/views/local/LocalTracksTab.qml",
                "qml/views/local/LocalTreeRail.qml",
                "qml/views/local/SelectCheck.qml",
                "qml/views/local/TreeRow.qml",
                "qml/views/local/VersionPicker.qml",
                // MyQBZ (Mixtapes / Collections / Artist-Collection builder).
                // Its own subdirectory, like views/local/, because the five
                // files only ever mount each other.
                "qml/views/myqbz/DiscoBuilderView.qml",
                "qml/views/myqbz/DiscoCandidateRow.qml",
                "qml/views/myqbz/MyQbzCard.qml",
                "qml/views/myqbz/MyQbzDetailRow.qml",
                "qml/views/myqbz/MyQbzDetailView.qml",
                "qml/views/myqbz/MyQbzGridView.qml",
                // Purchases (routes "purchases" and "purchase-album") — the
                // opt-in Qobuz store surface, plus its own module directory
                // like views/local/ and views/myqbz/.
                //
                // TWO different silent failures meet here, and the feature is
                // the one nobody can smoke-test (it is not sold in the owner's
                // region, so their account returns an empty list forever):
                // a missing ROUTE view leaves the qrc without the file and the
                // router mounts nothing, and a missing MODULE file fails its
                // PARENT at load with "… is not a type" — taking the whole
                // screen, not one row. Both are invisible to cargo check and to
                // both audit scripts, and `show_purchases` defaults OFF, so
                // neither would be noticed by anyone who never turns it on.
                // Offline Cache Manager (route "offlinemanager") — reached
                // only from Settings > Offline. A route view missing from this
                // array is absent from the qrc and the router mounts NOTHING:
                // a blank pane, invisible to cargo check and to both audits.
                "qml/views/OfflineManagerView.qml",
                // Awards (routes "award" and "awardalbums"). Both are route
                // views: absent from the qrc, the router mounts NOTHING and
                // the pane is blank — invisible to cargo check and to both
                // audits.
                "qml/views/AwardView.qml",
                "qml/views/AwardAlbumsView.qml",
                "qml/views/PurchasesView.qml",
                "qml/views/PurchaseAlbumView.qml",
                "qml/views/purchases/PurchaseAlbumsCollection.qml",
                "qml/views/purchases/PurchaseGridCard.qml",
                "qml/views/purchases/PurchaseListRow.qml",
                "qml/views/purchases/PurchaseTrackRow.qml",
                "qml/views/purchases/PurchasesToolbar.qml",
                // Playlist Manager (route "playlistmanager"): the router target
                // plus the TWELVE files of its own module directory. A .qml
                // missing from this array is absent from the qrc and fails its
                // PARENT file at load with "… is not a type" — invisible to
                // cargo check, and it takes the whole view down, not one row.
                "qml/views/PlaylistManagerView.qml",
                "qml/views/playlistmanager/PmActionButton.qml",
                "qml/views/playlistmanager/PmFolderCard.qml",
                "qml/views/playlistmanager/PmFolderChip.qml",
                "qml/views/playlistmanager/PmFolderMenu.qml",
                "qml/views/playlistmanager/PmGridCard.qml",
                "qml/views/playlistmanager/PmListRow.qml",
                "qml/views/playlistmanager/PmLocalBadge.qml",
                "qml/views/playlistmanager/PmMenuRow.qml",
                "qml/views/playlistmanager/PmPageHead.qml",
                "qml/views/playlistmanager/PmToolbar.qml",
                "qml/views/playlistmanager/PmTreeFolderRow.qml",
                "qml/views/playlistmanager/PmTreePlaylistRow.qml",
                // Miniplayer (2026-08-03 miniplayer/tray-port contract §3.4) —
                // its own module directory, like immersive/ and kiosk/. B2
                // ships the window, the card shell and the two DISPLAY
                // surfaces; B3 the footer, its four primitives and the hover
                // capsule; B4 the queue and lyrics surfaces.
                "qml/miniplayer/MiniWindow.qml",
                "qml/miniplayer/MiniShell.qml",
                "qml/miniplayer/MiniExplicitBadge.qml",
                "qml/miniplayer/MiniCoverArt.qml",
                "qml/miniplayer/MiniCompactSurface.qml",
                "qml/miniplayer/MiniArtworkSurface.qml",
                "qml/miniplayer/MiniQueueSurface.qml",
                "qml/miniplayer/MiniLyricsSurface.qml",
                "qml/miniplayer/MiniFooter.qml",
                "qml/miniplayer/MiniWindowControls.qml",
                "qml/miniplayer/MiniSeek.qml",
                "qml/miniplayer/MiniTransport.qml",
                "qml/miniplayer/MiniVolume.qml",
                "qml/miniplayer/TBtn.qml",
                "qml/miniplayer/CapBtn.qml",
            ],
            qrc_files: &qrc_refs,
            ..Default::default()
        })
        .build();
}
