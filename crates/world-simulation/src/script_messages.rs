//! Which per-frame messages a script type defines.
//!
//! The engine's choice is to call `FixedUpdate`, `Update` and `LateUpdate` only on scripts whose
//! type defines them, so a host reports each type's set once and the timeline skips the rest.

use crate::{ComponentDesc, ComponentHandle, World, WorldError};
use std::collections::{BTreeMap, BTreeSet};

/// The per-frame messages a script type defines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptMessages(u32);

impl ScriptMessages {
    pub const NONE: Self = Self(0);
    pub const FIXED_UPDATE: Self = Self(1);
    pub const UPDATE: Self = Self(2);
    pub const LATE_UPDATE: Self = Self(4);
    pub const ALL: Self = Self(7);

    /// Returns the set for `bits`, or `None` when it names an unknown message.
    pub fn from_bits(bits: u32) -> Option<Self> {
        (bits & !Self::ALL.0 == 0).then_some(Self(bits))
    }
    pub fn bits(self) -> u32 {
        self.0
    }
    pub fn contains(self, messages: Self) -> bool {
        self.0 & messages.0 == messages.0
    }
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl ScriptMessages {
    /// Whether a script with this set receives `message` in a phase. Every message other than the
    /// three per-frame ones is always delivered.
    pub(crate) fn wants(self, message: crate::ScriptMessage) -> bool {
        match message {
            crate::ScriptMessage::FixedUpdate => self.contains(Self::FIXED_UPDATE),
            crate::ScriptMessage::Update => self.contains(Self::UPDATE),
            crate::ScriptMessage::LateUpdate => self.contains(Self::LATE_UPDATE),
            _ => true,
        }
    }
}

impl Default for ScriptMessages {
    /// Types nobody has reported receive every message.
    fn default() -> Self {
        Self::ALL
    }
}

/// What is known about one script type: the messages its host reported and its live scripts.
#[derive(Clone, Default)]
struct TypeEntry {
    messages: Option<ScriptMessages>,
    scripts: BTreeSet<ComponentHandle>,
}

/// Script types by assembly, then type name, so a lookup needs no allocation.
#[derive(Clone, Default)]
pub(crate) struct ScriptTypes {
    by_assembly: BTreeMap<String, BTreeMap<String, TypeEntry>>,
}

impl ScriptTypes {
    /// Notes a new script of the type and returns the set it starts with.
    pub(crate) fn enroll(
        &mut self,
        assembly: &str,
        type_name: &str,
        script: ComponentHandle,
    ) -> ScriptMessages {
        let entry = self.entry(assembly, type_name);
        entry.scripts.insert(script);
        entry.messages.unwrap_or_default()
    }

    /// Forgets a script that was removed or retired.
    pub(crate) fn leave(&mut self, assembly: &str, type_name: &str, script: ComponentHandle) {
        if let Some(entry) = self
            .by_assembly
            .get_mut(assembly)
            .and_then(|types| types.get_mut(type_name))
        {
            entry.scripts.remove(&script);
        }
    }

    /// Records the type's set and returns the scripts that must take it.
    fn report(
        &mut self,
        assembly: &str,
        type_name: &str,
        messages: ScriptMessages,
    ) -> Vec<ComponentHandle> {
        let entry = self.entry(assembly, type_name);
        entry.messages = Some(messages);
        entry.scripts.iter().copied().collect()
    }

    fn entry(&mut self, assembly: &str, type_name: &str) -> &mut TypeEntry {
        if !self.by_assembly.contains_key(assembly) {
            self.by_assembly
                .insert(assembly.to_owned(), BTreeMap::new());
        }
        let types = self
            .by_assembly
            .get_mut(assembly)
            .expect("assembly just ensured");
        if !types.contains_key(type_name) {
            types.insert(type_name.to_owned(), TypeEntry::default());
        }
        types.get_mut(type_name).expect("type just ensured")
    }
}

impl World {
    /// Records the per-frame messages `type_name` in `assembly` defines and applies them to every
    /// registered script of that type. Scripts added later take the same set.
    pub fn set_script_type_messages(
        &mut self,
        assembly: &str,
        type_name: &str,
        messages: ScriptMessages,
    ) -> Result<(), WorldError> {
        crate::components::validate_script_name(assembly, type_name)?;
        for handle in self.lifecycle.types.report(assembly, type_name, messages) {
            if let Some(state) = self.lifecycle.scripts.get_mut(&handle) {
                state.messages = messages;
            }
        }
        Ok(())
    }

    /// Adds a new script to its type's roster and returns the set it starts with.
    pub(crate) fn enroll_script_type(&mut self, handle: ComponentHandle) -> ScriptMessages {
        match self.components.get(handle.0).map(|c| &c.desc) {
            Some(ComponentDesc::Script {
                assembly,
                type_name,
            }) => self.lifecycle.types.enroll(assembly, type_name, handle),
            _ => ScriptMessages::ALL,
        }
    }

    /// Drops a removed script from its type's roster.
    pub(crate) fn leave_script_type(&mut self, handle: ComponentHandle) {
        if let Some(ComponentDesc::Script {
            assembly,
            type_name,
        }) = self.components.get(handle.0).map(|c| &c.desc)
        {
            self.lifecycle.types.leave(assembly, type_name, handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DestroyWhen, ObjectDesc, WorldSettings};

    fn roster(world: &World) -> usize {
        world
            .lifecycle
            .types
            .by_assembly
            .values()
            .flat_map(BTreeMap::values)
            .map(|entry| entry.scripts.len())
            .sum()
    }

    #[test]
    fn messages_other_than_the_per_frame_three_are_always_wanted() {
        use crate::ScriptMessage::{Awake, FixedUpdate, OnDestroy, OnDisable, OnEnable, Start};
        for message in [Awake, OnEnable, Start, OnDisable, OnDestroy] {
            assert!(ScriptMessages::NONE.wants(message), "{message:?}");
        }
        assert!(!ScriptMessages::NONE.wants(FixedUpdate));
    }

    #[test]
    fn retired_and_removed_scripts_leave_their_types_roster() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        let desc = ComponentDesc::Script {
            assembly: "Example".into(),
            type_name: "Actor".into(),
        };
        let kept = world.add_component(object, desc.clone()).unwrap();
        let removed = world.add_component(object, desc).unwrap();
        assert_eq!(roster(&world), 2);
        world.destroy_component(removed).unwrap();
        world.destroy(object, DestroyWhen::EndOfFrame).unwrap();
        world.take_cleanup();
        world.finish_phase();
        assert_eq!(roster(&world), 0, "{kept:?} is retired with its object");
    }
}
