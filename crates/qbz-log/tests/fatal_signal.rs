//! Child-process regressions for signal status, real faults and locked loggers.
#![cfg(unix)]

use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_MARKER: &str = "QBZ_LOG_FATAL_TEST_CHILD";

struct MustNotFlush;
impl log::Log for MustNotFlush {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }
    fn log(&self, _: &log::Record<'_>) {}
    fn flush(&self) {
        panic!("a signal handler must not call the logger");
    }
}
static LOGGER: MustNotFlush = MustNotFlush;

#[test]
fn fatal_child() {
    let Ok(mode) = std::env::var(CHILD_MARKER) else {
        return;
    };
    // No core files from deliberately crashing tests.
    unsafe {
        let limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &limit);
    }
    log::set_logger(&LOGGER).unwrap();
    qbz_log::install_fatal_signal_reporter();
    qbz_log::install_fatal_signal_reporter();
    if mode == "stderr-locked" {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _stderr = std::io::stderr().lock();
            tx.send(()).unwrap();
            std::thread::sleep(Duration::from_secs(30));
        });
        rx.recv().unwrap();
    }
    if mode == "memory-fault" {
        unsafe {
            let page = libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            );
            assert_ne!(page, libc::MAP_FAILED);
            // A real protection fault in C; no Rust null-pointer UB.
            libc::memset(page, 1, 4096);
        }
    } else {
        unsafe { libc::raise(libc::SIGSEGV) };
    }
    panic!("child survived fatal signal");
}

fn crash(mode: &str, backtrace: bool) -> String {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["fatal_child", "--exact", "--nocapture"])
        .env(CHILD_MARKER, mode)
        .env_remove("QBZ_FATAL_BACKTRACE")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if backtrace {
        command.env("QBZ_FATAL_BACKTRACE", "1");
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "handler hung ({mode}): {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.signal(), Some(libc::SIGSEGV), "{stderr}");
    assert!(stderr.contains("qbz fatal SIGSEGV"), "{stderr}");
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    assert!(stderr.contains("pc=0x"), "{stderr}");
    stderr
}

#[test]
fn a_segfault_reports_itself_and_still_dies_of_the_signal() {
    let report = crash("memory-fault", false);
    assert!(
        !report.contains("qbz_log::fatal::imp::handle"),
        "default must not unwind"
    );
}

#[test]
fn handler_bypasses_locked_stderr_and_never_flushes_logger() {
    crash("stderr-locked", false);
}

#[test]
fn diagnostic_opt_in_captures_an_actual_frame() {
    let report = crash("raised", true);
    assert!(
        report.contains("qbz_log::fatal::imp::handle"),
        "no actual handler frame: {report}"
    );
}
