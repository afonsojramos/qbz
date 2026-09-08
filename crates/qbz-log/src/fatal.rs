//! Best-effort fatal-signal evidence on stderr, preserving the fatal status.
//!
//! The ordinary handler uses a stack buffer and write(2), never the logger,
//! Rust stderr locks, allocation, TLS thread names or a stack unwinder.
//! On Linux x86_64/aarch64 it also records the interrupted program counter.
//! Symbolication needs the matching binary and process mappings/core.
//!
//! QBZ_FATAL_BACKTRACE=1 explicitly opts into an UNSAFE diagnostic backtrace
//! for isolated CI runs. Unwinding in a signal handler can deadlock in malloc
//! or the loader. A ten-second alarm bounds that attempt under the ordinary
//! SIGALRM disposition; if it fires, the exit signal is SIGALRM, not the
//! original fault. This mode is never enabled by default for users.
//! Evidence goes to stderr, not the ring or qbz.log. A full/broken stderr
//! destination can still prevent output; this is not a crash-dump service.

#[cfg(unix)]
mod imp {
    use std::fmt::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Once;

    static IN_HANDLER: AtomicBool = AtomicBool::new(false);
    static BACKTRACE: AtomicBool = AtomicBool::new(false);
    static INSTALL: Once = Once::new();

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

    struct Report {
        bytes: [u8; 256],
        len: usize,
    }

    impl std::fmt::Write for Report {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            let count = text.len().min(self.bytes.len() - self.len);
            self.bytes[self.len..self.len + count].copy_from_slice(&text.as_bytes()[..count]);
            self.len += count;
            Ok(())
        }
    }

    fn emit(bytes: &[u8]) {
        // No stdio or logger mutex. Best effort: do not retry a failed write.
        unsafe { libc::write(libc::STDERR_FILENO, bytes.as_ptr().cast(), bytes.len()) };
    }

    unsafe fn program_counter(context: *mut libc::c_void) -> Option<usize> {
        if context.is_null() {
            return None;
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            let context = &*context.cast::<libc::ucontext_t>();
            return Some(context.uc_mcontext.gregs[libc::REG_RIP as usize] as usize);
        }
        #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
        {
            let context = &*context.cast::<libc::ucontext_t>();
            return Some(context.uc_mcontext.pc as usize);
        }
        #[allow(unreachable_code)]
        None
    }

    extern "C" fn handle(
        signum: libc::c_int,
        info: *mut libc::siginfo_t,
        context: *mut libc::c_void,
    ) {
        if IN_HANDLER.swap(true, Ordering::SeqCst) {
            restore_and_reraise(signum);
            return;
        }
        let backtrace = BACKTRACE.load(Ordering::Relaxed);
        if backtrace {
            unsafe { libc::alarm(10) };
        }
        // SA_SIGINFO supplies valid pointers for this invocation.
        let fault_addr = unsafe { info.as_ref() }
            .map(|info| unsafe { info.si_addr() } as usize)
            .unwrap_or(0);
        let mut report = Report {
            bytes: [0; 256],
            len: 0,
        };
        let _ = write!(
            report,
            "\n=== qbz fatal {} (signal {}) at {:#x}",
            signal_name(signum),
            signum,
            fault_addr
        );
        if let Some(pc) = unsafe { program_counter(context) } {
            let _ = write!(report, " pc={pc:#x}");
        }
        let _ = writeln!(report, " ===");
        emit(&report.bytes[..report.len]);

        if backtrace {
            // Deliberately opt-in: neither capture nor formatting is signal-safe.
            emit(format!("{}\n", std::backtrace::Backtrace::force_capture()).as_bytes());
        }
        // Never flush the logger here: the fault may have interrupted it while
        // holding its non-reentrant output mutex.
        restore_and_reraise(signum);
    }

    fn restore_and_reraise(signum: libc::c_int) {
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = libc::SIG_DFL;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signum, &action, std::ptr::null_mut());
            // The current signal is blocked inside its handler. It is delivered
            // with SIG_DFL when the handler returns and restores the mask.
            libc::raise(signum);
        }
    }

    fn install_alt_stack() {
        // Alternate stacks are PER THREAD. This covers the installing thread,
        // not every worker. Reuse a sufficiently large existing alternate stack.
        unsafe {
            let mut current: libc::stack_t = std::mem::zeroed();
            if libc::sigaltstack(std::ptr::null(), &mut current) != 0
                || (current.ss_flags & libc::SS_DISABLE == 0
                    && current.ss_size >= libc::SIGSTKSZ.max(64 * 1024))
            {
                return;
            }
            let size = libc::SIGSTKSZ.max(64 * 1024);
            let mut stack = vec![0u8; size].into_boxed_slice();
            let alt = libc::stack_t {
                ss_sp: stack.as_mut_ptr().cast(),
                ss_flags: 0,
                ss_size: size,
            };
            if libc::sigaltstack(&alt, std::ptr::null_mut()) == 0 {
                // The kernel retains the pointer until this thread exits.
                let _ = Box::leak(stack);
            }
        }
    }

    pub fn install() {
        install_alt_stack();
        INSTALL.call_once(|| {
            BACKTRACE.store(
                std::env::var("QBZ_FATAL_BACKTRACE").as_deref() == Ok("1"),
                Ordering::Relaxed,
            );
            for signum in [
                libc::SIGSEGV,
                libc::SIGBUS,
                libc::SIGILL,
                libc::SIGFPE,
                libc::SIGABRT,
            ] {
                unsafe {
                    let mut action: libc::sigaction = std::mem::zeroed();
                    action.sa_sigaction = handle as *const () as usize;
                    action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
                    libc::sigemptyset(&mut action.sa_mask);
                    libc::sigaction(signum, &action, std::ptr::null_mut());
                }
            }
        });
    }
}

#[cfg(not(unix))]
mod imp {
    pub fn install() {}
}

/// Install process-wide handlers once and an alternate stack on this thread
/// if it has no sufficiently large one. No-op off Unix. Call after logger installation.
pub fn install() {
    imp::install();
}
