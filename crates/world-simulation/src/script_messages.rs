//! Which per-frame messages a script type defines.
//!
//! The engine's choice is to call `FixedUpdate`, `Update` and `LateUpdate` only on scripts whose
//! type defines them, so a host reports each type's set once and the timeline skips the rest.

use crate::{ComponentDesc, World, WorldError};

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
        self.lifecycle
            .type_messages
            .insert((assembly.to_owned(), type_name.to_owned()), messages);
        let matching: Vec<_> = self
            .lifecycle
            .scripts
            .keys()
            .copied()
            .filter(|handle| {
                matches!(
                    self.components.get(handle.0).map(|c| &c.desc),
                    Some(ComponentDesc::Script { assembly: a, type_name: t })
                        if a == assembly && t == type_name
                )
            })
            .collect();
        for handle in matching {
            if let Some(state) = self.lifecycle.scripts.get_mut(&handle) {
                state.messages = messages;
            }
        }
        Ok(())
    }
}
