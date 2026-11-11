//! What the world holds for one frame of offscreen work, copied out of it.
//!
//! The adapter maps this plain data, so it does not need the world while it works, and a host
//! whose world lives in another image can fill the same fields. [`GraphicsSnapshot::take`] reads
//! them from a world in this process and drains the frame's records.
//!
//! The renderers are listed without the presented camera's culling. The presentation's own list
//! of mesh objects leaves out the layers the screen camera does not draw, and an offscreen camera
//! draws layers of its own. Each renderer here is still tested against the offscreen cameras'
//! culling masks, here for the ones the frame has and again, camera by camera, when the frame is
//! built.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    ComputeBufferId, ComputeShaderId, GlobalShaderState, MaterialId, OffscreenWork,
    RenderTextureId, ShaderId, ShaderProperties, WorldComputeBuffer, WorldComputeShader,
    WorldMaterial, WorldRenderTexture, WorldShader,
};
use crate::presentation::Camera;
use crate::{ComponentDesc, ComponentHandle, RendererKind, World, WorldError};

/// A camera a record names: its settings and its object's pose in the world, column-major.
#[derive(Clone, Debug, PartialEq)]
pub struct CameraView {
    pub settings: Camera,
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
    /// The world material of each slot; a slot has none when it never had one or its material
    /// was destroyed.
    pub materials: Vec<Option<MaterialId>>,
    pub render_queue: Option<i32>,
    pub property_block: Option<ShaderProperties>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphicsSnapshot {
    pub shaders: BTreeMap<ShaderId, WorldShader>,
    pub materials: BTreeMap<MaterialId, WorldMaterial>,
    /// Every live render texture, and each one destroyed after a record of the frame named it.
    pub render_textures: BTreeMap<RenderTextureId, WorldRenderTexture>,
    pub global: Option<GlobalShaderState>,
    /// The frame's records, in call order.
    pub work: Vec<OffscreenWork>,
    /// The live cameras that camera renders, camera setups and culls name.
    pub cameras: BTreeMap<ComponentHandle, CameraView>,
    pub renderers: Vec<RendererView>,
    /// The live compute shaders that the frame's dispatches name.
    pub compute_shaders: BTreeMap<ComputeShaderId, WorldComputeShader>,
    /// The live compute buffers that the frame's dispatches bind, with the contents they hold
    /// when the snapshot is taken.
    pub compute_buffers: BTreeMap<ComputeBufferId, WorldComputeBuffer>,
}

impl GraphicsSnapshot {
    /// Drains the world's offscreen work and copies what that work can reach: the cameras it
    /// names, the renderers they may draw, the materials and shaders of those renderers and of
    /// the blits, and the compute shaders and buffers of the dispatches. A frame with no work
    /// copies nothing else.
    ///
    /// # Errors
    /// Fails when the world refuses to describe an object it listed.
    pub fn take(world: &mut World) -> Result<Self, WorldError> {
        let retired = world.take_retired_render_textures();
        let work = world.take_offscreen();
        let mut snapshot = Self {
            work,
            ..Self::default()
        };
        if snapshot.work.is_empty() {
            return Ok(snapshot);
        }
        snapshot.global = Some(world.global_shader_state().clone());
        snapshot.render_textures = world
            .render_textures()
            .map(|(id, texture)| (id, *texture))
            .chain(retired)
            .collect();
        snapshot.cameras = camera_views(world, &snapshot.work);
        let mask = snapshot
            .cameras
            .values()
            .fold(cull_masks(&snapshot.work), |mask, camera| {
                mask | camera.settings.culling_mask
            });
        if mask != 0 {
            snapshot.renderers = renderer_views(world, mask)?;
        }
        snapshot.copy_materials(world);
        snapshot.copy_compute(world);
        Ok(snapshot)
    }

    /// The materials the blits, mesh draws and renderers name, and their shaders. A destroyed material
    /// or shader is left out.
    fn copy_materials(&mut self, world: &World) {
        let named: BTreeSet<MaterialId> = self
            .work
            .iter()
            .filter_map(|work| match work {
                OffscreenWork::Blit { material, .. } => *material,
                OffscreenWork::DrawMesh { material, .. }
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
            let Ok(material) = world.material(id) else {
                continue;
            };
            if let Ok(shader) = world.shader(material.shader) {
                self.shaders.insert(material.shader, shader.clone());
            }
            self.materials.insert(id, material.clone());
        }
    }

    /// The compute shaders the dispatches run and the buffers they bind. A destroyed shader or
    /// buffer is left out, and the record that names it stays in `work`.
    fn copy_compute(&mut self, world: &World) {
        for record in &self.work {
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
                if let Ok(held) = world.compute_buffer(*buffer) {
                    self.compute_buffers.insert(*buffer, held.clone());
                }
            }
        }
    }
}

/// The layers the frame's culls admit, as their own masks give them.
fn cull_masks(work: &[OffscreenWork]) -> u32 {
    work.iter().fold(0, |mask, record| match record {
        OffscreenWork::Cull { culling_mask, .. } => mask | culling_mask,
        _ => mask,
    })
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
            },
        );
    }
    views
}

/// The enabled mesh renderers on active objects whose layer `mask` admits.
fn renderer_views(world: &World, mask: u32) -> Result<Vec<RendererView>, WorldError> {
    let mut meshes = BTreeMap::new();
    for component in world.published_components() {
        if let ComponentDesc::MeshFilter(filter) = &component.desc
            && filter.drawn_mesh != 0
        {
            meshes.insert(component.object, filter.drawn_mesh);
        }
    }
    let mut views = Vec::new();
    for component in world.published_components() {
        let ComponentDesc::MeshRenderer(renderer) = &component.desc else {
            continue;
        };
        let shown = renderer.kind == RendererKind::Mesh
            && renderer.switches.enabled
            && !renderer.switches.force_rendering_off;
        let Some(mesh) = meshes.get(&component.object).copied().filter(|_| shown) else {
            continue;
        };
        let layer = u32::from(world.object(component.object)?.layer);
        if mask.checked_shr(layer).is_none_or(|bits| bits & 1 == 0)
            || !world.active_in_hierarchy(component.object)?
        {
            continue;
        }
        let slots = renderer.material_count as usize;
        views.push(RendererView {
            local_to_world: world.presented_matrix(component.object)?.to_cols_array(),
            mesh,
            layer,
            materials: (0..slots)
                .map(|slot| {
                    let held = renderer.slot_materials.get(slot).copied().flatten();
                    held.filter(|id| world.material(*id).is_ok())
                })
                .collect(),
            render_queue: renderer.render_queue,
            property_block: renderer.property_block.clone(),
        });
    }
    Ok(views)
}
