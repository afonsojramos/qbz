//! Script-specific application font fallbacks.
//!
//! The UI's "System" choice remains Qt's real system font.  This module only
//! gives Qt a deterministic face when a selected UI font lacks Devanagari,
//! avoiding its noisy scan through unrelated installed families.

unsafe extern "C" {
    fn qbz_register_devanagari_fallback() -> bool;
}

pub(crate) fn register_devanagari_fallback() {
    // SAFETY: the C++ function has no arguments, is called once on the GUI
    // thread after QGuiApplication exists, and only mutates QFontDatabase's
    // documented process-global application-font registry.
    if unsafe { qbz_register_devanagari_fallback() } {
        log::info!("[qbz-qt] font fallback -> Noto Sans Devanagari");
    } else {
        log::warn!("[qbz-qt] bundled Noto Sans Devanagari fallback failed to load");
    }
}
