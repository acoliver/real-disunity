//! Scene-local instance identity and deterministic sparse placement storage.

pub(crate) mod batch;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use worldrunner_render_plan::{RenderScene, SceneObject, SceneTransform, SceneValidationError};

use crate::pipeline::PipelineKey;
pub(crate) use batch::InstanceBatch;

/// Opaque identity of one placement in one resident scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct InstanceHandle {
    scene: u64,
    slot: usize,
    generation: u64,
}

/// An instance edit was rejected without changing the scene.
#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    /// The handle belongs to another resident scene.
    #[error("instance handle belongs to another scene")]
    ForeignHandle,
    /// The placement was removed or its slot has since been reused.
    #[error("instance handle is stale")]
    StaleHandle,
    /// The object refers to a resource outside the resident tables.
    #[error("instance {resource} index {index} is not resident")]
    Resource {
        /// Resource table name.
        resource: &'static str,
        /// Rejected table index.
        index: u32,
    },
    /// An in-place edit attempted to change the placement's batch resources.
    #[error("in-place instance replacement must keep its mesh and material")]
    ResourcesChanged,
    /// The transform violates the scene domain's invariants.
    #[error(transparent)]
    Transform(#[from] SceneValidationError),
}

/// One actual queue write, expressed in instance slots rather than bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceWrite {
    /// Resident mesh table index.
    pub mesh: usize,
    /// Resident material table index.
    pub material: usize,
    /// Half-open range within that mesh/material batch.
    pub instances: std::ops::Range<u32>,
}

pub(crate) type BatchKey = (usize, usize, usize);

#[derive(Debug)]
struct Slot {
    generation: u64,
    location: Option<(BatchKey, usize)>,
}

#[derive(Debug)]
pub(crate) struct Instances {
    scene: u64,
    slots: Vec<Slot>,
    free: BTreeSet<usize>,
    mesh_count: usize,
    pub(crate) pipelines: Vec<usize>,
    /// Whether velocity pipelines draw, so batches keep previous-frame transforms.
    pub track_previous: bool,
    pub wind_extents: Vec<glam::Vec3>,
    pub curved_wind: Vec<Option<glam::Vec3>>,
    pub impostor_extents: Vec<[f32; 3]>,
    pub batches: BTreeMap<BatchKey, InstanceBatch>,
}

impl Instances {
    pub fn new(scene: &RenderScene) -> Self {
        static NEXT_SCENE: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT_SCENE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("resident scene identity exhausted");
        let (wind_extents, curved_wind) = wind_bounds(scene);
        let mut instances = Self {
            scene: identity,
            slots: Vec::new(),
            free: BTreeSet::new(),
            mesh_count: scene.geometry().meshes().len(),
            pipelines: scene
                .resources()
                .materials()
                .iter()
                .map(|material| PipelineKey::from_material(material).index())
                .collect(),
            track_previous: false,
            wind_extents,
            curved_wind,
            impostor_extents: scene
                .resources()
                .materials()
                .iter()
                .map(|material| {
                    material.impostor().map_or([0.0; 3], |atlas| {
                        let [width, height] = atlas
                            .plane_size_millimeters()
                            .map(|v| crate::geometry::millimeter_to_meter(i64::from(v)));
                        [
                            if atlas.borders().is_some() {
                                width.hypot(height) * 0.5
                            } else {
                                0.0
                            },
                            atlas.relief().map_or(0.0, |(_, scale)| {
                                crate::geometry::millimeter_to_meter(i64::from(scale))
                            }),
                            glam::Vec3::from_array(
                                atlas
                                    .capture_center_millimeters()
                                    .map(|v| crate::geometry::millimeter_to_meter(i64::from(v))),
                            )
                            .length(),
                        ]
                    })
                })
                .collect(),
            batches: BTreeMap::new(),
        };
        for object in scene.geometry().objects() {
            instances
                .add(*object, true)
                .expect("validated scene object");
        }
        instances
    }

    /// Keeps previous-frame transforms for velocity pipelines.
    pub fn tracking_previous(mut self, track: bool) -> Self {
        self.track_previous = track;
        self
    }

    pub fn add(
        &mut self,
        object: SceneObject,
        visible: bool,
    ) -> Result<InstanceHandle, InstanceError> {
        object.transform().validate()?;
        let mesh = usize::try_from(object.mesh().raw()).expect("u32 table index fits usize");
        let material =
            usize::try_from(object.material().raw()).expect("u32 table index fits usize");
        if mesh >= self.mesh_count {
            return Err(InstanceError::Resource {
                resource: "mesh",
                index: object.mesh().raw(),
            });
        }
        let pipeline = *self
            .pipelines
            .get(material)
            .ok_or(InstanceError::Resource {
                resource: "material",
                index: object.material().raw(),
            })?;
        let key = (pipeline, mesh, material);
        let position = self.batches.entry(key).or_default().add(object, visible);
        let slot = self.free.pop_first().unwrap_or_else(|| {
            self.slots.push(Slot {
                generation: 0,
                location: None,
            });
            self.slots.len() - 1
        });
        self.slots[slot].location = Some((key, position));
        Ok(self.handle(slot))
    }

    pub fn handles(&self) -> impl Iterator<Item = InstanceHandle> + '_ {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.location.is_some())
            .map(|(index, _)| self.handle(index))
    }

    fn handle(&self, slot: usize) -> InstanceHandle {
        InstanceHandle {
            scene: self.scene,
            slot,
            generation: self.slots[slot].generation,
        }
    }

    fn location(&self, handle: InstanceHandle) -> Result<(BatchKey, usize), InstanceError> {
        if handle.scene != self.scene {
            return Err(InstanceError::ForeignHandle);
        }
        let slot = self
            .slots
            .get(handle.slot)
            .ok_or(InstanceError::StaleHandle)?;
        if slot.generation != handle.generation {
            return Err(InstanceError::StaleHandle);
        }
        slot.location.ok_or(InstanceError::StaleHandle)
    }

    pub fn get(&self, handle: InstanceHandle) -> Result<(SceneObject, bool), InstanceError> {
        let (key, position) = self.location(handle)?;
        Ok(self.batches[&key].get(position))
    }

    pub fn remove(&mut self, handle: InstanceHandle) -> Result<(), InstanceError> {
        let (key, position) = self.location(handle)?;
        let slot = &mut self.slots[handle.slot];
        slot.generation = slot
            .generation
            .checked_add(1)
            .expect("instance generation exhausted");
        slot.location = None;
        self.free.insert(handle.slot);
        self.batches
            .get_mut(&key)
            .expect("live batch")
            .remove(position);
        Ok(())
    }

    pub fn set_transform(
        &mut self,
        handle: InstanceHandle,
        transform: SceneTransform,
    ) -> Result<(), InstanceError> {
        let (key, position) = self.location(handle)?;
        transform.validate()?;
        self.batches
            .get_mut(&key)
            .expect("live batch")
            .set_transform(position, transform);
        Ok(())
    }

    pub fn set_object(
        &mut self,
        handle: InstanceHandle,
        object: SceneObject,
    ) -> Result<(), InstanceError> {
        let (key, position) = self.location(handle)?;
        object.transform().validate()?;
        let batch = self.batches.get_mut(&key).expect("live batch");
        let old = batch.get(position).0;
        if old.mesh() != object.mesh() || old.material() != object.material() {
            return Err(InstanceError::ResourcesChanged);
        }
        batch.set_object(position, object);
        Ok(())
    }

    pub fn set_visible(
        &mut self,
        handle: InstanceHandle,
        visible: bool,
    ) -> Result<(), InstanceError> {
        let (key, position) = self.location(handle)?;
        self.batches
            .get_mut(&key)
            .expect("live batch")
            .set_visible(position, visible);
        Ok(())
    }
}

fn wind_bounds(scene: &RenderScene) -> (Vec<glam::Vec3>, Vec<Option<glam::Vec3>>) {
    scene
        .resources()
        .materials()
        .iter()
        .map(|material| {
            let wind = material.features().and_then(|features| features.wind);
            (
                wind.map_or(glam::Vec3::ZERO, crate::pipeline::features::wind_extent),
                wind.and_then(crate::pipeline::features::foliage_origin),
            )
        })
        .unzip()
}
