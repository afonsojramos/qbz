//! Build script — emits two compile-time env vars consumed by the About modal
//! (`crate::about`): the build DATE (`QBZ_BUILD_DATE`, `YYYY-MM-DD`) and the
//! short git COMMIT (`QBZ_BUILD_COMMIT`). Both degrade gracefully:
//!
//! - The date prefers `SOURCE_DATE_EPOCH` (reproducible builds; Flathub sets it)
//!   and falls back to the wall clock at build time.
//! - The commit shells out to `git rev-parse --short HEAD`; in an offline source
//!   tarball with no `.git` (Flathub/Snap) it is simply empty.
//!
//! Nothing here touches the network or fails the build — a missing git or clock
//! just yields an empty string.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Re-run when HEAD moves so the embedded commit stays fresh.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    let epoch: i64 = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs() as i64)
        })
        .unwrap_or(0);

    println!("cargo:rustc-env=QBZ_BUILD_DATE={}", format_ymd(epoch));

    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=QBZ_BUILD_COMMIT={commit}");
}

/// Convert a Unix timestamp (seconds, UTC) to a `YYYY-MM-DD` string using
/// Howard Hinnant's days→civil algorithm (no chrono dependency in the build
/// script). Returns an empty string for a zero/invalid epoch.
fn format_ymd(epoch_secs: i64) -> String {
    if epoch_secs <= 0 {
        return String::new();
    }
    let days = epoch_secs.div_euclid(86_400);
    // days_from_civil inverse (Hinnant, "chrono-Compatible Low-Level Date Algorithms").
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}
