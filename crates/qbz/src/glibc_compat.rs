//! glibc symbol-version compatibility shim (Linux only).
//!
//! This binary is built on a host with a very new glibc (2.43+), whose linker
//! binds the single-precision libm functions `sinhf` / `coshf` / `atan2f` /
//! `acosf` / `asinf` / `acoshf` to their brand-new `GLIBC_2.43` symbol version
//! (glibc 2.43 re-versioned them). That alone makes the executable refuse to
//! start on any distro with glibc < 2.43 ("version `GLIBC_2.43' not found"),
//! even though nothing actually needs the new behaviour.
//!
//! Fix: `build.rs` passes `-Wl,--wrap=<fn>` for each of the six, so every
//! reference to e.g. `sinhf` (from anywhere in the binary, including the Slint
//! UI crate `qbz_ui`) is redirected by the linker to `__wrap_sinhf` below. Each
//! wrapper computes the result via the `f64` libm entry point (`sinh`, …), which
//! glibc has exported at the ancient `GLIBC_2.2.5` version forever — so the
//! binary's max required glibc drops to whatever else it needs (currently 2.39)
//! and it runs on older distros (Mint / Ubuntu / Debian / Fedora, glibc >= 2.39).
//!
//! The `f64`-then-truncate path is at least as accurate as the native `f32`
//! implementation, so results are unchanged for our use (visualizer / shader
//! math). The linker's `__real_<fn>` alias (the original 2.43 symbol) is
//! deliberately never referenced.

/// Defines a `--wrap` target for a single-argument libm float function,
/// computing it through the old-versioned `f64` entry point. See module docs.
macro_rules! wrap_unary_f32 {
    ($wrap:ident, $f64_method:ident) => {
        #[no_mangle]
        pub extern "C" fn $wrap(x: f32) -> f32 {
            (x as f64).$f64_method() as f32
        }
    };
}

wrap_unary_f32!(__wrap_sinhf, sinh);
wrap_unary_f32!(__wrap_coshf, cosh);
wrap_unary_f32!(__wrap_acosf, acos);
wrap_unary_f32!(__wrap_asinf, asin);
wrap_unary_f32!(__wrap_acoshf, acosh);

/// `--wrap` target for `atan2f(y, x)` — routed through `f64::atan2`. See module docs.
#[no_mangle]
pub extern "C" fn __wrap_atan2f(y: f32, x: f32) -> f32 {
    (y as f64).atan2(x as f64) as f32
}
