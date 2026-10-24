use std::cell::Cell;

use super::CompatibilityBridge;
use crate::HostError;

pub(super) struct DispatchScope<'a>(&'a Cell<usize>);

impl Drop for DispatchScope<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

impl CompatibilityBridge {
    pub(super) fn require_idle(&self) -> Result<(), HostError> {
        if self.dispatch_depth.get() != 0 {
            return Err(HostError::Compatibility(
                "managed dispatch is nonreentrant".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn enter_dispatch(&self) -> Result<DispatchScope<'_>, HostError> {
        self.require_idle()?;
        self.dispatch_depth.set(1);
        Ok(DispatchScope(&self.dispatch_depth))
    }
}
