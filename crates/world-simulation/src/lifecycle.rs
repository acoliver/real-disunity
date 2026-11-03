use crate::{ComponentHandle, FieldValue, ObjectHandle, World, WorldError};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MutationId {
    pub(crate) world: u64,
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
    pub messages: crate::ScriptMessages,
}

impl ScriptState {
    /// Applies the object's activity to the script. Returns whether the script woke, and its new
    /// activity when that changed.
    fn refresh(&mut self, in_active_hierarchy: bool) -> (bool, Option<bool>) {
        let woke = in_active_hierarchy && !self.awake;
        if woke {
            self.awake = true;
        }
        let eligible = in_active_hierarchy && self.enabled;
        let changed = (eligible != self.active).then_some(eligible);
        self.active = eligible;
        (woke, changed)
    }
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
            messages: crate::ScriptMessages::ALL,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Lifecycle {
    pub scripts: BTreeMap<ComponentHandle, ScriptState>,
    /// Scripts registered since the scheduler last looked for new ones, in registration order.
    /// It may name scripts removed since; readers skip those.
    pub(crate) added: Vec<ComponentHandle>,
    pub(crate) mutations: BTreeMap<usize, Vec<LifecycleWork>>,
    next_sequence: usize,
    pub(crate) unpublished: crate::unpublished::Unpublished,
    pub cleanup: Vec<LifecycleWork>,
    /// Script types by assembly and name, with the per-frame messages reported for each.
    pub(crate) types: crate::script_messages::ScriptTypes,
    /// The scripts a phase visits, kept equal to a scan of `scripts` by the methods below. Change
    /// a script's `start`, `awake`, `active`, `messages` or presence only through them.
    index: crate::phase_index::PhaseIndex,
}

impl Lifecycle {
    /// Stores a script's state and notes it for the scheduler's next look at new scripts.
    pub(crate) fn register(&mut self, script: ComponentHandle, state: ScriptState) {
        self.remove_script(script);
        self.index.add(script, crate::phase_index::Key::of(&state));
        self.scripts.insert(script, state);
        self.added.push(script);
    }

    /// Forgets a removed or retired script.
    pub(crate) fn remove_script(&mut self, script: ComponentHandle) {
        if let Some(state) = self.scripts.remove(&script) {
            self.index
                .drop_entries(script, crate::phase_index::Key::of(&state));
        }
    }

    /// Changes a registered script's state and keeps the phase index equal to it. Change `start`,
    /// `awake`, `active` and `messages` only here.
    pub(crate) fn edit_script<R>(
        &mut self,
        script: ComponentHandle,
        change: impl FnOnce(&mut ScriptState) -> R,
    ) -> Option<R> {
        let state = self.scripts.get_mut(&script)?;
        let before = crate::phase_index::Key::of(state);
        let result = change(state);
        self.index
            .retarget(script, before, crate::phase_index::Key::of(state));
        Some(result)
    }

    /// Records that the script's `Start` was issued.
    pub(crate) fn mark_started(&mut self, script: ComponentHandle) {
        self.edit_script(script, |state| state.start = StartState::Started)
            .expect("ready script");
    }

    /// Gives a registered script the per-frame messages its type defines.
    pub(crate) fn set_messages(
        &mut self,
        script: ComponentHandle,
        messages: crate::ScriptMessages,
    ) {
        self.edit_script(script, |state| state.messages = messages);
    }

    /// The scripts a phase for `message` has to look at, in handle order.
    pub(crate) fn phase_candidates(&self, message: crate::ScriptMessage) -> Vec<ComponentHandle> {
        match self.index.visit(message) {
            crate::phase_index::Visit::Listed(scripts) => scripts,
            crate::phase_index::Visit::All => self.scripts.keys().copied().collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn assert_index_matches_scripts(&self) {
        assert!(self.index.matches(&self.scripts), "phase index drifted");
    }
}

impl World {
    pub(crate) fn is_unpublished(&self, object: ObjectHandle) -> bool {
        self.lifecycle.unpublished.contains_object(object)
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
        Ok(!self.lifetime.components.contains(&script)
            && state.awake
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
        let mut work = std::mem::take(&mut self.lifecycle.cleanup);
        work.sort_by_key(|work| work.script);
        work
    }

    pub(crate) fn refresh_lifecycle(&mut self, nodes: &[ObjectHandle], mutation: MutationId) {
        let mut awake = Vec::new();
        let mut transitions = Vec::new();
        for &object in nodes {
            if self.is_unpublished(object) || self.lifetime.objects.contains(&object) {
                continue;
            }
            let active = self.active_in_hierarchy(object).expect("live hierarchy");
            for script in self.object_scripts(object) {
                let (woke, changed) = self
                    .lifecycle
                    .edit_script(script, |state| state.refresh(active))
                    .expect("registered script");
                if woke {
                    awake.push(LifecycleWork {
                        script,
                        message: ScriptMessage::Awake,
                    });
                }
                if !active {
                    self.coroutines.cancel_script(script);
                }
                if let Some(eligible) = changed {
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

    /// The object's registered, unretired scripts in handle order, the order of the script table.
    /// Reading the object's own component list keeps a refresh proportional to the objects it visits.
    fn object_scripts(&self, object: ObjectHandle) -> Vec<ComponentHandle> {
        let Ok(stored) = self.live_object(object) else {
            return Vec::new();
        };
        let mut scripts: Vec<_> = stored
            .components
            .iter()
            .copied()
            .filter(|script| {
                self.lifecycle.scripts.contains_key(script)
                    && !self.lifetime.components.contains(script)
                    && self.component(*script).is_ok_and(|c| c.object == object)
            })
            .collect();
        scripts.sort_unstable();
        scripts
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
