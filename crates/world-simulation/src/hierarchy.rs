use crate::{LocalTransform, MutationId, ObjectHandle, World, WorldError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParentSpace {
    KeepLocal,
    KeepWorld,
}

impl World {
    pub(crate) fn attach_parent(
        &mut self,
        object: ObjectHandle,
        parent: Option<ObjectHandle>,
    ) -> Result<(), WorldError> {
        self.objects
            .get_mut(object.0)
            .expect("validated object")
            .parent = parent;
        self.record_change(crate::journal::Change::Parent(object));
        if parent.is_some_and(|parent| self.lifetime.objects.contains(&parent)) {
            self.destroy(object, crate::DestroyWhen::EndOfFrame)?;
        }
        Ok(())
    }

    pub fn parent(&self, object: ObjectHandle) -> Result<Option<ObjectHandle>, WorldError> {
        Ok(self.live_object(object)?.parent)
    }

    pub fn children(&self, object: ObjectHandle) -> Result<Vec<ObjectHandle>, WorldError> {
        self.live_object(object)?;
        Ok(self
            .objects
            .iter(self.identity)
            .filter(|(_, value)| value.parent == Some(object))
            .map(|(id, _)| ObjectHandle(id))
            .collect())
    }

    pub(crate) fn subtree(&self, root: ObjectHandle) -> Result<Vec<ObjectHandle>, WorldError> {
        self.live_object(root)?;
        let mut result = Vec::new();
        let mut pending = vec![root];
        while let Some(object) = pending.pop() {
            result.push(object);
            pending.extend(self.children(object)?.into_iter().rev());
        }
        Ok(result)
    }

    pub fn active_in_hierarchy(&self, object: ObjectHandle) -> Result<bool, WorldError> {
        let mut current = Some(object);
        while let Some(handle) = current {
            if self.is_unpublished(handle) || !self.object(handle)?.active {
                return Ok(false);
            }
            current = self.parent(handle)?;
        }
        Ok(true)
    }

    pub fn set_parent(
        &mut self,
        object: ObjectHandle,
        parent: Option<ObjectHandle>,
        space: ParentSpace,
    ) -> Result<MutationId, WorldError> {
        self.live_object(object)?;
        let mut current = parent;
        while let Some(handle) = current {
            self.live_object(handle)?;
            if handle == object {
                return Err(WorldError::HierarchyCycle);
            }
            current = self.parent(handle)?;
        }
        let local = match space {
            ParentSpace::KeepLocal => self.local_transform(object)?,
            ParentSpace::KeepWorld => self.local_from_world(
                parent,
                self.world_matrix(object)?,
                self.world_rotation(object)?,
            )?,
        };
        let mutation = self.reserve_mutation()?;
        self.set_local_transform(object, local)?;
        self.attach_parent(object, parent)?;
        let nodes = self.subtree(object)?;
        self.refresh_lifecycle(&nodes, mutation);
        Ok(mutation)
    }

    pub fn set_active(
        &mut self,
        object: ObjectHandle,
        active: bool,
    ) -> Result<MutationId, WorldError> {
        let nodes = self.subtree(object)?;
        let mutation = self.reserve_mutation()?;
        self.objects
            .get_mut(object.0)
            .expect("validated object")
            .active = active;
        self.record_change(crate::journal::Change::Properties(object));
        self.refresh_lifecycle(&nodes, mutation);
        Ok(mutation)
    }

    /// Queues `Awake` and `OnEnable` for scripts on `object` and its children that are now
    /// eligible, without changing whether any object is active. Objects in an open
    /// construction receive no work until it is published.
    pub fn wake(&mut self, object: ObjectHandle) -> Result<MutationId, WorldError> {
        let nodes = self.subtree(object)?;
        let mutation = self.reserve_mutation()?;
        self.refresh_lifecycle(&nodes, mutation);
        Ok(mutation)
    }

    pub(crate) fn local_from_world(
        &self,
        parent: Option<ObjectHandle>,
        matrix: glam::Mat4,
        rotation: glam::Quat,
    ) -> Result<LocalTransform, WorldError> {
        let (local, local_rotation) = if let Some(parent) = parent {
            let parent_matrix = self.world_matrix(parent)?;
            if parent_matrix.determinant() == 0.0 {
                return Err(WorldError::SingularParent);
            }
            let inverse = parent_matrix.inverse();
            if !inverse.is_finite() {
                return Err(WorldError::SingularParent);
            }
            (
                inverse * matrix,
                self.world_rotation(parent)?.inverse() * rotation,
            )
        } else {
            (matrix, rotation)
        };
        crate::transform::decompose(local, local_rotation.normalize())
    }
}
