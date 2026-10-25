//! A process admits one live managed host. Unit tests that boot a session through
//! `Playback` hold this lock, so they run one at a time under parallel test threads.

use std::sync::{Mutex, MutexGuard, PoisonError};

static HOST: Mutex<()> = Mutex::new(());

pub fn one_host_at_a_time() -> MutexGuard<'static, ()> {
    // A failed test must not turn every later test into a poisoned-lock failure.
    HOST.lock().unwrap_or_else(PoisonError::into_inner)
}
