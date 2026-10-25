use crate::{ComponentHandle, FieldValue, ObjectHandle, World, WorldError};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MutationId {
    world: u64,
    pub(crate) sequence: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptMessage {
    Start,
    FixedUpdate,
    Update,
    LateUpdate,
    Coroutine(crate::CoroutineId),
    Contact2d(crate::ContactId),
    Awake,
    OnEnable,
    OnDisable,
    OnDestroy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LifecycleWork {
    pub script: ComponentHandle,
    pub message: ScriptMessage,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartState {
    Pending,
    Started,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnteredState {
    Dormant,
    Awake,
    Enabled,
}

#[derive(Clone)]
pub(crate) struct ScriptState {
    pub start: StartState,
    pub fields: Vec<(String, FieldValue)>,
    pub enabled: bool,
    pub awake: bool,
    pub active: bool,
    pub entered: EnteredState,
}

impl Default for ScriptState {
    fn default() -> Self {
        Self {
            start: StartState::Pending,
            fields: Vec::new(),
            enabled: true,
            awake: false,
            active: false,
            entered: EnteredState::Dormant,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Lifecycle {
    pub scripts: BTreeMap<ComponentHandle, ScriptState>,
    pub(crate) mutations: BTreeMap<usize, Vec<LifecycleWork>>,
    next_sequence: usize,
    pub(crate) unpublished: BTreeMap<usize, Vec<ObjectHandle>>,
    pub cleanup: Vec<LifecycleWork>,
}

impl World {
    pub(crate) fn is_unpublished(&self, object: ObjectHandle) -> bool {
        self.lifecycle
            .unpublished
            .values()
            .any(|nodes| nodes.contains(&object))
    }

    /// Immediate native ordering data. E4 invokes managed work before returning a clone.
    pub fn lifecycle_work(&self, mutation: MutationId) -> Result<&[LifecycleWork], WorldError> {
        if mutation.world != self.identity {
            return Err(WorldError::WrongMutation);
        }
        self.lifecycle
            .mutations
            .get(&mutation.sequence)
            .map(Vec::as_slice)
            .ok_or(WorldError::WrongMutation)
    }

    /// Transfers a batch to its caller and releases native ownership of its storage.
    pub fn take_lifecycle_work(
        &mut self,
        mutation: MutationId,
    ) -> Result<Vec<LifecycleWork>, WorldError> {
        if mutation.world != self.identity {
            return Err(WorldError::WrongMutation);
        }
        self.lifecycle
            .mutations
            .remove(&mutation.sequence)
            .ok_or(WorldError::WrongMutation)
    }

    pub(crate) fn reserve_mutation(&mut self) -> Result<MutationId, WorldError> {
        let sequence = self.lifecycle.next_sequence;
        self.lifecycle.next_sequence = sequence
            .checked_add(1)
            .ok_or(WorldError::CapacityExceeded(crate::LimitKind::Generation))?;
        Ok(MutationId {
            world: self.identity,
            sequence,
        })
    }

    pub fn script_ready(&self, script: ComponentHandle) -> Result<bool, WorldError> {
        self.component(script)?;
        let state = self
            .lifecycle
            .scripts
            .get(&script)
            .ok_or(WorldError::NotScript)?;
        Ok(state.awake
            && state.active
            && self.active_in_hierarchy(self.component(script)?.object)?)
    }

    pub fn script_enabled(&self, script: ComponentHandle) -> Result<bool, WorldError> {
        self.component(script)?;
        self.lifecycle
            .scripts
            .get(&script)
            .map(|state| state.enabled)
            .ok_or(WorldError::NotScript)
    }

    pub fn set_script_enabled(
        &mut self,
        script: ComponentHandle,
        enabled: bool,
    ) -> Result<MutationId, WorldError> {
        let object = self.component(script)?.object;
        if !self.lifecycle.scripts.contains_key(&script) {
            return Err(WorldError::NotScript);
        }
        let mutation = self.reserve_mutation()?;
        self.lifecycle
            .scripts
            .get_mut(&script)
            .expect("validated script")
            .enabled = enabled;
        self.refresh_lifecycle(&[object], mutation);
        Ok(mutation)
    }

    pub fn take_cleanup(&mut self) -> Vec<LifecycleWork> {
        std::mem::take(&mut self.lifecycle.cleanup)
    }

    pub(crate) fn refresh_lifecycle(&mut self, nodes: &[ObjectHandle], mutation: MutationId) {
        let mut awake = Vec::new();
        let mut transitions = Vec::new();
        for &object in nodes {
            if self.is_unpublished(object) {
                continue;
            }
            let active = self.active_in_hierarchy(object).expect("live hierarchy");
            let scripts: Vec<_> = self
                .lifecycle
                .scripts
                .keys()
                .copied()
                .filter(|&script| self.component(script).is_ok_and(|c| c.object == object))
                .collect();
            for script in scripts {
                let state = self
                    .lifecycle
                    .scripts
                    .get_mut(&script)
                    .expect("registered script");
                if active && !state.awake {
                    state.awake = true;
                    awake.push(LifecycleWork {
                        script,
                        message: ScriptMessage::Awake,
                    });
                }
                let eligible = active && state.enabled;
                if !active {
                    self.coroutines.cancel_script(script);
                }
                if eligible != state.active {
                    state.active = eligible;
                    let message = if eligible {
                        ScriptMessage::OnEnable
                    } else {
                        ScriptMessage::OnDisable
                    };
                    transitions.push(LifecycleWork { script, message });
                }
            }
        }
        awake.extend(transitions);
        self.lifecycle.mutations.insert(mutation.sequence, awake);
    }
}

#[cfg(test)]
mod tests {
    use crate::{ComponentDesc, DestroyWhen, LimitKind};
    use crate::{ObjectDesc, World, WorldError, WorldSettings};

    #[test]
    fn b4_clone_batches_and_retired_script_data_are_reclaimed() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let template = world.create_object(ObjectDesc::default()).unwrap();
        world
            .add_component(
                template,
                ComponentDesc::Script {
                    assembly: "Example".into(),
                    type_name: "Actor".into(),
                },
            )
            .unwrap();
        for _ in 0..128 {
            let clone = world.instantiate(template, None).unwrap();
            assert_eq!(world.take_lifecycle_work(clone.mutation).unwrap().len(), 2);
            world.destroy(clone.root, DestroyWhen::EndOfFrame).unwrap();
            assert_eq!(world.take_cleanup().len(), 2);
            world.finish_phase();
            assert!(world.lifecycle.mutations.is_empty());
            assert_eq!(world.lifecycle.scripts.len(), 1);
        }
    }

    #[test]
    fn b4_exhausted_sequences_fail_before_mutating_activation() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world.lifecycle.next_sequence = usize::MAX;
        assert_eq!(
            world.set_active(object, false),
            Err(WorldError::CapacityExceeded(LimitKind::Generation))
        );
        assert!(world.object(object).unwrap().active);
        assert!(world.lifecycle.mutations.is_empty());
    }

    #[test]
    fn b4_consumed_batches_do_not_retain_world_history() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let outstanding = world.set_active(object, true).unwrap();
        for _ in 0..1024 {
            let mutation = world.set_active(object, true).unwrap();
            assert!(world.take_lifecycle_work(mutation).unwrap().is_empty());
            assert_eq!(
                world.lifecycle_work(mutation),
                Err(WorldError::WrongMutation)
            );
            world.finish_phase();
            assert_eq!(world.lifecycle.mutations.len(), 1);
        }
        assert!(world.take_lifecycle_work(outstanding).unwrap().is_empty());
        assert!(world.lifecycle.mutations.is_empty());
        let next = world.set_active(object, false).unwrap();
        assert_ne!(next, outstanding);
        assert_eq!(
            world.take_lifecycle_work(outstanding),
            Err(WorldError::WrongMutation)
        );
        assert!(world.take_lifecycle_work(next).unwrap().is_empty());
    }
}
