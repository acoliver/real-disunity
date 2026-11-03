//! Which scripts a phase has to look at.
//!
//! A world holds tens of thousands of scripts while a few hundred define `FixedUpdate`, `Update`
//! or `LateUpdate`, and tens of thousands more wait for `Start` on objects that are not active. A
//! script is only ready, and so only receives `Start` or a per-frame message, once it is awake and
//! active. The index keeps, in handle order, the scripts that wait for `Start` and are awake and
//! active, and for each per-frame message the scripts whose type defines it. A phase visits those
//! instead of every script. `Lifecycle` owns the index and updates it wherever a script's `start`,
//! `awake`, `active` or `messages` change, so it always equals a scan of the script table.

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

/// The parts of a script's state the index depends on.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Key {
    awaiting_start: bool,
    messages: ScriptMessages,
}

impl Key {
    pub(crate) fn of(state: &ScriptState) -> Self {
        Self {
            awaiting_start: state.start == StartState::Pending && state.awake && state.active,
            messages: state.messages,
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct PhaseIndex {
    awaiting_start: BTreeSet<ComponentHandle>,
    per_frame: [BTreeSet<ComponentHandle>; 3],
}

impl PhaseIndex {
    pub(crate) fn add(&mut self, script: ComponentHandle, key: Key) {
        if key.awaiting_start {
            self.awaiting_start.insert(script);
        }
        for (slot, (_, bit)) in PER_FRAME.iter().enumerate() {
            if key.messages.contains(*bit) {
                self.per_frame[slot].insert(script);
            }
        }
    }

    pub(crate) fn drop_entries(&mut self, script: ComponentHandle, key: Key) {
        self.awaiting_start.remove(&script);
        for (slot, (_, bit)) in PER_FRAME.iter().enumerate() {
            if key.messages.contains(*bit) {
                self.per_frame[slot].remove(&script);
            }
        }
    }

    pub(crate) fn retarget(&mut self, script: ComponentHandle, before: Key, after: Key) {
        if before != after {
            self.drop_entries(script, before);
            self.add(script, after);
        }
    }

    pub(crate) fn visit(&self, message: ScriptMessage) -> Visit {
        if message == ScriptMessage::Start {
            return Visit::Listed(self.awaiting_start.iter().copied().collect());
        }
        let Some(slot) = PER_FRAME.iter().position(|(m, _)| *m == message) else {
            return Visit::All;
        };
        let mut scripts: Vec<_> = self
            .awaiting_start
            .iter()
            .chain(&self.per_frame[slot])
            .copied()
            .collect();
        scripts.sort_unstable();
        scripts.dedup();
        Visit::Listed(scripts)
    }

    /// Whether the index equals what a scan of `scripts` produces.
    #[cfg(test)]
    pub(crate) fn matches(
        &self,
        scripts: &std::collections::BTreeMap<ComponentHandle, ScriptState>,
    ) -> bool {
        let mut expected = Self::default();
        for (&script, state) in scripts {
            expected.add(script, Key::of(state));
        }
        self.awaiting_start == expected.awaiting_start && self.per_frame == expected.per_frame
    }
}
