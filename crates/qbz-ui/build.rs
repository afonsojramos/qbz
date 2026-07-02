//! Compiles the Slint UI tree with bundled translations. `ui/app.slint` is the
//! single entry point. Translations are bundled (pure-Rust, no C dep) from
//! `translations/<lang>/LC_MESSAGES/qbz-slint.po`; msgid = English source, no context.
fn main() {
    // SLINT_SCALE_FACTOR is a RUNTIME preference in this app (main.rs sets it
    // from the persisted interface-size preset). If it leaks into the BUILD
    // environment, the slint compiler const-propagates the factor and the
    // runtime override becomes a permanent no-op — strip it unconditionally.
    std::env::remove_var("SLINT_SCALE_FACTOR");
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Slint UI failed to compile");
}
