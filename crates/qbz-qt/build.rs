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

fn main() {
    let mut qrc_files: Vec<String> = vec![
        "qml/assets/qbz-logo.png".to_string(),
        "qml/assets/hi-res.svg".to_string(),
    ];
    collect_qrc_files(Path::new("qml/assets/icons"), &mut qrc_files);
    collect_qrc_files(Path::new("qml/assets/fonts"), &mut qrc_files);
    // Non-QML resources land at qrc:/qt/qml/com/blitzfc/qbz/<path> — QML
    // files in qml/ reference them relatively (e.g.
    // "assets/icons/primary/plus.svg").
    let qrc_refs: Vec<&str> = qrc_files.iter().map(String::as_str).collect();

    CxxQtBuilder::new()
        // Qt modules the bridge links against (Qt6 CMake in /usr/lib64/cmake).
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.qbz",
            rust_files: &["src/bridge.rs"],
            qml_files: &[
                "qml/Main.qml",
                "qml/LoginScreen.qml",
                "qml/AppShell.qml",
                "qml/AlbumCard.qml",
                "qml/Sidebar.qml",
                "qml/HeaderBar.qml",
                "qml/NowPlayingBarSmall.qml",
                "qml/QueuePanel.qml",
                "qml/HomeView.qml",
                "qml/LibraryView.qml",
                "qml/QbzTheme.qml",
                "qml/QbzIcon.qml",
                "qml/QbzScrollBar.qml",
                "qml/QbzSpinner.qml",
            ],
            qrc_files: &qrc_refs,
            ..Default::default()
        })
        .build();
}
