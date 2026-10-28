//! The alternate signal stack of a thread that runs managed code.
//!
//! .NET turns a fault in managed code, such as reading through a null reference, into an
//! exception from its SIGSEGV handler. The handler runs on the thread's alternate signal
//! stack when one is installed, and the runtime keeps a stack it finds instead of installing
//! its own. Rust's standard library gives every thread a stack of `SIGSTKSZ` bytes (8 KiB on
//! Linux) to report stack overflows. The runtime's handler frames and the kernel's signal frame,
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
        let disabled = libc::stack_t {
            ss_sp: std::ptr::null_mut(),
            ss_flags: libc::SS_DISABLE,
            ss_size: 0,
        };
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
    use super::{MIN_SIGNAL_STACK, release_small_signal_stack};

    fn installed() -> Option<usize> {
        // SAFETY: the query only writes the stack_t passed to it.
        unsafe {
            let mut current: libc::stack_t = std::mem::zeroed();
            assert_eq!(libc::sigaltstack(std::ptr::null(), &raw mut current), 0);
            (current.ss_flags & libc::SS_DISABLE == 0).then_some(current.ss_size)
        }
    }

    #[test]
    fn releases_the_standard_librarys_stack() {
        std::thread::spawn(|| {
            let before = installed().expect("Rust gives each thread an alternate stack");
            assert!(before < MIN_SIGNAL_STACK, "Rust's stack is {before} bytes");
            assert!(release_small_signal_stack());
            assert_eq!(installed(), None);
            assert!(!release_small_signal_stack(), "nothing is left to release");
        })
        .join()
        .unwrap();
    }

    #[test]
    fn keeps_a_stack_large_enough_for_the_runtime() {
        std::thread::spawn(|| {
            let mut memory = vec![0_u8; MIN_SIGNAL_STACK];
            let large = libc::stack_t {
                ss_sp: memory.as_mut_ptr().cast(),
                ss_flags: 0,
                ss_size: memory.len(),
            };
            // SAFETY: the stack stays allocated until it is disabled below.
            unsafe {
                assert_eq!(libc::sigaltstack(&raw const large, std::ptr::null_mut()), 0);
            }
            assert!(!release_small_signal_stack());
            assert_eq!(installed(), Some(MIN_SIGNAL_STACK));
            let disabled = libc::stack_t {
                ss_sp: std::ptr::null_mut(),
                ss_flags: libc::SS_DISABLE,
                ss_size: 0,
            };
            // SAFETY: disabling the stack before its memory is freed.
            unsafe {
                assert_eq!(
                    libc::sigaltstack(&raw const disabled, std::ptr::null_mut()),
                    0
                );
            }
            drop(memory);
        })
        .join()
        .unwrap();
    }
}
