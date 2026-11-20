//! Scene-local instance identity and deterministic sparse placement storage.

pub(crate) mod batch;
#[cfg(test)]
mod contour_tests;
pub(crate) mod fade;
pub(crate) mod history;
mod ready_memo;
pub(crate) mod skin;
pub(crate) mod skin_stream;
pub(crate) mod slot_bounds;
mod staged_uploads;
#[cfg(test)]
pub(crate) mod test_owner;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_plan::{
    RenderScene, SceneMaterial, SceneObject, SceneTransform, SceneValidationError,
};

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
    /// The object draws a skinned mesh and names no skin pose.
    #[error("instance draws a skinned mesh and has no skin pose")]
    MissingSkinPose,
    /// The pose does not have one entry for each joint of a skeleton the mesh is skinned to.
    #[error("skin pose has {pose_joints} joints, the mesh's skeleton has {skeleton_joints}")]
    SkinPoseJoints {
        /// Joints in the pose.
        pose_joints: u32,
        /// Joints in the skeleton of the mesh level.
        skeleton_joints: u32,
    },
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

/// The state of the placements and tables that cached batch bounds were checked in. Placement
/// edits and table changes only raise their counters, so an equal stamp means neither moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BoundsStamp {
    edits: u64,
    tables: u64,
    meshes: usize,
}

#[derive(Debug)]
pub(crate) struct Instances {
    scene: u64,
    slots: Vec<Slot>,
    free: BTreeSet<usize>,
    mesh_count: usize,
    pub(crate) pipelines: Vec<usize>,
    pub(crate) transmission: Vec<bool>,
    /// Whether velocity pipelines draw, so batches keep previous-frame transforms.
    pub track_previous: bool,
    /// The skeletons, poses and skinned meshes the placements are checked against.
    pub(crate) skin: skin::SkinTables,
    // Per-material tables; only `extend_resources` grows them. Cached placement bounds depend on them.
    pub wind_extents: Vec<glam::Vec3>,
    pub curved_wind: Vec<Option<glam::Vec3>>,
    pub impostor_extents: Vec<[f32; 3]>,
    pub fades: Vec<fade::Fade>,
    #[cfg(test)]
    pub fade_cull_disabled: bool,
    pub batches: BTreeMap<BatchKey, InstanceBatch>,
    /// Counts the changes to the material tables and mesh bounds that are not placement edits.
    tables: u64,
    /// The state in which every batch's cached bounds were last found current.
    bounds_current: std::cell::Cell<Option<BoundsStamp>>,
    ready: ready_memo::ReadyMemo,
    pub contours: worldrunner_render_plan::SceneContours,
    pub contour_members: BTreeMap<(BatchKey, usize), u8>,
    /// Counts the instance buffers; a scene built without a device keeps an unshared tally.
    pub(crate) census: worldrunner_render_gpu::census::MemoryCensus,
}

impl Instances {
    pub fn new(scene: &RenderScene) -> Self {
        static NEXT_SCENE: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT_SCENE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("resident scene identity exhausted");
        let (wind_extents, curved_wind) = wind_bounds(scene.resources().materials());
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
            skin: skin::SkinTables::new(scene),
            track_previous: false,
            transmission: scene
                .resources()
                .materials()
                .iter()
                .map(|material| material.translucency_micro() != 0)
                .collect(),
            wind_extents,
            curved_wind,
            impostor_extents: scene
                .resources()
                .materials()
                .iter()
                .map(impostor_extent)
                .collect(),
            fades: scene
                .resources()
                .materials()
                .iter()
                .map(|material| fade::Fade::new(material, scene.resources().textures()))
                .collect(),
            #[cfg(test)]
            fade_cull_disabled: false,
            batches: BTreeMap::new(),
            tables: 0,
            bounds_current: std::cell::Cell::new(None),
            ready: ready_memo::ReadyMemo::default(),
            contours: scene
                .effects()
                .map(|effects| effects.contours().clone())
                .unwrap_or_default(),
            contour_members: BTreeMap::new(),
            census: worldrunner_render_gpu::census::MemoryCensus::new(),
        };
        for object in scene.geometry().objects() {
            instances
                .add(*object, true)
                .expect("validated scene object");
        }
        instances
    }

    /// Rises with every placement edit. Batches are never removed, so the sum never repeats.
    pub fn edits(&self) -> u64 {
        self.batches.values().map(InstanceBatch::edits).sum()
    }

    /// What the cached bounds of every batch depend on: the placements, the material tables and
    /// mesh bounds that `extend_resources` and `forget_mesh_bounds` change, and the selection
    /// of every mesh, of which there are `mesh_count`.
    pub(crate) fn bounds_stamp(&self, mesh_count: usize) -> BoundsStamp {
        BoundsStamp {
            edits: self.edits(),
            tables: self.tables,
            meshes: mesh_count,
        }
    }

    /// Whether every batch's cached bounds were found current in the state `stamp` names.
    pub(crate) fn bounds_current_at(&self, stamp: BoundsStamp) -> bool {
        self.bounds_current.get() == Some(stamp)
    }

    /// Records that every batch's cached bounds are current in the state `stamp` names.
    pub(crate) fn set_bounds_current_at(&self, stamp: BoundsStamp) {
        self.bounds_current.set(Some(stamp));
    }

    /// Makes the next view check every batch's cached bounds, as every view once did.
    #[cfg(test)]
    pub(crate) fn recheck_bounds(&self) {
        self.bounds_current.set(None);
    }

    /// Whether every batch can be drawn by `owner`'s transaction, or by no transaction.
    pub fn ready_for(&self, owner: Option<&Publications>) -> bool {
        self.ready.check(&self.batches, owner)
    }

    /// How many times `ready_for` has scanned the batches rather than reusing a passing scan.
    #[cfg(test)]
    pub fn ready_scans(&self) -> usize {
        self.ready.scans()
    }

    /// Whether any batch draws a skinned mesh.
    pub fn has_skinned_batches(&self) -> bool {
        self.batches.values().any(|batch| batch.skin.is_some())
    }

    /// Rises whenever any batch stages buffer bytes or allocates a buffer, like `edits`.
    pub fn content_generation(&self) -> u64 {
        self.batches.values().map(InstanceBatch::content).sum()
    }

    /// Makes `materials` addressable after the ones the scene started with, and sets the mesh
    /// count. Placements already added keep their batches.
    pub fn extend_resources(&mut self, materials: &[SceneMaterial], mesh_count: usize) {
        let (wind, curved) = wind_bounds(materials);
        self.pipelines.extend(
            materials
                .iter()
                .map(|material| PipelineKey::from_material(material).index()),
        );
        self.wind_extents.extend(wind);
        self.transmission.extend(
            materials
                .iter()
                .map(|material| material.translucency_micro() != 0),
        );
        self.curved_wind.extend(curved);
        self.impostor_extents
            .extend(materials.iter().map(impostor_extent));
        // Appends refuse threshold coverage (`PreparedAppend::new`), so no appended material
        // needs the texture table. A path that admits it must pass the table here.
        self.fades.extend(materials.iter().map(|material| {
            assert!(
                matches!(
                    material.fade_coverage(),
                    worldrunner_render_plan::SceneFadeCoverage::Hashed
                ),
                "appended materials have hashed fade coverage"
            );
            fade::Fade::new(material, &[])
        }));
        self.mesh_count = mesh_count;
        self.tables += 1;
    }

    /// Discards the cached placement bounds of every batch that draws `mesh`.
    pub fn forget_mesh_bounds(&mut self, mesh: usize) {
        self.tables += 1;
        for ((_, batch_mesh, _), batch) in &mut self.batches {
            if *batch_mesh == mesh {
                batch.forget_bounds();
            }
        }
    }

    /// Keeps previous-frame transforms for velocity pipelines.
    pub fn tracking_previous(mut self, track: bool) -> Self {
        self.track_previous = track;
        self
    }

    /// Counts this scene's instance buffers in `census`.
    pub fn counted(mut self, census: worldrunner_render_gpu::census::MemoryCensus) -> Self {
        self.census = census;
        self
    }

    pub fn add(
        &mut self,
        object: SceneObject,
        visible: bool,
    ) -> Result<InstanceHandle, InstanceError> {
        object.transform().validate()?;
        self.contours.validate_id(object.contour_id())?;
        let descriptor = self.skin.check(&object)?;
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
        let batch = self.batches.entry(key).or_insert_with(|| {
            if self.skin.is_skinned(mesh) {
                InstanceBatch::skinned()
            } else {
                InstanceBatch::default()
            }
        });
        let position = batch.add(object, visible);
        if let Some(descriptor) = descriptor {
            batch.set_skin(position, descriptor);
        }
        let slot = self.free.pop_first().unwrap_or_else(|| {
            self.slots.push(Slot {
                generation: 0,
                location: None,
            });
            self.slots.len() - 1
        });
        self.slots[slot].location = Some((key, position));
        if object.contour_id() != 0 {
            self.contour_members
                .insert((key, position), object.contour_id());
        }
        Ok(self.handle(slot))
    }

    pub fn set_contours(
        &mut self,
        contours: worldrunner_render_plan::SceneContours,
    ) -> Result<(), SceneValidationError> {
        contours.validate()?;
        for &id in self.contour_members.values() {
            contours.validate_id(id)?;
        }
        self.contours = contours;
        Ok(())
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

    pub(crate) fn location(
        &self,
        handle: InstanceHandle,
    ) -> Result<(BatchKey, usize), InstanceError> {
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
        self.contour_members.remove(&(key, position));
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
        self.contours.validate_id(object.contour_id())?;
        let batch = self.batches.get_mut(&key).expect("live batch");
        let old = batch.get(position).0;
        if old.mesh() != object.mesh() || old.material() != object.material() {
            return Err(InstanceError::ResourcesChanged);
        }
        let descriptor = self.skin.check(&object)?;
        batch.set_object(position, object);
        if object.contour_id() == 0 {
            self.contour_members.remove(&(key, position));
        } else {
            self.contour_members
                .insert((key, position), object.contour_id());
        }
        if let Some(descriptor) = descriptor {
            batch.set_skin(position, descriptor);
        }
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

fn impostor_extent(material: &SceneMaterial) -> [f32; 3] {
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
}

fn wind_bounds(materials: &[SceneMaterial]) -> (Vec<glam::Vec3>, Vec<Option<glam::Vec3>>) {
    materials
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
