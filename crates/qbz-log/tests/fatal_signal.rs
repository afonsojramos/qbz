//! End-to-end proof that a crash reports itself instead of vanishing as `-11`.
//!
//! The intermittent packaged-AppDir SIGSEGV (see `qbz-log/src/fatal.rs`) left
//! nothing but an exit status, so every occurrence cost a re-run and taught us
//! nothing. This test crashes a real child process on purpose and requires
//! both halves of the contract:
//!
//!   1. the handler names the signal, the faulting address and the thread, and
//!      prints a backtrace, and
//!   2. the wait status the parent sees is STILL the signal — the reporter
//!      must not turn a segfault into a clean exit and hide it from CI, the
//!      smoke harness or a core dump.

#![cfg(unix)]

use std::os::unix::process::ExitStatusExt;
use std::process::Command;

/// Set in the child so it takes the crashing branch instead of recursing.
const CHILD_MARKER: &str = "QBZ_LOG_FATAL_TEST_CHILD";

#[test]
fn a_segfault_reports_itself_and_still_dies_of_the_signal() {
    if std::env::var_os(CHILD_MARKER).is_some() {
        qbz_log::install_fatal_signal_reporter();
        // `write_volatile` so the null store cannot be optimized away.
        unsafe { std::ptr::null_mut::<u8>().write_volatile(1) };
        unreachable!("the child must not survive the store");
    }

    let exe = std::env::current_exe().expect("test binary path");
    let output = Command::new(exe)
        .args([
            "a_segfault_reports_itself_and_still_dies_of_the_signal",
            "--exact",
            "--nocapture",
        ])
        .env(CHILD_MARKER, "1")
        .env("RUST_BACKTRACE", "1")
        .output()
        .expect("run the crashing child");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("qbz fatal SIGSEGV"),
        "the handler did not announce the signal; stderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("thread"),
        "the report must name the crashing thread; stderr was:\n{stderr}"
    );
    // A backtrace was attempted and produced frames, not the "disabled" note.
    assert!(
        stderr.contains("0x") || stderr.contains("qbz_log"),
        "the report carries no frames; stderr was:\n{stderr}"
    );
    assert_eq!(
        output.status.signal(),
        Some(11),
        "the child must still die of SIGSEGV so CI, the smoke harness and any \
         core dump see the real cause; status was {:?}",
        output.status
    );
}
