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
use crate::geometry::{GeometryError, pack_indices, pack_vertices};
use crate::renderer::ScenePipelines;
use crate::terrain::{TerrainError, build_terrain_mesh};
use crate::terrain_uniform::{TerrainUniformError, TerrainUniformState};
use crate::texture::{PreparedTexture, TextureError, prepare_texture};
use crate::upload::{
    FRAME_GROUP, MATERIAL_GROUP, TERRAIN_GROUP, material_sampler, placeholder_with_view,
    upload_buffer, upload_palette_array, upload_texture, upload_type_grid,
};

mod append;
pub use append::{
    PreparedAppend, PreparedMeshReplacement, PreparedTextureReplacement, ResourceBase,
};
mod caster_edits;
mod celestial;
mod compaction;
mod contact;
pub(crate) mod decals;
pub use decals::validate_decal_materials;
mod descriptors;
mod dynamic;
mod lod_material;
pub(crate) mod overlay;
mod pages;
pub(crate) mod pass_statistics;
mod programs;
mod publication;
mod retained;
pub use retained::reflections_enabled;
mod effect_statistics;
pub(crate) mod geometry_statistics;
mod statistics;
pub use effect_statistics::{
    EffectEnabled, EffectStatistics, PassExecution, PassInstallation, ReceiverBinding,
    ReceiverBindings, ShadowStatistics, SsaoStatistics,
};
pub use geometry_statistics::{
    TerrainGeometryStatistics, TerrainLeafIdentity, TerrainStreamingStatistics,
};
pub use pass_statistics::PassDrawStatistics;
mod streaming;
pub(crate) use publication::{ColorDraw, DrawPublications, SelectionPublication};
pub use statistics::{DrawCount, ResourceStatistics, SceneStatistics, TerrainPageStatistics};
pub use streaming::TerrainGeometryRequest;
mod visibility;
use crate::instances::Instances;
pub use crate::instances::{InstanceError, InstanceHandle, InstanceWrite};
use resources::prepared_terrain;
use visibility::MeshSelection;
pub use visibility::ViewStats;

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
    /// Exact triangle source exceeds the device's buffer or storage binding limit.
    #[error("terrain triangle source needs {required} bytes, device limit is {limit}")]
    TerrainPlaneLimit { required: u64, limit: u64 },
    #[error("terrain instance buffer byte size overflow")]
    TerrainInstanceSizeOverflow,
    /// Terrain instances exceed the device's vertex buffer size limit.
    #[error("terrain instances need {required} bytes, device buffer limit is {limit}")]
    TerrainInstanceLimit { required: u64, limit: u64 },
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
    /// An appended or replaced resource depends on state the whole resident scene shares, so
    /// only a full replacement can take it.
    #[error("{item} {index} needs the resident scene rebuilt: {reason}")]
    NeedsRebuild {
        /// Which kind of resource: texture, material, mesh or append.
        item: &'static str,
        /// Its table index.
        index: usize,
        /// What it depends on.
        reason: &'static str,
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
    /// Exact f32 heightfield and chunk descriptors for analytic filtering.
    triangle_source: Buffer,
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
    pub(crate) pass_counters: pass_statistics::PassCounters,
    effect_statistics: std::rc::Rc<Cell<EffectStatistics>>,
    resource_counters: statistics::ResourceCounters,
    decal_counts: std::rc::Rc<Cell<[usize; 2]>>,
    decals: decals::Decals,
    sky: Option<crate::sky::Sky>,
    celestial: Option<crate::sky::meshes::Meshes>,
    casters: caster_edits::CasterEdits,
    pub(crate) ssao: Option<worldrunner_render_plan::SceneSsao>,
    pub(crate) post: worldrunner_render_post::HdrPostSettings,
    pub(crate) ssr: Option<worldrunner_render_plan::SceneSsr>,
    pub(crate) water: Vec<worldrunner_render_plan::SceneWaterBody>,
    pub(crate) water_inputs: Vec<[Option<TextureView>; 3]>,
    frame_buffer: Buffer,
    frame_group: BindGroup,
    frame_slots: Vec<(Buffer, BindGroup)>,
    /// The committed frame group and the frame slot whose uniform it binds.
    committed_frame_group: std::rc::Rc<RefCell<(BindGroup, usize)>>,
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
    views: [selection_reuse::View; 5],
    caster_fits: selection_reuse::CasterFits,
    terrain: Option<GpuTerrain>,
    pub(crate) overlay: Option<std::rc::Rc<overlay::OverlayGpu>>,
    dynamic: crate::dynamic::DynamicGpu,
    unlit: crate::dynamic::UnlitRuntime,
    programs: Option<worldrunner_render_program::runtime::AdmittedPrograms>,
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
        prepared.validate_device(device, pipelines)?;
        let contact_heights = contact::height_buffer(device, queue, &prepared.contact)?;
        let (textures, views) = upload_textures(device, authority, &prepared.textures)?;
        let (placeholder, placeholder_view) = placeholder_with_view(device, queue);
        let sampler = material_sampler(device);
        let (materials, material_uniforms) = resources::scene_materials(
            device,
            queue,
            scene,
            pipelines,
            (&views, &placeholder_view),
            &contact_heights,
        );
        let meshes = upload_meshes(device, queue, &prepared.meshes);
        let terrain =
            prepared_terrain(device, authority, &prepared, pipelines, &sampler, previous)?;
        let (decals, frame_slots) =
            decals::Decals::allocate(device, queue, &prepared, pipelines.frame_layout())?;
        let gpu = Self {
            pass_counters: pass_statistics::PassCounters::default(),
            effect_statistics: std::rc::Rc::default(),
            resource_counters: statistics::ResourceCounters::new(&prepared),
            decal_counts: std::rc::Rc::default(),
            decals,
            celestial: celestial::upload(device, scene, pipelines, &meshes, &materials),
            sky: crate::sky::Sky::upload(device, scene, pipelines, &views, &placeholder_view)?,
            casters: caster_edits::CasterEdits::default(),
            post: resources::post_settings(scene)?,
            ssr: scene.effects().and_then(|effects| effects.ssr()).copied(),
            ssao: scene.effects().and_then(|effects| effects.ssao()).copied(),
            water: water_bodies(scene),
            water_inputs: crate::water::surface::inputs(scene, &views),
            committed_frame_group: std::rc::Rc::new(RefCell::new((frame_slots[0].1.clone(), 0))),
            frame_buffer: frame_slots[0].0.clone(),
            frame_group: frame_slots[0].1.clone(),
            frame_slots,
            materials,
            meshes,
            mesh_selection: prepared.selection,
            instances: Instances::new(scene).tracking_previous(pipelines.writes_velocity()),
            frame: std::rc::Rc::new(Cell::new(None)),
            drawing_frame: RefCell::new(None),
            views: Default::default(),
            caster_fits: selection_reuse::CasterFits::default(),
            terrain,
            overlay: previous.and_then(|gpu| gpu.overlay.clone()),
            dynamic: crate::dynamic::DynamicGpu::default(),
            unlit: crate::dynamic::UnlitRuntime::default(),
            programs: None,
            retained: Retained {
                contact_heights,
                textures,
                placeholder,
                sampler,
                material_uniforms,
            },
        };
        gpu.finish_upload(device, authority, pipelines, scene)
    }

    /// Writes the per-frame uniform the next draw will read.
    pub fn set_frame(&self, queue: &Queue, frame: FrameUniform) {
        let bytes = self.decals.prepare(&frame, &self.frame_buffer);
        self.decal_counts
            .set([decals::selected_count(&bytes), self.decals.count()]);
        self.frame.set(Some(frame));
        *self.committed_frame_group.borrow_mut() = (self.decals.group(), self.decals.slot);
        *self.drawing_frame.borrow_mut() = None;
        queue.write_buffer(&self.frame_buffer, 0, &frame.to_bytes());
        queue.write_buffer(
            self.decals.previous_buffer(),
            0,
            &frame.previous_frame_bytes(),
        );
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
    /// Panics when celestial meshes are present, before recording any draws, and when resident
    /// dynamic quads were staged with pipelines for a pass other than the one `pipelines`
    /// describe: staging prepares their draws for the pass of the pipelines it is given. The
    /// frame entry points ([`crate::compositor::encode_scene`] and the shadowed scene) prepare
    /// them for their own pass instead.
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
        // Both passes consume the same camera selection, compacted instances and terrain stream.
        // The forward-only draws are appended after that paired prefix.
        pass.set_bind_group(3, group, &[]);
        self.draw_color_pass(pass, pipelines, None, false).selection
    }

    pub(crate) fn draw_shadowed(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: &BindGroup,
    ) -> ColorDraw {
        self.draw_color(pass, pipelines, Some(shadow))
    }

    pub(crate) fn draw_opaque_capture(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
    ) {
        self.draw_color_pass(pass, pipelines, shadow, false);
    }

    pub(crate) fn draw_color(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
    ) -> ColorDraw {
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

    // Pair both passes here so object batches, compacted instances and terrain primitives
    // retain the same API order. Stencil consumption depends on that order for depth ties.
    fn draw_color_pass(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
        dynamic: bool,
    ) -> ColorDraw {
        if pipelines.depth != crate::renderer::depth::DepthMode::Independent {
            pass.set_stencil_reference(1);
        }
        let frame = self.drawing_uniform();
        let receiver = self
            .ssao
            .map_or(worldrunner_render_plan::SceneSsaoReceiver::Nearest, |ao| {
                ao.receiver
            });
        let selected = pipelines.for_receiver(receiver);
        let selected = selected.for_raster(frame.specular_convention, frame.front_face());
        let pipelines = &selected;
        let frame_group = self.drawing_frame_group();
        let selection = self.draw_objects(pass, &frame_group, pipelines.object_pipelines(), None);
        let (terrain, terrain_bound) = self.draw_terrain_color(pass, pipelines, shadow);
        let mut sky_count = DrawCount::default();
        if dynamic {
            if let Some(sky) = &self.sky {
                sky_count = sky.draw_targets(pass, &frame_group, pipelines.sky_targets());
            }
            if let Some(meshes) = &self.celestial {
                self.sky
                    .as_ref()
                    .expect("celestial atmosphere")
                    .bind_background(pass, &frame_group);
                meshes.draw(pass, pipelines.sky_targets());
            }
            self.dynamic
                .draw(pass, self.drawing_frame_slot(), pipelines);
        }
        ColorDraw {
            selection,
            terrain,
            sky: sky_count,
            shadows: ReceiverBindings::new(shadow.is_some(), terrain_bound && shadow.is_some()),
            ssao: ReceiverBindings::new(
                pipelines.has_ao_receiver(),
                terrain_bound && pipelines.has_ao_receiver(),
            ),
        }
    }

    fn draw_terrain_color(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
    ) -> (DrawCount, bool) {
        let mut count = DrawCount::default();
        let mut bound = false;
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
                bound = true;
                count = Self::draw_terrain_geometry(pass, terrain);
            }
        }
        (count, bound)
    }

    fn draw_terrain_geometry(pass: &mut RenderPass<'_>, terrain: &GpuTerrain) -> DrawCount {
        if let Some(streaming) = &terrain.streaming {
            return streaming.draw(pass, &terrain.instances);
        }
        let lod = terrain.lod.as_ref().expect("nonstreaming mesh");
        pass.set_vertex_buffer(0, lod.vertices.slice(..));
        pass.set_vertex_buffer(1, terrain.instances.slice(..));
        pass.set_index_buffer(lod.indices.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..lod.index_count, 0, 0..1);
        DrawCount {
            calls: 1,
            instances: 1,
        }
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
        let published = self.views[selection_reuse::view_index(shadow_matrix)]
            .published
            .clone();
        let selection = self.select_objects(shadow_matrix, &frame);
        pass.set_bind_group(FRAME_GROUP, frame_group, &[]);
        let count =
            self.record_selection(pass, pipelines, (shadow_matrix, &frame), &selection.ranges);
        if let Some((index, _)) = shadow_matrix {
            self.pass_counters.shadows[index as usize].record(count);
        }
        SelectionPublication {
            destination: published,
            selection,
        }
    }

    /// Owned statistics from the most recently submitted main-camera pass.
    pub fn view_stats(&self) -> ViewStats {
        self.views[selection_reuse::CAMERA]
            .published
            .borrow()
            .stats
            .clone()
    }

    /// Owned statistics from the most recently submitted shadow cascade zero.
    pub fn shadow_view_stats(&self) -> ViewStats {
        self.cascade_view_stats(0)
    }

    /// Owned statistics for one of the four supported shadow view indices.
    pub fn cascade_view_stats(&self, index: usize) -> ViewStats {
        self.views[index + 1].published.borrow().stats.clone()
    }

    /// Relief fragment-depth equivalence must be proved before consuming its metadata winner.
    pub(crate) fn has_relief(&self) -> bool {
        visibility::has_relief(&self.instances, &self.mesh_selection)
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
mod environment;
mod prepared;
mod resources;
mod selection_reuse;
mod slice_hull;
pub(crate) use slice_hull::SliceHull;
#[cfg(test)]
pub(crate) use slice_hull::UNCULLED as UNCULLED_CASCADES;
pub mod source_environment;
pub use prepared::PreparedScene;

#[cfg(test)]
pub(crate) mod tests;

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

#[cfg(test)]
mod statistics_tests;
