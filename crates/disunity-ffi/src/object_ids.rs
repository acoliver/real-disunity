//! The ABI identities of a session's objects, findable by identity and by native object.

use std::collections::{BTreeMap, HashMap};
use worldrunner_world_simulation::ObjectHandle;

/// Maps encoded ABI identities to native objects and back, so neither direction scans the session.
#[derive(Debug, Default)]
pub(crate) struct ObjectIds {
    by_id: BTreeMap<u64, ObjectHandle>,
    by_object: HashMap<ObjectHandle, u64>,
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
