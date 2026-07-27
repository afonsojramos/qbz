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
            qml_files: &["qml/Main.qml"],
            ..Default::default()
        })
        .build();
}
