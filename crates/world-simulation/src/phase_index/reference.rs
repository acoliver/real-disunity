//! The phase-task selection as it was before the index, kept as the oracle the index is
//! checked against: every registered script is visited in handle order.

use crate::timeline::Task;
use crate::{LifecycleWork, ScriptMessage, World};

pub(crate) const MESSAGES: [ScriptMessage; 8] = [
    ScriptMessage::Start,
    ScriptMessage::FixedUpdate,
    ScriptMessage::Update,
    ScriptMessage::LateUpdate,
    ScriptMessage::Awake,
    ScriptMessage::OnEnable,
    ScriptMessage::OnDisable,
    ScriptMessage::OnDestroy,
];

fn full_scan(world: &World, message: ScriptMessage) -> Vec<LifecycleWork> {
    let mut work = Vec::new();
    for (&script, state) in &world.lifecycle.scripts {
        let pending = state.start == crate::lifecycle::StartState::Pending;
        if !pending && (message == ScriptMessage::Start || !state.messages.wants(message)) {
            continue;
        }
        if !world.script_ready(script).unwrap_or(false) {
            continue;
        }
        if pending {
            work.push(LifecycleWork {
                script,
                message: ScriptMessage::Start,
            });
        }
        if message != ScriptMessage::Start && state.messages.wants(message) {
            work.push(LifecycleWork { script, message });
        }
    }
    work
}

fn queued(world: &mut World, message: ScriptMessage) -> Vec<LifecycleWork> {
    world
        .phase_tasks(message)
        .into_iter()
        .map(|task| match task {
            Task::Work(work) => work,
            _ => panic!("a phase queues only script work"),
        })
        .collect()
}

/// Asserts that a phase for every message queues what the full scan queues, in the same order.
pub(crate) fn assert_phases_match(world: &mut World) {
    for message in MESSAGES {
        let expected = full_scan(world, message);
        assert_eq!(queued(world, message), expected, "{message:?}");
    }
}
