//! The ABI identities of a session's objects, findable by identity and by native object.

use std::collections::{BTreeMap, HashMap};
use worldrunner_world_simulation::ObjectHandle;

/// Maps encoded ABI identities to native objects and back, so neither direction scans the session.
#[derive(Debug, Default)]
pub(crate) struct ObjectIds {
    by_id: BTreeMap<u64, ObjectHandle>,
    by_object: HashMap<ObjectHandle, u64>,
    /// The world's removed object count when the identities were last checked.
    checked_at: u64,
}

impl ObjectIds {
    pub(crate) fn get(&self, id: &u64) -> Option<&ObjectHandle> {
        self.by_id.get(id)
    }

    /// The identity that encodes `object`, if it has one.
    pub(crate) fn id_of(&self, object: ObjectHandle) -> Option<u64> {
        self.by_object.get(&object).copied()
    }

    pub(crate) fn insert(&mut self, id: u64, object: ObjectHandle) {
        if let Some(previous) = self.by_id.insert(id, object) {
            self.by_object.remove(&previous);
        }
        self.by_object.insert(object, id);
    }

    pub(crate) fn remove(&mut self, id: &u64) -> Option<ObjectHandle> {
        let object = self.by_id.remove(id)?;
        self.by_object.remove(&object);
        Some(object)
    }

    /// Drops the identities of objects `world` no longer holds. Objects only stop being live
    /// when the world removes one, so nothing is checked while its removed count holds still.
    pub(crate) fn prune(&mut self, world: &worldrunner_world_simulation::World) {
        let removed = world.removed_objects();
        if removed == self.checked_at {
            return;
        }
        self.checked_at = removed;
        self.retain(|_, handle| world.object_alive(*handle));
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&u64, &ObjectHandle) -> bool) {
        let by_object = &mut self.by_object;
        self.by_id.retain(|id, object| {
            let kept = keep(id, object);
            if !kept {
                by_object.remove(object);
            }
            kept
        });
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&u64, &ObjectHandle)> {
        self.by_id.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.by_id.len()
    }
}

#[cfg(test)]
mod tests {
    use super::ObjectIds;
    use worldrunner_world_simulation::{
        Advance, CallbackOutcome, DestroyWhen, ObjectDesc, TimeSpan, World, WorldSettings,
    };

    fn frame(world: &mut World) {
        let mut work = world.advance(TimeSpan::ZERO).unwrap();
        while let Advance::Callback(callback) = work {
            work = world
                .resume(callback.ticket, CallbackOutcome::Returned)
                .unwrap();
        }
    }

    #[test]
    fn prune_drops_the_identities_of_removed_objects_only() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let gone = world.create_object(ObjectDesc::default()).unwrap();
        let kept = world.create_object(ObjectDesc::default()).unwrap();
        let mut ids = ObjectIds::default();
        ids.insert(1, gone);
        ids.insert(2, kept);
        ids.prune(&world);
        assert_eq!(ids.len(), 2);
        world.destroy(gone, DestroyWhen::EndOfFrame).unwrap();
        frame(&mut world);
        ids.prune(&world);
        assert_eq!((ids.get(&1), ids.id_of(gone)), (None, None));
        assert_eq!(ids.id_of(kept), Some(2));
        let again = world.create_object(ObjectDesc::default()).unwrap();
        ids.insert(3, again);
        world.destroy(again, DestroyWhen::EndOfFrame).unwrap();
        frame(&mut world);
        ids.prune(&world);
        assert_eq!(ids.get(&3), None, "a second removal is checked too");
        assert_eq!(ids.len(), 1);
        ids.insert(4, gone);
        ids.prune(&world);
        assert_eq!(
            ids.get(&4),
            Some(&gone),
            "nothing is checked while the count holds still"
        );
        let last = world.create_object(ObjectDesc::default()).unwrap();
        world.destroy(last, DestroyWhen::EndOfFrame).unwrap();
        frame(&mut world);
        ids.prune(&world);
        assert_eq!(ids.get(&4), None);
    }
}
