//! The native response table is process-wide: 1,024 live handles shared by every world.
//! A test that fills it makes requests in any concurrently running test of the same
//! binary fail with a budget error. Every test in a binary that fills the table holds
//! this lock for its whole body, so those tests run one at a time.

use std::sync::{Mutex, MutexGuard, PoisonError};

static TABLE: Mutex<()> = Mutex::new(());

/// Exclusive use of the process-wide response table for the rest of the test.
pub fn exclusive() -> MutexGuard<'static, ()> {
    // A failed test must not turn every later test into a poisoned-lock failure.
    TABLE.lock().unwrap_or_else(PoisonError::into_inner)
}
