//! Slint UI compilation unit. The whole `.slint` tree (entry `ui/app.slint`)
//! is compiled by `build.rs` with bundled translations; `include_modules!`
//! brings the generated Rust types (AppWindow, MiniPlayerWindow, the globals,
//! the structs/enums) into this crate's root. They are `pub`, so the
//! `qbz-slint` binary (and future feature crates) use them via
//! `qbz_slint_ui::AppWindow` etc. — Slint generated TYPES cross crate
//! boundaries fine (only `.slint` file imports do not).
slint::include_modules!();
