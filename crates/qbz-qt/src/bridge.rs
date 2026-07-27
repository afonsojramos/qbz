//! Phase 0 toolchain-gate bridge.
//!
//! A single trivial QObject proves the cxx-qt codegen <-> Qt 6.11 pipeline
//! (qmltyperegistrar, moc, CMake link) before any real binding work lands.

#[cxx_qt::bridge]
mod qbz_bridge {
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, greeting)]
        type HelloBridge = super::HelloBridgeRust;
    }

    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }
}

/// Rust side of the phase-0 object (constructed from QML if instantiated).
#[derive(Default)]
pub struct HelloBridgeRust {
    greeting: cxx_qt_lib::QString,
}
