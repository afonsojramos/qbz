//! QBZ Slint POC binary.
//!
//! Validates whether Slint can become QBZ's future UI foundation on top of
//! the framework-agnostic `qbz-app` / `qbz-core` stack. See the POC ADR
//! (`qbz-nix-docs/qbz-adr/qbz_slint_functional_poc_adr.md`).
//!
//! Lives only on the private `slint-poc` branch (ADR-007). The Slint UI tree
//! is compiled from `ui/app.slint` by `build.rs`; `include_modules!` pulls in
//! the generated Rust bindings.

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    window.run()
}
