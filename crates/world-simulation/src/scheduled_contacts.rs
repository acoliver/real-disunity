use crate::{
    LifecycleWork, ScriptMessage, World, WorldError, physics2d::ContactEvent, timeline::Task,
};
use std::collections::BTreeSet;

/// Identifies one contact in the currently dispatched fixed step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactId {
    world: u64,
    fixed_frame: u64,
    pub(crate) index: usize,
}
impl World {
    pub fn contact_event(&self, id: ContactId) -> Result<&ContactEvent, WorldError> {
        if id.world != self.identity {
            return Err(WorldError::WrongWorld);
        }
        if id.fixed_frame != self.clock.fixed_frame {
            return Err(WorldError::InvalidContact);
        }
        self.contacts()
            .get(id.index)
            .ok_or(WorldError::InvalidContact)
    }
    pub(crate) fn contact_tasks(&self) -> Vec<Task> {
        let mut work = Vec::new();
        for (index, contact) in self.contacts().iter().enumerate() {
            let mut targets = BTreeSet::from([contact.object_a, contact.object_b]);
            for body in [contact.body_a, contact.body_b].into_iter().flatten() {
                if let Ok(component) = self.component(body) {
                    targets.insert(component.object);
                }
            }
            for &script in self.lifecycle.scripts.keys() {
                if self
                    .component(script)
                    .is_ok_and(|component| targets.contains(&component.object))
                {
                    let id = ContactId {
                        world: self.identity,
                        fixed_frame: self.clock.fixed_frame,
                        index,
                    };
                    work.push(Task::Work(LifecycleWork {
                        script,
                        message: ScriptMessage::Contact2d(id),
                    }));
                }
            }
        }
        work
    }
}
