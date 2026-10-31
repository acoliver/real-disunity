//! Translation-follow relation between objects, independent of the transform hierarchy.

use std::collections::BTreeSet;

use glam::Vec3;

use crate::{ComponentDesc, ComponentError, LocalTransform, ObjectHandle, World, WorldError};

/// A validated transform write for one follower, applied after the triggering write.
pub(crate) type FollowShift = (crate::ComponentHandle, LocalTransform);

impl World {
    /// Makes `object` follow `target`, or stops it following when `target` is `None`.
    ///
    /// Whenever the world translation of `target` changes by a displacement, `object` is shifted
    /// by the same displacement, and so is everything that follows `object`, transitively. The
    /// relation does not compose transforms and does not constrain the hierarchy: a follower
    /// keeps its own rotation and scale and may have any parent. Follow links may form cycles.
    /// Each object is shifted at most once per triggering write, and the writer itself is never
    /// shifted by its own write, so in a cycle `A -> B -> A` moving `A` moves `B` once.
    ///
    /// An object follows at most one target. Setting a new target replaces the previous one.
    /// An object cannot follow itself. A destroyed target clears the link of its followers when
    /// its storage is retired. Follow links are not part of the change journal and are not
    /// copied by instantiation.
    ///
    /// See [`World::set_local_transform`] for when a displacement is observed.
    pub fn set_follow_target(
        &mut self,
        object: ObjectHandle,
        target: Option<ObjectHandle>,
    ) -> Result<(), WorldError> {
        self.live_object(object)?;
        if let Some(target) = target {
            self.live_object(target)?;
            if target == object {
                return Err(WorldError::FollowSelf);
            }
        }
        self.store_follow(object, target);
        Ok(())
    }

    /// The object `object` follows, if any.
    pub fn follow_target(&self, object: ObjectHandle) -> Result<Option<ObjectHandle>, WorldError> {
        Ok(self.live_object(object)?.follow)
    }

    /// The objects that follow `object` directly, in ascending handle order.
    pub fn followers(&self, object: ObjectHandle) -> Result<Vec<ObjectHandle>, WorldError> {
        Ok(self
            .live_object(object)?
            .followers
            .iter()
            .copied()
            .collect())
    }

    /// Sets the stored link and both sides of its index, recording the old link for rollback.
    /// Objects that are no longer stored have no index to update.
    pub(crate) fn store_follow(&mut self, object: ObjectHandle, target: Option<ObjectHandle>) {
        let Some(stored) = self.objects.get_mut(object.0) else {
            return;
        };
        let previous = std::mem::replace(&mut stored.follow, target);
        if let Some(old) = previous.and_then(|old| self.objects.get_mut(old.0)) {
            old.followers.remove(&object);
        }
        if let Some(new) = target.and_then(|new| self.objects.get_mut(new.0)) {
            new.followers.insert(object);
        }
        if let Some(staging) = &mut self.staging {
            staging.record_follow(object, previous);
        }
    }

    /// Drops every follow link that involves `object`, which is about to leave storage.
    pub(crate) fn clear_follow_links(&mut self, object: ObjectHandle) {
        let Some(stored) = self.objects.get(object.0) else {
            return;
        };
        let followers: Vec<_> = stored.followers.iter().copied().collect();
        for follower in followers {
            self.store_follow(follower, None);
        }
        self.store_follow(object, None);
    }

    /// The transform writes that move every follower of `component`'s object by the world
    /// displacement that replacing its transform with `desc` produces. Empty when nothing
    /// follows the object or the object does not move.
    ///
    /// A follower with a hierarchy parent has its local translation shifted by the displacement
    /// taken into that parent's space as the parent stands before the write, so the follower's
    /// world translation moves by the displacement. If an ancestor of the follower also
    /// follows and is shifted by the same write, the follower moves with the ancestor as well.
    /// Moving a hierarchy ancestor of an object does not move that object's followers.
    pub(crate) fn follow_shifts(
        &self,
        component: crate::ComponentHandle,
        desc: &ComponentDesc,
    ) -> Result<Vec<FollowShift>, WorldError> {
        let (ComponentDesc::Transform(new), Some(snapshot)) =
            (desc, self.components.get(component.0))
        else {
            return Ok(Vec::new());
        };
        let ComponentDesc::Transform(old) = snapshot.desc else {
            return Ok(Vec::new());
        };
        let origin = snapshot.object;
        let local = new.translation - old.translation;
        if local == Vec3::ZERO || self.live_object(origin)?.followers.is_empty() {
            return Ok(Vec::new());
        }
        let displacement = self.world_displacement(origin, local)?;
        if !displacement.is_finite() {
            return Err(WorldError::InvalidComponent(
                ComponentError::InvalidTransform,
            ));
        }
        self.followed_by(origin)
            .into_iter()
            .map(|follower| self.shift_for(follower, displacement))
            .collect()
    }

    /// Every object that follows `origin`, directly or transitively, in ascending handle order.
    /// `origin` is excluded even when a cycle leads back to it.
    fn followed_by(&self, origin: ObjectHandle) -> BTreeSet<ObjectHandle> {
        let mut found = BTreeSet::from([origin]);
        let mut pending = vec![origin];
        while let Some(object) = pending.pop() {
            let Some(stored) = self.objects.get(object.0) else {
                continue;
            };
            for &follower in &stored.followers {
                if found.insert(follower) {
                    pending.push(follower);
                }
            }
        }
        found.remove(&origin);
        found
    }

    /// `local`, a change of `object`'s local translation, in world space.
    fn world_displacement(&self, object: ObjectHandle, local: Vec3) -> Result<Vec3, WorldError> {
        match self.live_object(object)?.parent {
            Some(parent) => Ok(self.world_matrix(parent)?.transform_vector3(local)),
            None => Ok(local),
        }
    }

    fn shift_for(
        &self,
        follower: ObjectHandle,
        displacement: Vec3,
    ) -> Result<FollowShift, WorldError> {
        let stored = self.live_object(follower)?;
        let local = match stored.parent {
            Some(parent) => self.parent_space_vector(parent, displacement)?,
            None => displacement,
        };
        let mut value = self.local_transform(follower)?;
        value.translation += local;
        value.validate()?;
        Ok((stored.transform, value))
    }

    fn parent_space_vector(&self, parent: ObjectHandle, vector: Vec3) -> Result<Vec3, WorldError> {
        let matrix = self.world_matrix(parent)?;
        if matrix.determinant() == 0.0 {
            return Err(WorldError::SingularParent);
        }
        let inverse = matrix.inverse();
        if !inverse.is_finite() {
            return Err(WorldError::SingularParent);
        }
        Ok(inverse.transform_vector3(vector))
    }
}
