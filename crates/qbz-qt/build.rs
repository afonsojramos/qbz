use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
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
                "qml/ShellPlaceholder.qml",
                "qml/QbzTheme.qml",
            ],
            // Non-QML resources. The path below lands at
            // qrc:/qt/qml/com/blitzfc/qbz/qml/assets/qbz-logo.png — QML files
            // in qml/ reference it relatively as "assets/qbz-logo.png".
            qrc_files: &["qml/assets/qbz-logo.png"],
            ..Default::default()
        })
        .build();
}
