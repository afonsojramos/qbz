//! Fatal-signal handler: turn a bare `-11` into a named stack.
//!
//! WHY THIS EXISTS. The packaged AppDir has an intermittent SIGSEGV during
//! startup — inside the window between `QGuiApplication` construction and the
//! QML engine coming up, with ~30 threads (tokio workers, the audio thread,
//! the loudness analyzer, the visualizer producer, connectivity) already
//! running beside it. Observed twice on 2026-09-08 on different commits: once
//! on the `v2.1.0` tag's own `release-flatpak` attempt 1, which then passed on
//! a plain re-run of the identical commit, and once on the #745 bugfix branch.
//! Every one of those left exactly the same evidence: `status -11` and a log
//! that stops mid-startup. Nothing to act on.
//!
//! Re-running is not a fix and a user who hits this cannot re-run anything, so
//! the process now reports its own death: signal, faulting address, thread,
//! and a backtrace, on stderr and in the log file. The next occurrence names
//! the frame instead of costing another round trip.
//!
//! SAFETY POSTURE. A crash handler must never make things worse:
//!
//!   * A dedicated signal stack (`sigaltstack`) so a stack-overflow SIGSEGV
//!     still has room to run this.
//!   * A re-entrancy latch: a fault raised *by the handler* takes the default
//!     disposition instead of recursing.
//!   * `alarm()` armed FIRST. Capturing a backtrace allocates and takes the
//!     dynamic-loader lock, neither of which is async-signal-safe, so a crash
//!     inside `malloc` can deadlock here. The alarm guarantees the process
//!     still dies, and dies promptly, instead of turning a crash into a hang.
//!   * The signal is re-raised with the default handler restored, so the wait
//!     status the parent sees is unchanged (`-11` stays `-11`) and a core is
//!     still produced where cores are enabled.
//!
//! Best effort by construction: if the backtrace cannot be taken we still get
//! the signal, the address and the thread, which is already more than a bare
//! exit status.

#[cfg(unix)]
mod imp {
    use std::backtrace::Backtrace;
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Set once the handler is running, so a fault inside the handler falls
    /// through to the default disposition instead of recursing forever.
    static IN_HANDLER: AtomicBool = AtomicBool::new(false);

    /// Upper bound on the whole handler. Ten seconds is far longer than a
    /// backtrace needs and far shorter than any smoke deadline.
    const HANDLER_DEADLINE_SECS: libc::c_uint = 10;

    fn signal_name(signum: libc::c_int) -> &'static str {
        match signum {
            libc::SIGSEGV => "SIGSEGV",
            libc::SIGBUS => "SIGBUS",
            libc::SIGILL => "SIGILL",
            libc::SIGFPE => "SIGFPE",
            libc::SIGABRT => "SIGABRT",
            _ => "signal",
        }
    }

    /// Write to fd 2 directly. `println!` locks stdout and can deadlock if the
    /// crashing thread already held that lock.
    fn emit(text: &str) {
        let mut stderr = std::io::stderr();
        let _ = stderr.write_all(text.as_bytes());
        let _ = stderr.flush();
    }

    extern "C" fn handle(
        signum: libc::c_int,
        info: *mut libc::siginfo_t,
        _context: *mut libc::c_void,
    ) {
        if IN_HANDLER.swap(true, Ordering::SeqCst) {
            // Faulted while reporting a fault. Stop trying.
            restore_and_reraise(signum);
            return;
        }

        // FIRST, before anything that can block: guarantee an exit.
        // SAFETY: `alarm` is async-signal-safe and only arms a timer.
        unsafe { libc::alarm(HANDLER_DEADLINE_SECS) };

        // SAFETY: the kernel hands us a valid `siginfo_t` for these signals.
        let fault_addr = unsafe { info.as_ref() }
            .map(|info| unsafe { info.si_addr() } as usize)
            .unwrap_or(0);

        emit(&format!(
            "\n=== qbz fatal {} (code {}) at {:#x}, thread {:?} ===\n",
            signal_name(signum),
            signum,
            fault_addr,
            std::thread::current().name().unwrap_or("<unnamed>"),
        ));
        // Allocates and takes loader locks. Guarded by the alarm above.
        emit(&format!("{}\n", Backtrace::force_capture()));

        // Push whatever the file sink still holds, so the log ends at the
        // crash instead of up to FILE_FLUSH_INTERVAL before it.
        log::logger().flush();

        restore_and_reraise(signum);
    }

    fn restore_and_reraise(signum: libc::c_int) {
        // SAFETY: restoring SIG_DFL and re-raising on the current thread is
        // the documented way to keep the original wait status and core file.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signum, &action, std::ptr::null_mut());
            libc::raise(signum);
        }
    }

    /// Give the handler its own stack, so a stack-overflow SIGSEGV can run it.
    fn install_alt_stack() {
        // Leaked on purpose: the handler may run at any point in the process
        // lifetime, including after main's locals are gone.
        let size = libc::SIGSTKSZ.max(64 * 1024);
        let stack = vec![0u8; size].into_boxed_slice();
        let stack = Box::leak(stack);
        // SAFETY: `stack` is a live, uniquely owned, leaked allocation of
        // exactly `size` bytes.
        unsafe {
            let alt = libc::stack_t {
                ss_sp: stack.as_mut_ptr().cast(),
                ss_flags: 0,
                ss_size: size,
            };
            libc::sigaltstack(&alt, std::ptr::null_mut());
        }
    }

    pub fn install() {
        install_alt_stack();
        for signum in [
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGILL,
            libc::SIGFPE,
            libc::SIGABRT,
        ] {
            // SAFETY: a zeroed `sigaction` with SA_SIGINFO and our `extern "C"`
            // handler is the standard installation; SA_ONSTACK pairs with the
            // alternate stack above.
            unsafe {
                let mut action: libc::sigaction = std::mem::zeroed();
                action.sa_sigaction = handle as *const () as usize;
                action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
                libc::sigemptyset(&mut action.sa_mask);
                libc::sigaction(signum, &action, std::ptr::null_mut());
            }
        }
    }
}

#[cfg(not(unix))]
mod imp {
    pub fn install() {}
}

/// Install the fatal-signal reporter. Idempotent in practice (re-installing
/// the same handler is a no-op) and a no-op off unix. Call as early in `main`
/// as possible, right after the logger: everything that crashes later is then
/// covered, including the whole Qt/QML startup window.
pub fn install() {
    imp::install();
}

#[cfg(all(test, unix))]
mod tests {
    /// The handler must survive being installed twice and must not disturb an
    /// ordinary process: the interesting behaviour is only reachable by
    /// actually faulting, which a test process must not do.
    #[test]
    fn installing_is_idempotent_and_harmless() {
        super::install();
        super::install();
        // Still alive, still able to allocate and log.
        log::logger().flush();
    }
}
