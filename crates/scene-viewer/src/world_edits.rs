//! Turns one drain of world changes into a single retained scene batch.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_plan::simulation::mesh_instances::{
    MeshExtractError, MeshResolver, extract_mesh_object,
};
use worldrunner_world_simulation::{ComponentKind, ObjectHandle, World, WorldChanges};

use crate::{ObjectId, SceneObjectEdit};

/// Which retained [`ObjectId`] shows which world object. The caller owns it for the life of the
/// retained host, so ids stay stable while an object lives and are never reused afterwards.
#[derive(Clone, Debug, Default)]
pub struct ObjectIdMap {
    ids: BTreeMap<ObjectHandle, ObjectId>,
    next: u64,
}

impl ObjectIdMap {
    /// Maps `objects` to ids `0..n` in order, the ids a host assigns to the objects of its
    /// initial scene. Pass the objects in the order the initial scene lists them.
    pub fn from_initial(objects: impl IntoIterator<Item = ObjectHandle>) -> Self {
        let ids: BTreeMap<_, _> = objects
            .into_iter()
            .enumerate()
            .map(|(index, object)| (object, ObjectId(index as u64)))
            .collect();
        let next = ids.len() as u64;
        Self { ids, next }
    }

    pub fn get(&self, object: ObjectHandle) -> Option<ObjectId> {
        self.ids.get(&object).copied()
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }
}

/// What to do with the id map once the whole batch is known to be valid.
enum Mapping {
    Assign(ObjectHandle, ObjectId),
    Release(ObjectHandle),
}

/// The retained edits that bring the scene in line with `world` after `changes`.
///
/// An object is drawn when it is alive, active in its hierarchy and has a mesh instance. Newly
/// drawn objects get an `Add`, drawn objects whose transform, parent, active state or mesh
/// instance changed get a `Replace`, and objects that stopped being drawn or were destroyed get
/// a `Remove`. Descendants of an object whose transform, parent or active state changed are
/// re-extracted too, because their world transforms moved with it.
///
/// `ids` is updated only when extraction succeeds for every affected object. On error it is
/// unchanged and the caller keeps the drained changes' effect out of the scene.
pub fn edits_from_world_changes(
    changes: &WorldChanges,
    world: &World,
    resolver: &impl MeshResolver,
    ids: &mut ObjectIdMap,
) -> Result<Vec<SceneObjectEdit>, MeshExtractError> {
    let mut edits = Vec::new();
    let mut mappings = Vec::new();
    let mut next = ids.next;
    for &object in &changes.destroyed {
        if let Some(id) = ids.get(object) {
            edits.push(SceneObjectEdit::Remove(id));
            mappings.push(Mapping::Release(object));
        }
    }
    for object in affected_objects(changes, world) {
        let wanted = extract_mesh_object(world, object, resolver)?;
        match (ids.get(object), wanted) {
            (Some(id), Some(scene)) => edits.push(SceneObjectEdit::Replace { id, object: scene }),
            (Some(id), None) => {
                edits.push(SceneObjectEdit::Remove(id));
                mappings.push(Mapping::Release(object));
            }
            (None, Some(scene)) => {
                let id = ObjectId(next);
                next += 1;
                edits.push(SceneObjectEdit::Add { id, object: scene });
                mappings.push(Mapping::Assign(object, id));
            }
            (None, None) => {}
        }
    }
    for mapping in mappings {
        match mapping {
            Mapping::Assign(object, id) => ids.ids.insert(object, id),
            Mapping::Release(object) => ids.ids.remove(&object),
        };
    }
    ids.next = next;
    Ok(edits)
}

/// Objects whose drawn state may have changed, in handle order, without destroyed objects.
fn affected_objects(changes: &WorldChanges, world: &World) -> BTreeSet<ObjectHandle> {
    let moved: Vec<_> = changes
        .transforms
        .iter()
        .chain(&changes.parents)
        .chain(&changes.properties)
        .copied()
        .collect();
    let mut affected: BTreeSet<_> = moved.iter().copied().collect();
    affected.extend(world.descendants(&moved));
    affected.extend(&changes.created);
    affected.extend(
        changes
            .components
            .iter()
            .filter(|entry| entry.kind == ComponentKind::MeshInstance)
            .map(|entry| entry.object),
    );
    for destroyed in &changes.destroyed {
        affected.remove(destroyed);
    }
    affected
}

#[cfg(test)]
mod tests;
