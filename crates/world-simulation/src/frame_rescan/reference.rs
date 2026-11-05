//! The start of a frame as it was before it tracked what changed, kept as the oracle the
//! tracking is checked against: every registered script's object is refreshed.

use crate::{ComponentHandle, CoroutineId, LifecycleWork, World};
use std::collections::BTreeSet;

/// Looks at every scripted object, as the frame start did.
pub(super) fn frame_start(world: &mut World) {
    world.lifecycle.added.clear();
    world.scheduler.frame_start = false;
    let nodes: BTreeSet<_> = world
        .lifecycle
        .scripts
        .keys()
        .copied()
        .filter_map(|script| world.component(script).ok().map(|c| c.object))
        .filter(|&object| !world.is_unpublished(object))
        .collect();
    if !nodes.is_empty() {
        let mutation = world.reserve_mutation().unwrap();
        world.refresh_lifecycle(&nodes.into_iter().collect::<Vec<_>>(), mutation);
    }
}

/// What a frame start leaves behind that a script or the host can see. An empty batch of work
/// is not counted: the rescan queued one every frame and nothing reads it.
#[derive(Debug, PartialEq)]
pub(super) struct Observed {
    scripts: Vec<(ComponentHandle, u8)>,
    work: Vec<(usize, Vec<LifecycleWork>)>,
    cleanup: Vec<LifecycleWork>,
    coroutines: Vec<CoroutineId>,
    cancelled: Vec<CoroutineId>,
}

fn observe(world: &World) -> Observed {
    let scripts = world
        .lifecycle
        .scripts
        .iter()
        .map(|(&script, state)| {
            let flags = [state.enabled, state.awake, state.active]
                .into_iter()
                .fold(0, |bits, flag| bits * 2 + u8::from(flag));
            let entered = match state.entered {
                crate::lifecycle::EnteredState::Dormant => 0,
                crate::lifecycle::EnteredState::Awake => 1,
                crate::lifecycle::EnteredState::Enabled => 2,
            };
            (script, flags * 4 + entered)
        })
        .collect();
    Observed {
        scripts,
        work: world
            .lifecycle
            .mutations
            .iter()
            .filter(|(_, work)| !work.is_empty())
            .map(|(&sequence, work)| (sequence, work.clone()))
            .collect(),
        cleanup: world.lifecycle.cleanup.clone(),
        coroutines: world.coroutines.entries.keys().copied().collect(),
        cancelled: world.coroutines.cancelled.clone(),
    }
}

/// Asserts that starting a frame now leaves the world as the full rescan would.
pub(super) fn assert_frame_start_matches(world: &World) {
    let mut fast = world.clone();
    fast.scheduler.frame_start = true;
    fast.initialize_scripts().unwrap();
    let mut full = world.clone();
    frame_start(&mut full);
    let (fast, full) = (observe(&fast), observe(&full));
    let differing: Vec<_> = fast
        .scripts
        .iter()
        .zip(&full.scripts)
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(differing, Vec::<(&_, &_)>::new(), "script states differ");
    assert_eq!(fast.work, full.work, "queued work differs");
    assert_eq!(fast, full);
}
