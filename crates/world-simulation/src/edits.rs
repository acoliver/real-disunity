use std::collections::BTreeSet;

use glam::{Quat, Vec3};

use crate::journal::Change;
use crate::physics2d::Physics;
use crate::{
    ComponentChange, ComponentDesc, ComponentHandle, ComponentKind, LocalTransform, ObjectHandle,
    World, WorldError,
};

/// Typed edits accepted atomically by native storage, independent of any ABI command encoding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ObjectEdit {
    Translation(ObjectHandle, Vec3),
    Rotation(ObjectHandle, Quat),
    MaterialSlot(ObjectHandle, u32),
}

/// Undo log for an in-progress transaction. Only what the transaction touches is recorded, so
/// the cost follows the size of the batch and the bodies the step moves, not the world.
pub(crate) struct Staging {
    undo: Vec<Undo>,
    physics: Option<Physics>,
}

enum Undo {
    Replaced(ComponentHandle, Box<ComponentDesc>),
    Added(ComponentHandle),
}

impl World {
    /// Publishes edits and physics as one completed native state. Failure retains
    /// all prior state; the caller retains ownership of the submitted edits.
    pub fn step_with_edits(&mut self, edits: &[ObjectEdit], dt: f32) -> Result<(), WorldError> {
        if edits.is_empty() {
            return self.step_physics(dt);
        }
        self.validate_edits(edits)?;
        self.transact(true, |world| {
            world.apply_edits_unstaged(edits)?;
            world.step_physics(dt)
        })
    }

    /// Applies the batch all or nothing. A failure leaves the world and its change journal as
    /// they were.
    pub fn apply_edits(&mut self, edits: &[ObjectEdit]) -> Result<(), WorldError> {
        self.validate_edits(edits)?;
        self.transact(false, |world| world.apply_edits_unstaged(edits))
    }

    fn transact(
        &mut self,
        stage_physics: bool,
        run: impl FnOnce(&mut Self) -> Result<(), WorldError>,
    ) -> Result<(), WorldError> {
        self.staging = Some(Staging {
            undo: Vec::new(),
            physics: stage_physics.then(|| self.physics.clone()),
        });
        self.journal.begin_stage();
        let result = run(self);
        if result.is_ok() {
            self.staging = None;
            self.journal.commit_stage();
        } else {
            self.roll_back();
        }
        result
    }

    fn roll_back(&mut self) {
        self.journal.discard_stage();
        let Some(staging) = self.staging.take() else {
            return;
        };
        for undo in staging.undo.into_iter().rev() {
            match undo {
                Undo::Replaced(handle, desc) => self.restore_component(handle, *desc),
                Undo::Added(handle) => self.unwind_added_component(handle),
            }
        }
        if let Some(physics) = staging.physics {
            self.physics = physics;
        }
    }

    fn restore_component(&mut self, handle: ComponentHandle, desc: ComponentDesc) {
        let Some(slot) = self.components.get_mut(handle.0) else {
            return;
        };
        slot.desc = desc;
        let (owner, kind) = (slot.object, slot.desc.kind());
        if kind == ComponentKind::Transform {
            self.spatial_touch(owner);
        }
    }

    fn unwind_added_component(&mut self, handle: ComponentHandle) {
        let Some(component) = self.components.get(handle.0) else {
            return;
        };
        let owner = component.object;
        if matches!(component.desc, ComponentDesc::Renderer { .. })
            && let Some(object) = self.objects.get_mut(owner.0)
        {
            object.renderer = None;
        }
        if matches!(component.desc, ComponentDesc::MeshInstance(_))
            && let Some(object) = self.objects.get_mut(owner.0)
        {
            object.mesh_instance = None;
        }
        self.lifecycle.scripts.remove(&handle);
        self.lifetime.components.remove(&handle);
        self.components.unwind_insert(handle.0);
    }

    pub(crate) fn note_component_replaced(
        &mut self,
        handle: ComponentHandle,
        previous: ComponentDesc,
    ) {
        if (self.journal.enabled() || self.spatial.is_some())
            && let Some(component) = self.components.get(handle.0)
        {
            let object = component.object;
            self.record_change(match previous.kind() {
                ComponentKind::Transform => Change::Transform(object),
                kind => Change::Component(object, kind, ComponentChange::Changed),
            });
        }
        if let Some(staging) = &mut self.staging {
            staging
                .undo
                .push(Undo::Replaced(handle, Box::new(previous)));
        }
    }

    pub(crate) fn note_component_added(&mut self, handle: ComponentHandle) {
        self.note_component_presence(handle, ComponentChange::Added);
        if let Some(staging) = &mut self.staging {
            staging.undo.push(Undo::Added(handle));
        }
    }

    pub(crate) fn note_component_removed(&mut self, handle: ComponentHandle) {
        self.note_component_presence(handle, ComponentChange::Removed);
    }

    fn note_component_presence(&mut self, handle: ComponentHandle, change: ComponentChange) {
        if !self.journal.enabled() {
            return;
        }
        let Some(component) = self.components.get(handle.0) else {
            return;
        };
        let (object, kind) = (component.object, component.desc.kind());
        if kind != ComponentKind::Transform {
            self.record_change(Change::Component(object, kind, change));
        }
    }

    pub(crate) fn note_object_removed(&mut self, object: ObjectHandle) {
        self.record_change(Change::Destroyed(object));
    }

    fn apply_edits_unstaged(&mut self, edits: &[ObjectEdit]) -> Result<(), WorldError> {
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
