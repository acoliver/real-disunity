use std::collections::VecDeque;

use super::{Scope, Task};
use crate::{Advance, World, WorldError};

impl World {
    /// Runs the `OnDisable` and `OnDestroy` messages and the asset cleanup that destruction has
    /// queued, and the cleanup of any object those messages destroy, in one advance. It runs no
    /// `Start`, fixed step, physics, `Update`, coroutine, delayed task, `LateUpdate` or
    /// `WillRender` phase, leaves the clock and frame count as they are and begins no graphics
    /// frame. Hosts use it while tearing a world down, where the objects still alive must not
    /// receive per-frame messages.
    ///
    /// Destruction that is due by time (`DestroyWhen::AfterScaled`) is not expired here: the
    /// clock does not move, and a host that tears the world down destroys what remains itself.
    /// Like `advance`, it is refused while another advance is in progress and on a faulted
    /// world.
    pub fn advance_teardown(&mut self) -> Result<Advance, WorldError> {
        self.require_idle()?;
        self.scheduler.fixed_steps = 0;
        self.scheduler.scopes.push(Scope {
            tasks: VecDeque::from([Task::Cleanup]),
            callback: None,
            lifecycle: false,
        });
        self.poll_scheduler()
    }
}
