//! The host's listing of the scheduler's threads.

use mlua::Thread;

use crate::original::GameScripts;

/// What a listed thread is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThreadWait {
    /// Due now and not waiting: a thread that has not run yet, or one a wake made due.
    Ready,
    /// Created, or sleeping for zero, and waiting for the next advance.
    Deferred,
    /// Asleep until its wake time, or until woken when it has none.
    Sleep,
    /// In the wait whose native is named, until woken or until its timeout.
    Wait(String),
}

/// A live scheduler thread as the host sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct ThreadInfo {
    pub thread: Thread,
    /// The name of the thread's clock.
    pub clock: String,
    /// The game's flags on the thread.
    pub flags: i64,
    /// The game's source name for the thread, empty when it has none.
    pub source: String,
    pub wait: ThreadWait,
    /// The wake time on the thread's own clock in milliseconds. `None` when it waits without a
    /// timeout.
    pub due: Option<u64>,
    /// The order threads were created in, which breaks ties between threads due together.
    pub order: u64,
}

impl GameScripts {
    /// Every live thread in the order it was created. A thread that finished, failed or was
    /// killed is not listed.
    pub fn threads(&self) -> Vec<ThreadInfo> {
        self.scheduler().lock().expect("scheduler").infos()
    }
}
