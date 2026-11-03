//! Which scripts a phase has to look at.
//!
//! A world holds tens of thousands of scripts while a few hundred define `FixedUpdate`, `Update`
//! or `LateUpdate` and only newly created ones wait for `Start`. The index keeps those groups in
//! handle order so a phase visits them instead of every script. `Lifecycle` owns it and updates it
//! wherever a script's `start` or `messages` change, so it always equals a scan of the script table.

use crate::lifecycle::{ScriptState, StartState};
use crate::{ComponentHandle, ScriptMessage, ScriptMessages};
use std::collections::BTreeSet;

#[cfg(test)]
pub(crate) mod reference;
#[cfg(test)]
mod tests;

const PER_FRAME: [(ScriptMessage, ScriptMessages); 3] = [
    (ScriptMessage::FixedUpdate, ScriptMessages::FIXED_UPDATE),
    (ScriptMessage::Update, ScriptMessages::UPDATE),
    (ScriptMessage::LateUpdate, ScriptMessages::LATE_UPDATE),
];

/// What a phase for one message has to visit.
pub(crate) enum Visit {
    /// The union of these scripts, which wait for `Start` or define the message.
    Listed(Vec<ComponentHandle>),
    /// Every script: the message is delivered to scripts regardless of the types' reports.
    All,
}

#[derive(Clone, Default)]
pub(crate) struct PhaseIndex {
    pending_start: BTreeSet<ComponentHandle>,
    per_frame: [BTreeSet<ComponentHandle>; 3],
}

impl PhaseIndex {
    pub(crate) fn insert(&mut self, script: ComponentHandle, state: &ScriptState) {
        if state.start == StartState::Pending {
            self.pending_start.insert(script);
        }
        self.add_messages(script, state.messages);
    }

    pub(crate) fn remove(&mut self, script: ComponentHandle, state: &ScriptState) {
        self.pending_start.remove(&script);
        self.drop_messages(script, state.messages);
    }

    pub(crate) fn started(&mut self, script: ComponentHandle) {
        self.pending_start.remove(&script);
    }

    pub(crate) fn retarget(
        &mut self,
        script: ComponentHandle,
        old: ScriptMessages,
        new: ScriptMessages,
    ) {
        self.drop_messages(script, old);
        self.add_messages(script, new);
    }

    pub(crate) fn visit(&self, message: ScriptMessage) -> Visit {
        if message == ScriptMessage::Start {
            return Visit::Listed(self.pending_start.iter().copied().collect());
        }
        let Some(slot) = PER_FRAME.iter().position(|(m, _)| *m == message) else {
            return Visit::All;
        };
        let mut scripts: Vec<_> = self
            .pending_start
            .iter()
            .chain(&self.per_frame[slot])
            .copied()
            .collect();
        scripts.sort_unstable();
        scripts.dedup();
        Visit::Listed(scripts)
    }

    fn add_messages(&mut self, script: ComponentHandle, messages: ScriptMessages) {
        for (slot, (_, bit)) in PER_FRAME.iter().enumerate() {
            if messages.contains(*bit) {
                self.per_frame[slot].insert(script);
            }
        }
    }

    fn drop_messages(&mut self, script: ComponentHandle, messages: ScriptMessages) {
        for (slot, (_, bit)) in PER_FRAME.iter().enumerate() {
            if messages.contains(*bit) {
                self.per_frame[slot].remove(&script);
            }
        }
    }

    /// Whether the index equals what a scan of `scripts` produces.
    #[cfg(test)]
    pub(crate) fn matches(
        &self,
        scripts: &std::collections::BTreeMap<ComponentHandle, ScriptState>,
    ) -> bool {
        let mut expected = Self::default();
        for (&script, state) in scripts {
            expected.insert(script, state);
        }
        self.pending_start == expected.pending_start && self.per_frame == expected.per_frame
    }
}
