//! Compiles the Slint UI tree with bundled translations. `ui/app.slint` is the
//! single entry point. Translations are bundled (pure-Rust, no C dep) from
//! `<lang>/LC_MESSAGES/qbz-ui.po`; msgid = English source, no context.
//!
//! THE CATALOGUES MOVED OUT of this crate and are now BORROWED. They belong to
//! `qbz-i18n`, which embeds them with `include_str!` and is the crate every
//! frontend uses — it declared itself "frontend-agnostic" while reaching into
//! this directory for its data, so deleting this crate (planned before the Qt
//! release) would have broken the Qt app over a path nobody read as a
//! dependency. This borrow is the temporary half: it dies with the crate.
fn main() {
    // SLINT_SCALE_FACTOR is a RUNTIME preference in this app (main.rs sets it
    // from the persisted interface-size preset). If it leaks into the BUILD
    // environment, the slint compiler const-propagates the factor and the
    // runtime override becomes a permanent no-op — strip it unconditionally.
    std::env::remove_var("SLINT_SCALE_FACTOR");
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("../qbz-i18n/translations")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Slint UI failed to compile");
}
