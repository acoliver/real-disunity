//! Which per-frame messages a script defines.
//!
//! The engine's choice is to call `FixedUpdate`, `Update` and `LateUpdate` only on scripts whose
//! type defines them, so a host reports each script's set once and the timeline skips the rest.

use crate::{ComponentHandle, World, WorldError};

/// The per-frame messages a script defines.
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
    /// Scripts nobody has reported receive every message.
    fn default() -> Self {
        Self::ALL
    }
}

impl World {
    /// Sets the per-frame messages one registered script defines, so the timeline skips it for
    /// the rest. A new script starts with every message and a clone takes its source's set.
    pub fn set_script_messages(
        &mut self,
        script: ComponentHandle,
        messages: ScriptMessages,
    ) -> Result<(), WorldError> {
        self.component(script)?;
        if !self.lifecycle.scripts.contains_key(&script) {
            return Err(WorldError::NotScript);
        }
        self.lifecycle.set_messages(script, messages);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_other_than_the_per_frame_three_are_always_wanted() {
        use crate::ScriptMessage::{Awake, FixedUpdate, OnDestroy, OnDisable, OnEnable, Start};
        for message in [Awake, OnEnable, Start, OnDisable, OnDestroy] {
            assert!(ScriptMessages::NONE.wants(message), "{message:?}");
        }
        assert!(!ScriptMessages::NONE.wants(FixedUpdate));
    }
}
