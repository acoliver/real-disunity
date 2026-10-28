//! Retained scene uploads, pipeline-compatible batches, and draw submission.
use std::array;
use std::cell::{Cell, RefCell};

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer,
    BufferUsages, Device, IndexFormat, Queue, RenderPass, Sampler, Texture, TextureView,
    TextureViewDescriptor,
};

use worldrunner_render_plan::{MaterialHandle, RenderScene, TextureHandle};

use crate::frame::{FRAME_UNIFORM_BYTES, FrameUniform};
use crate::geometry::{GeometryError, pack_indices, pack_instance, pack_vertices};
use crate::renderer::ScenePipelines;
use crate::terrain::{TERRAIN_PLACEMENT, TerrainError, build_terrain_mesh};
use crate::terrain_uniform::{TerrainUniformError, TerrainUniformState};
use crate::texture::{PreparedTexture, TextureError, prepare_texture};
use crate::upload::{
    FRAME_GROUP, MATERIAL_GROUP, TERRAIN_GROUP, material_sampler, placeholder_texture,
    upload_buffer, upload_palette_array, upload_texture, upload_type_grid,
};

mod celestial;
mod contact;
pub(crate) mod decals;
pub use decals::validate_decal_materials;
mod descriptors;
mod dynamic;
mod lod_material;
mod pages;
mod publication;
mod retained;
mod streaming;
pub(crate) use publication::{DrawPublications, SelectionPublication};
pub use streaming::TerrainGeometryRequest;
mod visibility;
use crate::instances::Instances;
pub use crate::instances::{InstanceError, InstanceHandle, InstanceWrite};
use resources::prepared_terrain;
pub use visibility::ViewStats;
use visibility::{MeshSelection, Selection};

#[cfg(test)]
pub(crate) struct StreamingPreparation {
    pub buffers: Vec<(Buffer, Buffer)>,
    pub uploads: usize,
}

#[cfg(test)]
thread_local! {
    pub(crate) static STREAMING_PREPARATIONS: RefCell<Option<Vec<StreamingPreparation>>> = const { RefCell::new(None) };
}

/// Why one scene could not be turned into GPU resources.
#[derive(Debug, thiserror::Error)]
pub enum DrawError {
    #[error("layered terrain is unsupported by the device limits")]
    LayeredLimit,
    #[error(transparent)]
    TerrainPages(
        #[from]
        worldrunner_render_gpu::configuration::ConfigurationError<
            worldrunner_render_gpu::configuration::DriverAllocationError,
        >,
    ),
    #[error(transparent)]
    LayeredGpu(#[from] crate::terrain::pages::layered_gpu::LayeredGpuError),
    #[error(transparent)]
    Layered(#[from] crate::terrain::pages::layered::LayeredError),
    #[error(transparent)]
    Atmosphere(#[from] crate::sky::preparation::PreparationError),
    /// The selected terrain contact samples cannot be sized in GPU address space.
    #[error("terrain contact height buffer byte size overflow")]
    ContactHeightSizeOverflow,
    /// Contact samples exceed the device buffer or storage binding limit.
    #[error("terrain contact needs {required} bytes, device limit is {limit}")]
    ContactHeightLimit {
        /// Bytes required by the selected terrain geometry.
        required: u64,
        /// Smaller of the buffer and storage binding limits.
        limit: u64,
    },
    #[error(transparent)]
    Submission(#[from] worldrunner_render_gpu::submission::SubmitError),
    /// The environmental profile requests an unsupported stock effect.
    #[error(transparent)]
    Frame(#[from] crate::frame::FrameError),
    #[error(transparent)]
    Upload(#[from] worldrunner_render_gpu::texture::UploadError),
    /// One texture payload could not be read as an uploadable container.
    #[error("scene texture {index} could not be prepared: {source}")]
    Texture {
        /// Index of the offending texture in the scene texture table.
        index: usize,
        /// The underlying container or format failure.
        #[source]
        source: TextureError,
    },
    /// One mesh level could not be packed into vertex bytes.
    #[error("mesh {mesh} level {level} could not be packed: {source}")]
    Geometry {
        /// Index of the offending mesh in the scene mesh table.
        mesh: usize,
        /// Level index within that mesh's chain.
        level: usize,
        /// The underlying packing failure.
        #[source]
        source: GeometryError,
    },
    /// The terrain heightfield could not be tessellated into its mesh.
    #[error("the scene terrain could not be tessellated: {source}")]
    Terrain {
        /// The underlying tessellation failure.
        #[source]
        source: TerrainError,
    },
    /// The terrain grid and surface table could not be encoded for the shader.
    #[error("the scene terrain could not be encoded into its uniform: {source}")]
    TerrainUniform {
        /// The underlying encoding failure.
        #[source]
        source: TerrainUniformError,
    },
    /// A terrain palette layer was missing a map the blend requires.
    ///
    /// Terrain samples all three maps unconditionally, so a layer that lacks
    /// one cannot be substituted with the placeholder the way an object
    /// material's optional map can: every layer of an array texture must agree
    /// on format and extent, and the placeholder agrees with neither.
    #[error("terrain palette layer {layer} has no {map} map")]
    TerrainPaletteMap {
        /// Index of the offending layer in the palette arrays.
        layer: usize,
        /// Which map the layer's material failed to provide.
        map: &'static str,
    },
}

/// One mesh level held as the buffer pair a draw needs.
#[derive(Clone, Debug)]
struct GpuLod {
    transition: Option<Buffer>,
    vertices: Buffer,
    indices: Buffer,
    index_count: u32,
}

/// One mesh chain, uploaded level by level.
#[derive(Debug)]
struct GpuMesh {
    lods: Vec<GpuLod>,
}

/// The terrain surface, as the one draw it has become.
///
/// There is no material index and no batch table entry here. Terrain is a
/// single mesh drawn once, with its own pipeline and its own bind group, so the
/// indirection the object path needs to share meshes between placements would
/// only obscure that.
#[derive(Debug)]
struct GpuTerrain {
    lod: Option<GpuLod>,
    streaming: Option<streaming::Streaming>,
    instances: Buffer,
    bind_group: BindGroup,
    bounds: visibility::Bounds,
    page_bounds: [f32; 4],
    page_elevation: [f32; 2],
    uniform: Buffer,
    sampler: Sampler,
    pages: Option<RefCell<crate::terrain::pages::scene::ScenePages>>,
    retained: Vec<Texture>,
    layered: Option<std::sync::Arc<crate::terrain::pages::layered_gpu::LayeredGpuSource>>,
}

/// Resources the bind groups reference.
///
/// wgpu reference-counts these internally, so dropping them here would not
/// invalidate the bind groups that use them. They are kept anyway, so that the
/// lifetime of a scene's GPU memory is visible in this type rather than
/// implied by another crate's internals.
#[derive(Debug)]
#[expect(
    dead_code,
    reason = "held solely to own the memory the bind groups read"
)]
struct Retained {
    contact_heights: Buffer,
    textures: Vec<Texture>,
    placeholder: Texture,
    sampler: Sampler,
    material_uniforms: Vec<Buffer>,
}

/// One scene resident on the GPU, ready to draw.
#[derive(Debug)]
pub struct SceneGpu {
    decals: decals::Decals,
    sky: Option<crate::sky::Sky>,
    celestial: Option<crate::sky::meshes::Meshes>,
    caster_generation: u64,
    pub(crate) ssao: Option<worldrunner_render_plan::SceneSsao>,
    pub(crate) post: worldrunner_render_post::HdrPostSettings,
    pub(crate) ssr: Option<worldrunner_render_plan::SceneSsr>,
    pub(crate) water: Vec<worldrunner_render_plan::SceneWaterBody>,
    pub(crate) water_inputs: Vec<[Option<TextureView>; 3]>,
    frame_buffer: Buffer,
    frame_group: BindGroup,
    frame_slots: Vec<(Buffer, BindGroup)>,
    committed_frame_group: std::rc::Rc<RefCell<BindGroup>>,
    materials: Vec<BindGroup>,
    meshes: Vec<GpuMesh>,
    mesh_selection: Vec<MeshSelection>,
    instances: Instances,
    frame: std::rc::Rc<Cell<Option<FrameUniform>>>,
    drawing_frame: RefCell<
        Option<(
            worldrunner_render_gpu::submission::Publications,
            FrameUniform,
        )>,
    >,
    camera_selection: std::rc::Rc<RefCell<Selection>>,
    shadow_selection: [std::rc::Rc<RefCell<Selection>>; 4],
    terrain: Option<GpuTerrain>,
    dynamic: crate::dynamic::DynamicGpu,
    retained: Retained,
}

fn mesh_selections(scene: &RenderScene) -> Vec<MeshSelection> {
    scene
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect()
}

impl SceneGpu {
    /// Uploads every texture, material, mesh and placement of one scene.
    ///
    /// # Errors
    ///
    /// Returns [`DrawError`] when a texture payload cannot be prepared for
    /// upload or a mesh level cannot be packed.
    pub fn upload(
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        scene: &RenderScene,
        pipelines: &ScenePipelines,
    ) -> Result<Self, DrawError> {
        Self::upload_prepared(device, authority, PreparedScene::new(scene)?, pipelines)
    }

    /// Uploads an owned CPU preparation without repacking source content.
    ///
    /// # Errors
    /// Returns errors for unsupported profiles or GPU upload constraints.
    pub fn upload_prepared(
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        prepared: PreparedScene,
        pipelines: &ScenePipelines,
    ) -> Result<Self, DrawError> {
        Self::upload_prepared_reusing(device, authority, prepared, pipelines, None)
    }

    /// Reuses unchanged streamed chunks from the preceding retained generation.
    pub fn upload_prepared_reusing(
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        prepared: PreparedScene,
        pipelines: &ScenePipelines,
        previous: Option<&Self>,
    ) -> Result<Self, DrawError> {
        let scene = &prepared.scene;
        let queue = authority.queue();
        validate_profile(scene, pipelines)?;
        prepared.validate_device(device, pipelines)?;
        let contact_heights = contact::height_buffer(device, queue, &prepared.contact)?;
        let (textures, views) = upload_textures(device, queue, &prepared.textures)?;
        let placeholder = placeholder_texture(device, queue);
        let placeholder_view = placeholder.create_view(&TextureViewDescriptor::default());
        let sampler = material_sampler(device);
        let (materials, material_uniforms) = resources::scene_materials(
            device,
            queue,
            scene,
            pipelines,
            (&views, &placeholder_view),
            (&sampler, &contact_heights),
        );
        let meshes = upload_meshes(device, queue, &prepared.meshes);
        let terrain =
            prepared_terrain(device, authority, &prepared, pipelines, &sampler, previous)?;
        let (decals, frame_slots) =
            decals::Decals::allocate(device, queue, &prepared, pipelines.frame_layout())?;
        let (frame_buffer, frame_group) = frame_slots[0].clone();
        let mut gpu = Self {
            decals,
            celestial: celestial::upload(device, scene, pipelines, &meshes, &materials),
            sky: crate::sky::Sky::upload(device, scene, pipelines, &views, &placeholder_view)?,
            caster_generation: 0,
            post: resources::post_settings(scene)?,
            ssr: scene.effects().and_then(|effects| effects.ssr()).copied(),
            ssao: scene.effects().and_then(|effects| effects.ssao()).copied(),
            water: water_bodies(scene),
            water_inputs: crate::water::surface::inputs(scene, &views),
            committed_frame_group: std::rc::Rc::new(RefCell::new(frame_group.clone())),
            frame_slots,
            frame_buffer,
            frame_group,
            materials,
            meshes,
            mesh_selection: prepared.selection,
            instances: Instances::new(scene),
            frame: std::rc::Rc::new(Cell::new(None)),
            drawing_frame: RefCell::new(None),
            camera_selection: std::rc::Rc::default(),
            shadow_selection: std::array::from_fn(|_| std::rc::Rc::default()),
            terrain,
            dynamic: crate::dynamic::DynamicGpu::default(),
            retained: Retained {
                contact_heights,
                textures,
                placeholder,
                sampler,
                material_uniforms,
            },
        };
        gpu.install_terrain_page_policy(device, authority, pipelines, scene)?;
        gpu.flush_instances(device, authority)?;
        Ok(gpu)
    }

    /// Writes the per-frame uniform the next draw will read.
    pub fn set_frame(&self, queue: &Queue, frame: FrameUniform) {
        let bytes = self.decals.prepare(&frame, &self.frame_buffer);
        self.frame.set(Some(frame));
        *self.committed_frame_group.borrow_mut() = self.decals.group();
        *self.drawing_frame.borrow_mut() = None;
        queue.write_buffer(&self.frame_buffer, 0, &frame.to_bytes());
        queue.write_buffer(&self.decals.buffers[self.decals.slot], 0, &bytes);
    }

    /// Records every draw of the scene into one render pass.
    /// Raw rendering does not publish selection or statistics; use composition for that.
    ///
    /// The frame group is bound once for the whole pass rather than per draw,
    /// which is sound only because all four object pipelines were built from
    /// one pipeline layout: pipelines sharing a layout do not invalidate each
    /// other's bind groups.
    ///
    /// Terrain's layout differs, but only from group one onwards: it holds the
    /// very same frame layout at group zero. Bind groups survive a pipeline
    /// switch for every slot up to the first the two layouts disagree on, so
    /// the frame binding made here outlives the switch to the terrain pipeline
    /// and only group one is invalidated — which both paths set themselves
    /// anyway. Terrain draws last so that this reasoning is needed in one
    /// direction only.
    /// Scenes with celestial meshes require [`crate::compositor::encode_scene`] or
    /// [`crate::compositor::encode_resident`] to schedule their internal visibility pass.
    ///
    /// # Panics
    /// Panics when celestial meshes are present, before recording any draws.
    pub fn draw(&self, pass: &mut RenderPass<'_>, pipelines: &ScenePipelines) {
        assert!(
            self.celestial.is_none(),
            "celestial meshes require compositor::encode_scene or encode_resident"
        );
        assert!(
            !self
                .sky
                .as_ref()
                .is_some_and(crate::sky::Sky::requires_preparation),
            "physical skies require compositor preparation"
        );
        self.draw_color(pass, pipelines, None);
    }

    pub(crate) fn draw_prepass(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        group: &BindGroup,
    ) -> SelectionPublication {
        pass.set_bind_group(3, group, &[]);
        self.draw_color_pass(pass, pipelines, None, false)
    }

    pub(crate) fn draw_shadowed(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: &BindGroup,
    ) -> SelectionPublication {
        self.draw_color(pass, pipelines, Some(shadow))
    }

    pub(crate) fn draw_color(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
    ) -> SelectionPublication {
        self.draw_color_pass(pass, pipelines, shadow, true)
    }

    fn drawing_uniform(&self) -> FrameUniform {
        self.drawing_frame
            .borrow()
            .as_ref()
            .and_then(|(owner, frame)| {
                worldrunner_render_gpu::submission::Publications::current()
                    .filter(|current| current.same_transaction(owner))
                    .map(|_| *frame)
            })
            .or_else(|| self.frame.get())
            .expect("set frame before drawing")
    }

    fn drawing_frame_group(&self) -> BindGroup {
        let transaction_frame = self
            .drawing_frame
            .borrow()
            .as_ref()
            .is_some_and(|(owner, _)| {
                worldrunner_render_gpu::submission::Publications::current()
                    .is_some_and(|current| current.same_transaction(owner))
            });
        if transaction_frame {
            self.decals.group()
        } else {
            self.committed_frame_group.borrow().clone()
        }
    }

    fn draw_color_pass(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
        dynamic: bool,
    ) -> SelectionPublication {
        let frame = self.drawing_uniform();
        let selected = pipelines.for_raster(frame.specular_convention, frame.front_face());
        let pipelines = &selected;
        let frame_group = self.drawing_frame_group();
        let selection = self.draw_objects(pass, &frame_group, pipelines.object_pipelines(), None);
        if let Some(terrain) = &self.terrain {
            let cached_group = terrain
                .pages
                .as_ref()
                .map(|pages| pages.borrow().draw_group());
            if !matches!(cached_group, Some(None)) {
                if let Some(Some(group)) = cached_group {
                    pass.set_pipeline(if terrain.layered.is_some() {
                        &pipelines
                            .layered
                            .as_ref()
                            .expect("validated layered device")
                            .cached
                    } else {
                        &pipelines.cached_terrain
                    });
                    pass.set_bind_group(TERRAIN_GROUP, &group, &[]);
                } else {
                    pass.set_pipeline(if terrain.layered.is_some() {
                        &pipelines
                            .layered
                            .as_ref()
                            .expect("validated layered device")
                            .terrain
                    } else {
                        pipelines.terrain()
                    });
                    pass.set_bind_group(TERRAIN_GROUP, &terrain.bind_group, &[]);
                }
                if let Some(shadow) = shadow {
                    pass.set_bind_group(2, shadow, &[]);
                }
                Self::draw_terrain_geometry(pass, terrain);
            }
        }
        if dynamic {
            if let Some(sky) = &self.sky {
                sky.draw(pass, &frame_group);
            }
            if let Some(meshes) = &self.celestial {
                self.sky
                    .as_ref()
                    .expect("celestial atmosphere")
                    .bind_background(pass, &frame_group);
                meshes.draw(pass);
            }
            self.dynamic.draw(pass, &frame_group, pipelines);
        }
        selection
    }

    pub(crate) fn draw_terrain_depth(
        &self,
        pass: &mut RenderPass<'_>,
        frame: &BindGroup,
        pipeline: &wgpu::RenderPipeline,
        matrix: glam::Mat4,
    ) {
        if let Some(terrain) = &self.terrain
            && visibility::Frustum::new(matrix).intersects(terrain.bounds)
        {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(FRAME_GROUP, frame, &[]);
            Self::draw_terrain_geometry(pass, terrain);
        }
    }

    fn draw_terrain_geometry(pass: &mut RenderPass<'_>, terrain: &GpuTerrain) {
        if let Some(streaming) = &terrain.streaming {
            streaming.draw(pass, &terrain.instances);
            return;
        }
        let lod = terrain.lod.as_ref().expect("nonstreaming mesh");
        pass.set_vertex_buffer(0, lod.vertices.slice(..));
        pass.set_vertex_buffer(1, terrain.instances.slice(..));
        pass.set_index_buffer(lod.indices.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..lod.index_count, 0, 0..1);
    }

    pub(crate) fn draw_objects(
        &self,
        pass: &mut RenderPass<'_>,
        frame_group: &BindGroup,
        pipelines: &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
        shadow_matrix: Option<(u32, glam::Mat4)>,
    ) -> SelectionPublication {
        assert!(
            self.instances
                .batches
                .values()
                .all(|batch| !batch.has_pending()),
            "flush instance updates before drawing"
        );
        let frame = self
            .drawing_frame
            .borrow()
            .as_ref()
            .and_then(|(owner, frame)| {
                worldrunner_render_gpu::submission::Publications::current()
                    .filter(|current| current.same_transaction(owner))
                    .map(|_| *frame)
            })
            .or_else(|| self.frame.get())
            .expect("set a frame before drawing");
        let published = if let Some((index, _)) = shadow_matrix {
            self.shadow_selection[index as usize].clone()
        } else {
            self.camera_selection.clone()
        };
        let mut selection = published.borrow().clone();
        selection.casters_only = shadow_matrix.is_some();
        selection.rebuild(
            &self.instances,
            &self.mesh_selection,
            shadow_matrix.map_or(frame.view_projection, |(_, matrix)| matrix),
            frame.camera_position_millimeters,
        );
        pass.set_bind_group(FRAME_GROUP, frame_group, &[]);
        for range in &selection.ranges {
            self.record_lod_range(pass, pipelines, range);
        }
        SelectionPublication {
            destination: published,
            selection,
        }
    }

    /// Owned statistics from the most recently submitted main-camera pass.
    pub fn view_stats(&self) -> ViewStats {
        self.camera_selection.borrow().stats.clone()
    }

    /// Owned statistics from the most recently submitted shadow cascade zero.
    pub fn shadow_view_stats(&self) -> ViewStats {
        self.cascade_view_stats(0)
    }

    /// Owned statistics for one of the four supported shadow view indices.
    pub fn cascade_view_stats(&self, index: usize) -> ViewStats {
        self.shadow_selection[index].borrow().stats.clone()
    }

    pub(crate) fn caster_matrix(&self, receiver: glam::Mat4) -> glam::Mat4 {
        visibility::caster_matrix(
            &self.instances,
            &self.mesh_selection,
            self.terrain.as_ref().map(|terrain| terrain.bounds),
            receiver,
        )
    }

    /// Returns live mesh/material batch counts per pipeline variant.
    /// Visibility holes can split a batch into several instanced draws.
    #[must_use]
    pub fn batch_counts(&self) -> [usize; 4] {
        array::from_fn(|index| {
            self.instances
                .batches
                .iter()
                .filter(|(key, batch)| key.0 % 4 == index && batch.has_instances())
                .count()
        })
    }
}

use resources::{upload_meshes, upload_textures};
mod prepared;
mod resources;
pub use prepared::PreparedScene;

#[cfg(test)]
mod tests;

fn validate_profile(scene: &RenderScene, pipelines: &ScenePipelines) -> Result<(), DrawError> {
    if let Some(effects) = scene.effects() {
        crate::frame::validate_stock_effects(effects)?;
        if matches!(
            effects.sky(),
            worldrunner_render_plan::SceneSky::DomeTexture { .. }
        ) && effects.atmosphere().is_none()
        {
            return Err(crate::frame::FrameError::UnsupportedEffect(
                "dome sky requires atmosphere",
            )
            .into());
        }
        if crate::post::requested(effects)
            && pipelines.color_format != wgpu::TextureFormat::Rgba16Float
        {
            return Err(crate::frame::FrameError::UnsupportedEffect(
                "post-processing requires HDR",
            )
            .into());
        }
        if effects.atmosphere().is_some()
            && pipelines.color_format != wgpu::TextureFormat::Rgba16Float
        {
            return Err(crate::frame::FrameError::UnsupportedEffect("sky requires HDR").into());
        }
        if effects.ssr().is_some_and(|settings| settings.enabled)
            && pipelines.color_format != wgpu::TextureFormat::Rgba16Float
        {
            return Err(crate::frame::FrameError::UnsupportedEffect("SSR requires HDR").into());
        }
        if effects.water().is_some() && pipelines.color_format != wgpu::TextureFormat::Rgba16Float {
            return Err(crate::frame::FrameError::UnsupportedEffect("water requires HDR").into());
        }
        if effects.ssao().is_some() && !pipelines.ssao {
            return Err(crate::frame::FrameError::UnsupportedEffect("SSAO").into());
        }
    }

    Ok(())
}

fn water_bodies(scene: &RenderScene) -> Vec<worldrunner_render_plan::SceneWaterBody> {
    scene
        .effects()
        .map_or_else(Vec::new, |effects| effects.water_bodies().to_vec())
}

mod fade;
