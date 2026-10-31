//! Turns one drain of world changes into a single retained scene batch.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_plan::SceneObject;
use worldrunner_render_plan::simulation::mesh_instances::{
    MeshExtractError, MeshExtractOptions, MeshResolver, extract_mesh_objects,
};
use worldrunner_world_simulation::{ComponentKind, ObjectHandle, World, WorldChanges};

use crate::{ObjectId, SceneObjectEdit};

/// Which retained [`ObjectId`]s show which world object. A world object that the host draws as
/// several retained objects, such as LOD levels or impostor cards, maps to an ordered list. The
/// caller owns the map for the life of the retained host, so ids stay stable while an object
/// lives and are never reused afterwards.
#[derive(Clone, Debug, Default)]
pub struct ObjectIdMap {
    ids: BTreeMap<ObjectHandle, Vec<ObjectId>>,
    next: u64,
}

impl ObjectIdMap {
    /// Maps `objects` to ids `0..n` in order, the ids a host assigns to the objects of its
    /// initial scene. Pass one entry per retained scene object, in the order the initial scene
    /// lists them. A world object listed several times owns all of its ids, in that order.
    pub fn from_initial(objects: impl IntoIterator<Item = ObjectHandle>) -> Self {
        let mut ids: BTreeMap<ObjectHandle, Vec<ObjectId>> = BTreeMap::new();
        let mut next = 0_u64;
        for object in objects {
            ids.entry(object).or_default().push(ObjectId(next));
            next += 1;
        }
        Self { ids, next }
    }

    /// The first retained id of `object`, which is its only id when it draws one scene object.
    pub fn get(&self, object: ObjectHandle) -> Option<ObjectId> {
        self.ids.get(&object).and_then(|ids| ids.first().copied())
    }

    /// Every retained id of `object` in scene order, empty when it is not drawn.
    pub fn get_all(&self, object: ObjectHandle) -> &[ObjectId] {
        self.ids.get(&object).map_or(&[], Vec::as_slice)
    }

    /// The number of drawn world objects.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// The number of retained ids across all world objects.
    pub fn id_count(&self) -> usize {
        self.ids.values().map(Vec::len).sum()
    }
}

/// The new id list of one world object once the whole batch is known to be valid. An empty list
/// releases the object.
struct Mapping {
    object: ObjectHandle,
    ids: Vec<ObjectId>,
}

/// The retained edits that bring the scene in line with `world` after `changes`.
///
/// An object is drawn when it is alive, active in its hierarchy and has a mesh instance. Newly
/// drawn objects get an `Add` per scene object, drawn objects whose transform, parent, active
/// state or mesh instance changed get a `Replace` per scene object, and objects that stopped
/// being drawn or were destroyed get a `Remove` per retained id. If the resolver now returns
/// more or fewer scene objects for an object than it has ids, the surplus ids are removed and
/// the missing ones are added under fresh ids. Descendants of an object whose transform, parent
/// or active state changed are re-extracted too, because their world transforms moved with it.
///
/// `ids` is updated only when extraction succeeds for every affected object. On error it is
/// unchanged and the caller keeps the drained changes' effect out of the scene.
pub fn edits_from_world_changes(
    changes: &WorldChanges,
    world: &World,
    resolver: &impl MeshResolver,
    ids: &mut ObjectIdMap,
) -> Result<Vec<SceneObjectEdit>, MeshExtractError> {
    edits_from_world_changes_with(changes, world, resolver, MeshExtractOptions::default(), ids)
}

/// [`edits_from_world_changes`] with explicit extraction options.
pub fn edits_from_world_changes_with(
    changes: &WorldChanges,
    world: &World,
    resolver: &impl MeshResolver,
    options: MeshExtractOptions,
    ids: &mut ObjectIdMap,
) -> Result<Vec<SceneObjectEdit>, MeshExtractError> {
    let mut edits = Vec::new();
    let mut mappings = Vec::new();
    let mut next = ids.next;
    for &object in &changes.destroyed {
        let current = ids.get_all(object);
        if !current.is_empty() {
            edits.extend(current.iter().copied().map(SceneObjectEdit::Remove));
            mappings.push(Mapping {
                object,
                ids: Vec::new(),
            });
        }
    }
    for object in affected_objects(changes, world) {
        let wanted = extract_mesh_objects(world, object, resolver, options)?;
        let current = ids.get_all(object);
        let updated = edits_for_object(current, wanted, &mut next, &mut edits);
        if updated != current {
            mappings.push(Mapping {
                object,
                ids: updated,
            });
        }
    }
    for Mapping { object, ids: list } in mappings {
        if list.is_empty() {
            ids.ids.remove(&object);
        } else {
            ids.ids.insert(object, list);
        }
    }
    ids.next = next;
    Ok(edits)
}

/// Pairs the retained ids of one object with its wanted scene objects in order, replacing the
/// pairs, removing surplus ids and adding surplus objects under fresh ids from `next`. Returns
/// the object's id list afterwards.
fn edits_for_object(
    current: &[ObjectId],
    wanted: Vec<SceneObject>,
    next: &mut u64,
    edits: &mut Vec<SceneObjectEdit>,
) -> Vec<ObjectId> {
    let mut updated = Vec::with_capacity(wanted.len());
    let mut wanted = wanted.into_iter();
    for &id in current {
        match wanted.next() {
            Some(object) => {
                edits.push(SceneObjectEdit::Replace { id, object });
                updated.push(id);
            }
            None => edits.push(SceneObjectEdit::Remove(id)),
        }
    }
    for object in wanted {
        let id = ObjectId(*next);
        *next += 1;
        edits.push(SceneObjectEdit::Add { id, object });
        updated.push(id);
    }
    updated
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
mod placed_tests;
#[cfg(test)]
mod template_tests;
#[cfg(test)]
mod tests;
