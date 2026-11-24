//! What the world holds for one frame of offscreen work, copied out of it.
//!
//! The adapter maps this plain data, so it does not need the world while it works, and a host
//! whose world lives in another image can fill the same fields. [`GraphicsSnapshot::take`] reads
//! them from a world in this process and drains the frame's records.
//!
//! The renderers are the ones the frame's camera renders and culls drew when the script called
//! them (see `render_sets`), listed without the presented camera's culling: the presentation's own
//! list of mesh objects leaves out the layers the screen camera does not draw, and an offscreen
//! camera draws layers of its own.
//!
//! A frame with a camera also copies every enabled light on an active object, in component order,
//! so the host can give the camera's shaders the light it draws with.

use std::collections::{BTreeMap, BTreeSet};

use glam::Mat4;

use super::render_sets::index_of;
use super::{
    CameraBuffers, ComputeBufferId, ComputeShaderId, Environment, GlobalShaderState, MaterialId,
    OffscreenWork, RenderTextureId, ShaderId, ShaderProperties, WorldComputeBuffer,
    WorldComputeShader, WorldMaterial, WorldRenderTexture, WorldShader,
};
use crate::presentation::{Camera, LightSettings};
use crate::{ComponentDesc, ComponentHandle, MeshRenderer, World, WorldError};

/// A camera a record names: its settings and its object's pose in the world, column-major.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraView {
    pub settings: Camera,
    pub world_matrix: [f32; 16],
    /// The buffers the camera draws into when its script set them, which replace a target texture.
    pub target_buffers: Option<CameraBuffers>,
}

/// A light that may light what a camera of the frame draws: its settings and its object's pose in
/// the world, column-major. The third column is the light's forward axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLight {
    pub settings: LightSettings,
    pub world_matrix: [f32; 16],
}

/// A mesh renderer that a camera of the frame may draw.
#[derive(Clone, Debug, PartialEq)]
pub struct RendererView {
    /// Column-major, from the mesh's own coordinates to the world.
    pub local_to_world: [f32; 16],
    /// The identity of the mesh contents, the one the host reads them by.
    pub mesh: u64,
    pub layer: u32,
    /// The renderer's shadow casting mode: 0 casts no shadow, 1 casts, 2 casts from both sides and 3
    /// casts and is not otherwise drawn.
    pub shadow_casting_mode: u32,
    /// The world material of each slot; a slot has none when it never had one or its material
    /// was destroyed.
    pub materials: Vec<Option<MaterialId>>,
    pub render_queue: Option<i32>,
    /// The values that override every slot's material.
    pub property_block: Option<ShaderProperties>,
    /// The values that override one slot's material, by slot index. A slot's draws read its own
    /// block first, then `property_block`, then the material.
    pub slot_blocks: BTreeMap<u32, ShaderProperties>,
}

impl RendererView {
    /// Every property block the renderer holds: the all-slot block, then each slot's.
    pub fn blocks(&self) -> impl Iterator<Item = &ShaderProperties> {
        self.property_block.iter().chain(self.slot_blocks.values())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphicsSnapshot {
    pub shaders: BTreeMap<ShaderId, WorldShader>,
    pub materials: BTreeMap<MaterialId, WorldMaterial>,
    /// Every live render texture, and each one destroyed after a record of the frame named it,
    /// in every snapshot, including one with no work.
    pub render_textures: BTreeMap<RenderTextureId, WorldRenderTexture>,
    /// The global state the frame's records replay from: the state as it was before the frame's
    /// first change to it, which the records after that carry on in order. `None` for a frame with
    /// no work.
    pub global: Option<GlobalShaderState>,
    /// The scene's ambient light and default reflection, copied with the global state.
    pub environment: Environment,
    /// The frame's records, in call order.
    pub work: Vec<OffscreenWork>,
    /// The live cameras that camera renders, camera setups and culls name.
    pub cameras: BTreeMap<ComponentHandle, CameraView>,
    /// The enabled lights on active objects, in component order, when the frame has a camera;
    /// none otherwise.
    pub lights: Vec<WorldLight>,
    /// The renderers the frame's camera renders and culls drew, each once, in the order the
    /// frame's sets first name them. A renderer is read as it is when the snapshot is taken.
    pub renderers: Vec<RendererView>,
    /// The sets of renderers that camera renders and culls drew when the script called them, each
    /// as indices into `renderers`. A set is listed once however many records share it.
    pub renderer_sets: Vec<Vec<u32>>,
    /// For the index of a camera render or a cull in `work`, the index in `renderer_sets` of the
    /// renderers that were drawable when the record was made. The world makes one for every cull
    /// and for every camera render whose camera it can read; a record with no entry draws nothing.
    pub record_sets: BTreeMap<u32, u32>,
    /// How many skinned renderers the frame's sets name that no view lists, each counted once
    /// however many sets name it: the renderer's skin names no root bone that is in the world, so
    /// nothing says where its mesh goes.
    pub skinned_not_drawn: u32,
    /// The live compute shaders that the frame's dispatches name.
    pub compute_shaders: BTreeMap<ComputeShaderId, WorldComputeShader>,
    /// The live compute buffers that the frame's dispatches and the materials it draws with bind,
    /// each once, with the contents they hold when the snapshot is taken.
    pub compute_buffers: BTreeMap<ComputeBufferId, WorldComputeBuffer>,
}

impl GraphicsSnapshot {
    /// The indices into `renderers` of the renderers the camera render or cull at `record` in
    /// `work` draws, or `None` when the world made no set for the record.
    pub fn record_renderers(&self, record: usize) -> Option<&[u32]> {
        let set = self.record_sets.get(&u32::try_from(record).ok()?)?;
        self.renderer_sets
            .get(usize::try_from(*set).ok()?)
            .map(Vec::as_slice)
    }

    /// Drains the world's offscreen work and copies what that work can reach: the cameras it
    /// names, the renderers they may draw, the materials and shaders of those renderers and of
    /// the blits, the buffers those materials bind, and the compute shaders and buffers of the
    /// dispatches. Every snapshot lists the live render textures; a frame with no work copies
    /// nothing else.
    ///
    /// # Errors
    /// Fails when the world refuses to describe an object it listed.
    pub fn take(world: &mut World) -> Result<Self, WorldError> {
        let retired = world.take_retired_render_textures();
        let captured = world.take_render_sets();
        let (work, copies, start) = world.take_offscreen();
        // The list is whole for every snapshot: a host drops the render textures a snapshot
        // omits, so a frame with no work must not look like a world with none.
        let render_textures = world
            .render_textures()
            .map(|(id, texture)| (id, *texture))
            .chain(retired)
            .collect();
        let mut snapshot = Self {
            render_textures,
            work,
            ..Self::default()
        };
        if snapshot.work.is_empty() {
            return Ok(snapshot);
        }
        snapshot.global = Some(start.unwrap_or_else(|| world.global_shader_state().clone()));
        snapshot.environment = world.live_environment();
        snapshot.cameras = camera_views(world, &snapshot.work);
        if !snapshot.cameras.is_empty() {
            snapshot.lights = light_views(world)?;
        }
        let drawn = renderer_views(world, &captured.sets)?;
        snapshot.renderers = drawn.views;
        snapshot.renderer_sets = drawn.sets;
        snapshot.skinned_not_drawn = index_of(drawn.unplaced.len());
        snapshot.record_sets = captured.records;
        snapshot.copy_materials(world, copies);
        snapshot.copy_material_buffers(world);
        snapshot.copy_compute(world);
        Ok(snapshot)
    }

    /// Adds the materials `ids` name, their shaders and the buffers they bind to a snapshot, for a
    /// host that draws with them in a frame whose records do not name them, such as the UI's own
    /// draws. A material a record of the snapshot already copied keeps that copy. A destroyed
    /// material or shader is left out. When something is added to a snapshot of a frame with no
    /// work, the global state and the environment are the world's now.
    pub fn include_materials(&mut self, world: &World, ids: impl IntoIterator<Item = MaterialId>) {
        let mut added = false;
        for id in ids {
            if self.materials.contains_key(&id) {
                continue;
            }
            let Ok(material) = world.material(id) else {
                continue;
            };
            if let Ok(shader) = world.shader(material.shader) {
                self.shaders.insert(material.shader, shader.clone());
            }
            self.materials.insert(id, material.clone());
            added = true;
        }
        if !added {
            return;
        }
        if self.global.is_none() {
            self.global = Some(world.global_shader_state().clone());
            self.environment = world.live_environment();
        }
        self.copy_material_buffers(world);
    }

    /// The materials the blits, mesh draws and renderers name, and their shaders. A material that
    /// changed after a draw recorded it is the draw's copy of it (`copies`, which the draw names by
    /// the copy's id). A destroyed material or shader is left out.
    fn copy_materials(&mut self, world: &World, mut copies: BTreeMap<MaterialId, WorldMaterial>) {
        let named: BTreeSet<MaterialId> = self
            .work
            .iter()
            .filter_map(|work| match work {
                OffscreenWork::Blit { material, .. } => *material,
                OffscreenWork::DrawMesh { material, .. }
                | OffscreenWork::DrawMeshBatch { material, .. }
                | OffscreenWork::DrawProcedural { material, .. } => Some(*material),
                _ => None,
            })
            .chain(
                self.renderers
                    .iter()
                    .flat_map(|r| r.materials.iter().flatten().copied()),
            )
            .collect();
        for id in named {
            let Some(material) = copies
                .remove(&id)
                .or_else(|| world.material(id).ok().cloned())
            else {
                continue;
            };
            if let Ok(shader) = world.shader(material.shader) {
                self.shaders.insert(material.shader, shader.clone());
            }
            self.materials.insert(id, material);
        }
    }

    /// The compute shaders the dispatches run and the buffers they bind. A destroyed shader or
    /// buffer is left out, and the record that names it stays in `work`.
    fn copy_compute(&mut self, world: &World) {
        for record in &self.work {
            if let OffscreenWork::DrawMeshBatch { batch, .. } = record {
                copy_buffer(&mut self.compute_buffers, world, batch.buffer);
                continue;
            }
            let OffscreenWork::Dispatch {
                shader, bindings, ..
            } = record
            else {
                continue;
            };
            if let Ok(held) = world.compute_shader(*shader) {
                self.compute_shaders.insert(*shader, held.clone());
            }
            for (_, buffer) in &bindings.buffers {
                copy_buffer(&mut self.compute_buffers, world, *buffer);
            }
        }
    }

    /// The buffers the copied materials bind. A destroyed buffer is left out; the material keeps
    /// its binding for the host to skip.
    fn copy_material_buffers(&mut self, world: &World) {
        let bound: Vec<ComputeBufferId> = self
            .materials
            .values()
            .flat_map(|material| material.buffers.iter().map(|(_, buffer)| *buffer))
            .collect();
        for buffer in bound {
            copy_buffer(&mut self.compute_buffers, world, buffer);
        }
    }
}

/// Copies a buffer into `copied` once, whichever kind of record names it. A destroyed buffer is
/// left out.
fn copy_buffer(
    copied: &mut BTreeMap<ComputeBufferId, WorldComputeBuffer>,
    world: &World,
    id: ComputeBufferId,
) {
    if copied.contains_key(&id) {
        return;
    }
    if let Ok(held) = world.compute_buffer(id) {
        copied.insert(id, held.clone());
    }
}

/// The live cameras that camera renders, camera setups and culls name.
fn camera_views(world: &World, work: &[OffscreenWork]) -> BTreeMap<ComponentHandle, CameraView> {
    let mut views = BTreeMap::new();
    for record in work {
        let (OffscreenWork::CameraRender { camera, .. }
        | OffscreenWork::SetupCamera { camera, .. }
        | OffscreenWork::Cull { camera, .. }) = record
        else {
            continue;
        };
        let Ok(component) = world.component(*camera) else {
            continue;
        };
        let (ComponentDesc::Camera(settings), Ok(pose)) =
            (&component.desc, world.world_matrix(component.object))
        else {
            continue;
        };
        views.insert(
            *camera,
            CameraView {
                settings: *settings,
                world_matrix: pose.to_cols_array(),
                target_buffers: world.camera_buffers(*camera),
            },
        );
    }
    views
}

/// The enabled lights on active objects, in component order, with their poses in the world.
fn light_views(world: &World) -> Result<Vec<WorldLight>, WorldError> {
    let mut lights = Vec::new();
    for component in world.published_components() {
        let ComponentDesc::Light(light) = &component.desc else {
            continue;
        };
        if !light.enabled || !world.active_in_hierarchy(component.object)? {
            continue;
        }
        lights.push(WorldLight {
            settings: light.settings,
            world_matrix: world.world_matrix(component.object)?.to_cols_array(),
        });
    }
    Ok(lights)
}

/// The renderers a snapshot lists, and the sets of its records as indices into them.
struct RendererViews {
    views: Vec<RendererView>,
    sets: Vec<Vec<u32>>,
    /// The skinned renderers of the sets that have no placement, so no view.
    unplaced: BTreeSet<ComponentHandle>,
}

/// The renderers of the `captured` sets, each listed once, and the sets as indices into them. A
/// renderer the world no longer holds, or that no longer has a mesh, is left out of its set, and
/// so is a skinned renderer with no placement, which is noted in `unplaced`. The renderer is read
/// as it is now: the set fixes which renderers draw, and the view says what they draw.
fn renderer_views(
    world: &World,
    captured: &[Vec<ComponentHandle>],
) -> Result<RendererViews, WorldError> {
    let mut listing = RendererViews {
        views: Vec::new(),
        sets: Vec::with_capacity(captured.len()),
        unplaced: BTreeSet::new(),
    };
    let mut index = BTreeMap::new();
    for set in captured {
        let indices = set_indices(world, set, &mut listing, &mut index)?;
        listing.sets.push(indices);
    }
    Ok(listing)
}

/// The indices in `listing.views` of the renderers of `set`, listing those that it lacks.
fn set_indices(
    world: &World,
    set: &[ComponentHandle],
    listing: &mut RendererViews,
    index: &mut BTreeMap<ComponentHandle, u32>,
) -> Result<Vec<u32>, WorldError> {
    let mut indices = Vec::with_capacity(set.len());
    for handle in set {
        if let Some(known) = index.get(handle) {
            indices.push(*known);
            continue;
        }
        let Ok(component) = world.component(*handle) else {
            continue;
        };
        let ComponentDesc::MeshRenderer(renderer) = &component.desc else {
            continue;
        };
        let Some(mesh) = world.renderer_mesh(component.object, renderer) else {
            continue;
        };
        let Some(placement) = world.renderer_placement(component.object, renderer)? else {
            listing.unplaced.insert(*handle);
            continue;
        };
        let layer = u32::from(world.object_layer(component.object)?);
        let at = index_of(listing.views.len());
        listing
            .views
            .push(renderer_view(renderer, placement, (mesh, layer), world));
        index.insert(*handle, at);
        indices.push(at);
    }
    Ok(indices)
}

/// What a camera needs of `renderer`, which draws `mesh` on `layer` at `placement`.
fn renderer_view(
    renderer: &MeshRenderer,
    placement: Mat4,
    (mesh, layer): (u64, u32),
    world: &World,
) -> RendererView {
    let slots = renderer.material_count as usize;
    RendererView {
        local_to_world: placement.to_cols_array(),
        mesh,
        layer,
        shadow_casting_mode: renderer.shadow_casting_mode,
        materials: (0..slots)
            .map(|slot| {
                let held = renderer.slot_materials.get(slot).copied().flatten();
                held.filter(|id| world.material(*id).is_ok())
            })
            .collect(),
        render_queue: renderer.render_queue,
        property_block: renderer.property_block.clone(),
        slot_blocks: renderer.slot_blocks.clone(),
    }
}
