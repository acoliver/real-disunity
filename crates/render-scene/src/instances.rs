//! Scene-local instance identity and deterministic sparse placement storage.

pub(crate) mod batch;
#[cfg(test)]
mod contour_tests;
pub(crate) mod fade;
mod flushed_memo;
pub(crate) mod history;
mod lookup;
mod ready_memo;
mod settled_memo;
pub(crate) mod skin;
pub(crate) mod skin_stream;
pub(crate) mod slot_bounds;
mod staged_uploads;
#[cfg(test)]
mod test_hooks;
#[cfg(test)]
pub(crate) mod test_owner;
#[cfg(test)]
mod tests;
mod totals;

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_plan::{
    RenderScene, SceneMaterial, SceneObject, SceneTransform, SceneValidationError,
};

use crate::pipeline::PipelineKey;
pub(crate) use batch::InstanceBatch;
pub(crate) use lookup::Ascending;

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

/// What cached views of the placements and their buffers depend on, read together.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub edits: u64,
    pub content: u64,
    pub skinned: bool,
}

/// The state of the placements and tables that what batches draw and cached batch bounds derive
/// from. Placement edits and table changes only raise their counters, so an equal stamp means
/// neither moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayoutStamp {
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
    /// Every batch is made by `batch_for`, which counts it in `totals` and `skinned`; a batch put
    /// in the map any other way is outside those sums.
    pub batches: BTreeMap<BatchKey, InstanceBatch>,
    /// The sums of the counters of the batches.
    totals: totals::Totals,
    /// How many batches draw a skinned mesh.
    skinned: usize,
    /// Counts the changes to the material tables and mesh bounds that are not placement edits.
    tables: u64,
    /// The state in which every batch's cached bounds were last found current.
    bounds_current: std::cell::Cell<Option<LayoutStamp>>,
    /// How far the eye has travelled, which tells every batch's level memo what still holds.
    eye_path: std::cell::Cell<slot_bounds::EyePath>,
    ready: ready_memo::ReadyMemo,
    /// The state in which a flush last found every batch settled.
    settled: settled_memo::SettledMemo,
    /// The state in which every batch was last found flushed, or not.
    flushed: flushed_memo::FlushedMemo,
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
            totals: totals::Totals::default(),
            skinned: 0,
            tables: 0,
            bounds_current: std::cell::Cell::new(None),
            eye_path: std::cell::Cell::default(),
            ready: ready_memo::ReadyMemo::default(),
            settled: settled_memo::SettledMemo::default(),
            flushed: flushed_memo::FlushedMemo::default(),
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
        self.sum(&self.totals.edits, InstanceBatch::edits)
    }

    /// `edits`, `content_generation` and `has_skinned_batches`, read together from the kept sums.
    pub fn stamp(&self) -> Stamp {
        Stamp {
            edits: self.edits(),
            content: self.content_generation(),
            skinned: self.has_skinned_batches(),
        }
    }

    /// What the cached bounds of every batch and the materials and levels they draw depend on:
    /// the placements, the material tables and mesh bounds that `extend_resources` and
    /// `forget_mesh_bounds` change, and the selection of every mesh, of which there are
    /// `mesh_count`.
    pub(crate) fn layout_stamp(&self, mesh_count: usize) -> LayoutStamp {
        LayoutStamp {
            edits: self.edits(),
            tables: self.tables,
            meshes: mesh_count,
        }
    }

    /// Whether every batch's cached bounds were found current in the state `stamp` names.
    pub(crate) fn bounds_current_at(&self, stamp: LayoutStamp) -> bool {
        self.bounds_current.get() == Some(stamp)
    }

    /// Records that every batch's cached bounds are current in the state `stamp` names.
    pub(crate) fn set_bounds_current_at(&self, stamp: LayoutStamp) {
        self.bounds_current.set(Some(stamp));
    }

    /// How far the eye has travelled when it is at the eye `key` names, which level memos
    /// compare with the margin of each entry. A new eye has travelled farther than any eye
    /// before it.
    pub(crate) fn eye_travel(&self, key: slot_bounds::LevelKey) -> u64 {
        let mut path = self.eye_path.get();
        let (travelled, restarted) = path.advance(key);
        self.eye_path.set(path);
        if restarted {
            // Entries stored under the travel before the restart may be within the margin of
            // the travel after it.
            for batch in self.batches.values() {
                batch.bounds.borrow_mut().forget_levels();
            }
        }
        travelled
    }

    /// Whether every batch can be drawn by `owner`'s transaction, or by no transaction.
    pub fn ready_for(&self, owner: Option<&Publications>) -> bool {
        self.ready.check(&self.batches, &self.totals, owner)
    }

    /// Whether any batch draws a skinned mesh.
    pub fn has_skinned_batches(&self) -> bool {
        debug_assert_eq!(
            self.skinned > 0,
            self.batches.values().any(|batch| batch.skin.is_some())
        );
        self.skinned > 0
    }

    /// Rises whenever any batch stages buffer bytes or allocates a buffer, like `edits`.
    pub fn content_generation(&self) -> u64 {
        self.sum(&self.totals.content, InstanceBatch::content)
    }

    /// `tally`, which equals the sum of `counter` over the batches.
    fn sum(&self, tally: &totals::Tally, counter: fn(&InstanceBatch) -> u64) -> u64 {
        debug_assert_eq!(tally.get(), self.batches.values().map(counter).sum::<u64>());
        tally.get()
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
        let batch = self.batch_for(key);
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

    /// The batch at `key`, made and counted in the scene's sums when the key has none.
    fn batch_for(&mut self, key: BatchKey) -> &mut InstanceBatch {
        match self.batches.entry(key) {
            Entry::Occupied(batch) => batch.into_mut(),
            Entry::Vacant(vacant) => {
                let skinned = self.skin.is_skinned(key.1);
                let mut batch = if skinned {
                    InstanceBatch::skinned()
                } else {
                    InstanceBatch::default()
                };
                batch.count_in(&self.totals);
                self.skinned += usize::from(skinned);
                vacant.insert(batch)
            }
        }
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
