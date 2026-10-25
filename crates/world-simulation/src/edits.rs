use std::collections::BTreeSet;

use glam::{Quat, Vec3};

use crate::{ComponentDesc, LocalTransform, ObjectHandle, World, WorldError};

/// Typed edits accepted atomically by native storage, independent of any ABI command encoding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectEdit {
    Translation(ObjectHandle, Vec3),
    Rotation(ObjectHandle, Quat),
    MaterialSlot(ObjectHandle, u32),
}

impl World {
    /// Publishes edits and physics as one completed native state. Failure retains
    /// all prior state; the caller retains ownership of the submitted edits.
    pub fn step_with_edits(&mut self, edits: &[ObjectEdit], dt: f32) -> Result<(), WorldError> {
        if edits.is_empty() {
            return self.step_physics(dt);
        }
        let mut staged = Self {
            identity: self.identity,
            objects: self.objects.clone(),
            components: self.components.clone(),
            physics: self.physics.clone(),
            lifetime: self.lifetime.clone(),
            lifecycle: self.lifecycle.clone(),
        };
        staged.apply_edits(edits)?;
        staged.step_physics(dt)?;
        *self = staged;
        Ok(())
    }

    pub fn apply_edits(&mut self, edits: &[ObjectEdit]) -> Result<(), WorldError> {
        self.validate_edits(edits)?;
        for edit in edits {
            match *edit {
                ObjectEdit::Translation(object, translation) => {
                    let value = LocalTransform {
                        translation,
                        ..self.local_transform(object)?
                    };
                    self.set_local_transform(object, value)?;
                }
                ObjectEdit::Rotation(object, rotation) => {
                    let value = LocalTransform {
                        rotation,
                        ..self.local_transform(object)?
                    };
                    self.set_local_transform(object, value)?;
                }
                ObjectEdit::MaterialSlot(object, material_slot) => {
                    let desc = ComponentDesc::Renderer { material_slot };
                    if let Some(handle) = self.renderer(object)? {
                        self.set_component(handle, desc)?;
                    } else {
                        self.add_component(object, desc)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_edits(&self, edits: &[ObjectEdit]) -> Result<(), WorldError> {
        let mut renderers = BTreeSet::new();
        for edit in edits {
            match *edit {
                ObjectEdit::Translation(object, translation) => {
                    self.local_transform(object)?;
                    LocalTransform {
                        translation,
                        ..Default::default()
                    }
                    .validate()?;
                }
                ObjectEdit::Rotation(object, rotation) => {
                    self.local_transform(object)?;
                    LocalTransform {
                        rotation,
                        ..Default::default()
                    }
                    .validate()?;
                }
                ObjectEdit::MaterialSlot(object, _) => {
                    self.local_transform(object)?;
                    if self.renderer(object)?.is_none() {
                        renderers.insert(object);
                    }
                }
            }
        }
        self.components.check_additional(renderers.len())
    }
}
