//! Which scripted objects the start of a frame revisits.
//!
//! A script's `Awake`, `OnEnable` and `OnDisable` follow from its object's activity in the
//! hierarchy, the script's enabled flag and whether it has woken. Every mutation that changes one
//! of those refreshes the objects it affects, except the few listed on [`World::mark_unsettled`]:
//! those note the object, and the next frame start refreshes the noted objects. An object that was
//! refreshed and not noted since gives the same answer again, so a frame start leaves every other
//! object alone instead of visiting each of the tens of thousands of scripted objects.

use crate::{ComponentHandle, ObjectHandle, World};
use std::collections::HashSet;

#[cfg(test)]
mod reference;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod walk;

/// Objects whose scripts may differ from the lifecycle a refresh would give them.
#[derive(Clone, Default)]
pub(crate) struct Unsettled {
    objects: HashSet<ObjectHandle>,
}

impl World {
    /// Notes that a mutation changed what a refresh of `object` would do, without refreshing it.
    /// The next frame start refreshes it. Callers: registering a script, a script's `Awake`
    /// finding its object inactive, and publishing a construction (for the published objects
    /// outside it that sit below the published ones).
    pub(crate) fn mark_unsettled(&mut self, object: ObjectHandle) {
        self.lifecycle.unsettled.objects.insert(object);
    }

    /// Notes every object below `nodes` that is not itself one of `nodes`.
    pub(crate) fn mark_below_unsettled(&mut self, nodes: &[ObjectHandle]) {
        let inside: HashSet<_> = nodes.iter().copied().collect();
        let mut pending: Vec<_> = nodes
            .iter()
            .flat_map(|&node| self.child_objects(node))
            .collect();
        while let Some(object) = pending.pop() {
            if !inside.contains(&object) {
                self.mark_unsettled(object);
                pending.extend(self.child_objects(object));
            }
        }
    }

    fn child_objects(&self, object: ObjectHandle) -> Vec<ObjectHandle> {
        self.live_object(object)
            .map(|stored| stored.children.iter().copied().collect())
            .unwrap_or_default()
    }

    /// The objects the next look at scripts refreshes, sorted and without repeats: those of the
    /// scripts registered since the last look and, at a frame start, the unsettled ones. An object
    /// that has been removed or published later, or that has no script, is left out.
    pub(crate) fn refresh_candidates(
        &mut self,
        added: Vec<ComponentHandle>,
        frame_start: bool,
    ) -> Vec<ObjectHandle> {
        let mut objects: Vec<_> = added
            .into_iter()
            .filter(|script| self.lifecycle.scripts.contains_key(script))
            .filter_map(|script| self.component(script).ok().map(|c| c.object))
            .collect();
        if frame_start {
            objects.extend(std::mem::take(&mut self.lifecycle.unsettled.objects));
        }
        objects.retain(|&object| !self.is_unpublished(object) && self.has_script(object));
        objects.sort_unstable();
        objects.dedup();
        objects
    }

    fn has_script(&self, object: ObjectHandle) -> bool {
        self.live_object(object).is_ok_and(|stored| {
            stored
                .components
                .iter()
                .any(|component| self.lifecycle.scripts.contains_key(component))
        })
    }
}
