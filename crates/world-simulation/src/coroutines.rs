use crate::{ComponentHandle, TimeSpan, World, WorldError};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CoroutineId {
    pub(crate) world: u64,
    pub(crate) sequence: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitCondition {
    NextFrame,
    FixedUpdate,
    Scaled(TimeSpan),
    Unscaled(TimeSpan),
}
#[derive(Clone)]
pub(crate) struct Coroutine {
    pub script: ComponentHandle,
    pub wait: WaitCondition,
    pub deadline: TimeSpan,
    pub frame: u64,
}
#[derive(Clone, Default)]
pub(crate) struct Coroutines {
    pub next: u64,
    pub entries: BTreeMap<CoroutineId, Coroutine>,
    pub cancelled: Vec<CoroutineId>,
}
impl Coroutines {
    pub fn cancel(&mut self, id: CoroutineId) {
        if self.entries.remove(&id).is_some() {
            self.cancelled.push(id);
        }
    }
    pub fn cancel_script(&mut self, script: ComponentHandle) {
        let ids: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(&id, entry)| (entry.script == script).then_some(id))
            .collect();
        for id in ids {
            self.cancel(id);
        }
    }
    pub fn cancel_all(&mut self) {
        self.cancelled
            .extend(std::mem::take(&mut self.entries).into_keys());
    }
}
impl World {
    pub fn start_coroutine(
        &mut self,
        script: ComponentHandle,
        wait: WaitCondition,
    ) -> Result<CoroutineId, WorldError> {
        self.component(script)?;
        if !self.coroutine_eligible(script) {
            return Err(WorldError::NotReady);
        }
        let next = self
            .coroutines
            .next
            .checked_add(1)
            .ok_or(WorldError::InputCapacity)?;
        let entry = self.coroutine_wait(script, wait)?;
        let id = CoroutineId {
            world: self.identity,
            sequence: next,
        };
        self.coroutines.next = next;
        self.coroutines.entries.insert(id, entry);
        Ok(id)
    }
    pub(crate) fn coroutine_eligible(&self, script: ComponentHandle) -> bool {
        self.lifecycle
            .scripts
            .get(&script)
            .is_some_and(|state| state.awake)
            && self
                .component(script)
                .is_ok_and(|component| self.active_in_hierarchy(component.object).unwrap_or(false))
    }
    pub fn stop_coroutine(&mut self, id: CoroutineId) -> Result<(), WorldError> {
        if id.world != self.identity {
            return Err(WorldError::WrongWorld);
        }
        self.coroutines.cancel(id);
        Ok(())
    }
    /// Transfers cancellation identities to the host, consuming each notification once.
    pub fn take_cancelled_coroutines(&mut self) -> Vec<CoroutineId> {
        std::mem::take(&mut self.coroutines.cancelled)
    }
    pub fn coroutine_count(&self) -> usize {
        self.coroutines.entries.len()
    }
    pub(crate) fn coroutine_wait(
        &self,
        script: ComponentHandle,
        wait: WaitCondition,
    ) -> Result<Coroutine, WorldError> {
        let deadline = match wait {
            WaitCondition::Scaled(span) => self.clock.time.checked_add(span)?,
            WaitCondition::Unscaled(span) => self.clock.unscaled_time.checked_add(span)?,
            WaitCondition::NextFrame | WaitCondition::FixedUpdate => TimeSpan::ZERO,
        };
        Ok(Coroutine {
            script,
            wait,
            deadline,
            frame: self.clock.frame,
        })
    }
}
