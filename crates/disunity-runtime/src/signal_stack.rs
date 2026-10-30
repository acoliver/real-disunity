//! The alternate signal stack of a thread that runs managed code.
//!
//! .NET turns a fault in managed code, such as reading through a null reference, into an
//! exception from its SIGSEGV handler. The handler runs on the thread's alternate signal
//! stack when one is installed, and the runtime keeps a stack it finds instead of installing
//! its own. Rust's standard library gives every thread a stack of `SIGSTKSZ` bytes to report
//! stack overflows: 8 KiB on Linux and 128 KiB on macOS, where nothing needs releasing. The runtime's handler frames and the kernel's signal frame,
//! which is larger on CPUs with AVX-512 state, do not fit in 8 KiB, so the first
//! `NullReferenceException` crashed the process.
//!
//! [`release_small_signal_stack`] removes a stack smaller than [`MIN_SIGNAL_STACK`] from the
//! current thread before the host uses the runtime on it. The runtime then installs its own
//! stack on the thread it starts on; on other threads the handler runs on the thread's stack.
//! Rust's standard library frees its stack when the thread exits whether or not it is installed.

/// The smallest alternate signal stack left in place. The runtime installs 16 KiB stacks; a
/// smaller stack is Rust's default.
pub const MIN_SIGNAL_STACK: usize = 16 * 1024;

/// A request that disables the current thread's alternate signal stack. macOS rejects a
/// disabling request whose size is below `MINSIGSTKSZ`, so the size is a valid one even though
/// a disabled stack is never used.
#[cfg(unix)]
fn disabling() -> libc::stack_t {
    libc::stack_t {
        ss_sp: std::ptr::null_mut(),
        ss_flags: libc::SS_DISABLE,
        ss_size: libc::SIGSTKSZ,
    }
}

/// Removes the current thread's alternate signal stack if it is smaller than
/// [`MIN_SIGNAL_STACK`]. Returns whether a stack was removed.
#[cfg(unix)]
pub fn release_small_signal_stack() -> bool {
    // SAFETY: sigaltstack reads and writes only the stack_t values passed to it, and a
    // disabled stack is never used, so removing it cannot leave a handler on freed memory.
    unsafe {
        let mut current: libc::stack_t = std::mem::zeroed();
        if libc::sigaltstack(std::ptr::null(), &raw mut current) != 0 {
            return false;
        }
        if current.ss_flags & libc::SS_DISABLE != 0 || current.ss_size >= MIN_SIGNAL_STACK {
            return false;
        }
        let disabled = disabling();
        libc::sigaltstack(&raw const disabled, std::ptr::null_mut()) == 0
    }
}

/// Windows has no alternate signal stacks.
#[cfg(not(unix))]
pub fn release_small_signal_stack() -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::{MIN_SIGNAL_STACK, disabling, release_small_signal_stack};

    /// Fails with the OS error when `sigaltstack` does.
    fn require_ok(result: libc::c_int, what: &str) {
        assert_eq!(result, 0, "{what}: {}", std::io::Error::last_os_error());
    }

    fn installed() -> Option<usize> {
        // SAFETY: the query only writes the stack_t passed to it.
        unsafe {
            let mut current: libc::stack_t = std::mem::zeroed();
            require_ok(
                libc::sigaltstack(std::ptr::null(), &raw mut current),
                "query the alternate stack",
            );
            (current.ss_flags & libc::SS_DISABLE == 0).then_some(current.ss_size)
        }
    }

    /// Rust's stack is released where it is smaller than the runtime needs (Linux) and kept
    /// where it is already large enough (macOS).
    #[test]
    fn releases_the_standard_librarys_stack_only_when_it_is_small() {
        std::thread::spawn(|| {
            let before = installed().expect("Rust gives each thread an alternate stack");
            let small = before < MIN_SIGNAL_STACK;
            assert_eq!(
                release_small_signal_stack(),
                small,
                "Rust's stack is {before} bytes"
            );
            assert_eq!(installed(), (!small).then_some(before));
            assert!(!release_small_signal_stack(), "nothing is left to release");
        })
        .join()
        .unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rusts_stack_on_linux_is_too_small_for_the_runtime() {
        std::thread::spawn(|| {
            let before = installed().expect("Rust gives each thread an alternate stack");
            assert!(before < MIN_SIGNAL_STACK, "Rust's stack is {before} bytes");
        })
        .join()
        .unwrap();
    }

    #[test]
    fn keeps_a_stack_large_enough_for_the_runtime() {
        // Large enough for every platform's minimum alternate stack size, which is above
        // MIN_SIGNAL_STACK on macOS.
        const LARGE: usize = 256 * 1024;
        std::thread::spawn(|| {
            let mut memory = vec![0_u8; LARGE];
            let large = libc::stack_t {
                ss_sp: memory.as_mut_ptr().cast(),
                ss_flags: 0,
                ss_size: memory.len(),
            };
            // SAFETY: the stack stays allocated until it is disabled below.
            unsafe {
                require_ok(
                    libc::sigaltstack(&raw const large, std::ptr::null_mut()),
                    "install a large stack",
                );
            }
            assert!(!release_small_signal_stack());
            assert_eq!(installed(), Some(LARGE));
            let disabled = disabling();
            // SAFETY: disabling the stack before its memory is freed.
            unsafe {
                require_ok(
                    libc::sigaltstack(&raw const disabled, std::ptr::null_mut()),
                    "disable the large stack",
                );
            }
            assert_eq!(installed(), None, "the stack is disabled");
            drop(memory);
        })
        .join()
        .unwrap();
    }
}
