//! Retained scene uploads, pipeline-compatible batches, and draw submission.
use std::array;
use std::cell::{Cell, Ref, RefCell};

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer,
    BufferUsages, Device, IndexFormat, Queue, RenderPass, Sampler, Texture, TextureView,
    TextureViewDescriptor,
};

use worldrunner_render_plan::{MaterialHandle, RenderScene, TextureHandle};

use crate::frame::{FRAME_UNIFORM_BYTES, FrameUniform};
use crate::geometry::{GeometryError, pack_indices, pack_instance, pack_vertices};
use crate::pipeline::MaterialUniform;
use crate::renderer::ScenePipelines;
use crate::terrain::{TERRAIN_PLACEMENT, TerrainError, build_terrain_mesh};
use crate::terrain_uniform::{TerrainUniformError, TerrainUniformState};
use crate::texture::{PreparedTexture, TextureError, prepare_texture};
use crate::upload::{
    FRAME_GROUP, MATERIAL_GROUP, TERRAIN_GROUP, material_sampler, placeholder_texture,
    upload_buffer, upload_palette_array, upload_texture, upload_type_grid,
};

mod retained;
mod visibility;
use crate::instances::Instances;
pub use crate::instances::{InstanceError, InstanceHandle, InstanceWrite};
pub use visibility::ViewStats;
use visibility::{MeshSelection, Selection};

/// Why one scene could not be turned into GPU resources.
#[derive(Debug, thiserror::Error)]
pub enum DrawError {
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
#[derive(Debug)]
struct GpuLod {
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
    lod: GpuLod,
    instances: Buffer,
    bind_group: BindGroup,
    bounds: visibility::Bounds,
    #[expect(
        dead_code,
        reason = "owns the grid and palette memory the bind group reads"
    )]
    retained: Vec<Texture>,
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
    textures: Vec<Texture>,
    placeholder: Texture,
    sampler: Sampler,
    material_uniforms: Vec<Buffer>,
}

/// One scene resident on the GPU, ready to draw.
#[derive(Debug)]
pub struct SceneGpu {
    pub(crate) ssao: Option<worldrunner_render_plan::SceneSsao>,
    pub(crate) post: worldrunner_render_post::HdrPostSettings,
    pub(crate) ssr: Option<worldrunner_render_plan::SceneSsr>,
    pub(crate) water: Option<worldrunner_render_plan::SceneWaterPlane>,
    frame_buffer: Buffer,
    frame_group: BindGroup,
    frame_slots: Vec<(Buffer, BindGroup)>,
    materials: Vec<BindGroup>,
    meshes: Vec<GpuMesh>,
    mesh_selection: Vec<MeshSelection>,
    instances: Instances,
    frame: Cell<Option<FrameUniform>>,
    camera_selection: RefCell<Selection>,
    shadow_selection: [RefCell<Selection>; 4],
    terrain: Option<GpuTerrain>,
    #[expect(dead_code, reason = "owns the memory the bind groups read")]
    retained: Retained,
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
        queue: &Queue,
        scene: &RenderScene,
        pipelines: &ScenePipelines,
    ) -> Result<Self, DrawError> {
        validate_profile(scene, pipelines)?;
        let (textures, views) = upload_textures(device, queue, scene)?;
        let placeholder = placeholder_texture(device, queue);
        let placeholder_view = placeholder.create_view(&TextureViewDescriptor::default());
        let sampler = material_sampler(device);
        let (materials, material_uniforms) = upload_materials(
            device,
            queue,
            scene,
            &MaterialBindings {
                layout: pipelines.material_layout(),
                views: &views,
                placeholder: &placeholder_view,
                sampler: &sampler,
            },
        );
        let meshes = upload_meshes(device, queue, scene)?;
        let instances = Instances::new(scene);
        let mesh_selection = scene
            .geometry()
            .meshes()
            .iter()
            .map(MeshSelection::new)
            .collect();
        let terrain = upload_terrain(device, queue, scene, pipelines.terrain_layout(), &sampler)?;
        let frame_slots = allocate_frame_slots(device, queue, pipelines.frame_layout());
        let (frame_buffer, frame_group) = frame_slots[0].clone();
        let mut gpu = Self {
            post: scene
                .effects()
                .map(crate::post::settings)
                .transpose()
                .map_err(|_| crate::frame::FrameError::UnsupportedEffect("post settings"))?
                .unwrap_or_default(),
            ssr: scene.effects().and_then(|effects| effects.ssr()).copied(),
            ssao: scene.effects().and_then(|effects| effects.ssao()).copied(),
            water: scene.effects().and_then(|effects| effects.water()).copied(),
            frame_slots,
            frame_buffer,
            frame_group,
            materials,
            meshes,
            mesh_selection,
            instances,
            frame: Cell::new(None),
            camera_selection: RefCell::default(),
            shadow_selection: std::array::from_fn(|_| RefCell::default()),
            terrain,
            retained: Retained {
                textures,
                placeholder,
                sampler,
                material_uniforms,
            },
        };
        gpu.flush_instances(device, queue);
        Ok(gpu)
    }

    /// Writes the per-frame uniform the next draw will read.
    pub fn set_frame(&self, queue: &Queue, frame: FrameUniform) {
        self.frame.set(Some(frame));
        queue.write_buffer(&self.frame_buffer, 0, &frame.to_bytes());
    }

    /// Records every draw of the scene into one render pass.
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
    pub fn draw(&self, pass: &mut RenderPass<'_>, pipelines: &ScenePipelines) {
        self.draw_color(pass, pipelines, None);
    }

    pub(crate) fn draw_prepass(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        group: &BindGroup,
    ) {
        pass.set_bind_group(3, group, &[]);
        self.draw_color(pass, pipelines, None);
    }

    pub(crate) fn draw_shadowed(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: &BindGroup,
    ) {
        self.draw_color(pass, pipelines, Some(shadow));
    }

    fn draw_color(
        &self,
        pass: &mut RenderPass<'_>,
        pipelines: &ScenePipelines,
        shadow: Option<&BindGroup>,
    ) {
        self.draw_objects(pass, &self.frame_group, pipelines.object_pipelines(), None);
        if let Some(terrain) = &self.terrain {
            pass.set_pipeline(pipelines.terrain());
            pass.set_bind_group(TERRAIN_GROUP, &terrain.bind_group, &[]);
            if let Some(shadow) = shadow {
                pass.set_bind_group(2, shadow, &[]);
            }
            Self::draw_terrain_geometry(pass, terrain);
        }
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
        pass.set_vertex_buffer(0, terrain.lod.vertices.slice(..));
        pass.set_vertex_buffer(1, terrain.instances.slice(..));
        pass.set_index_buffer(terrain.lod.indices.slice(..), IndexFormat::Uint32);
        pass.draw_indexed(0..terrain.lod.index_count, 0, 0..1);
    }

    pub(crate) fn draw_objects(
        &self,
        pass: &mut RenderPass<'_>,
        frame_group: &BindGroup,
        pipelines: &[wgpu::RenderPipeline; 4],
        shadow_matrix: Option<(u32, glam::Mat4)>,
    ) {
        assert!(
            self.instances
                .batches
                .values()
                .all(|batch| !batch.has_pending()),
            "flush instance updates before drawing"
        );
        let frame = self.frame.get().expect("set a frame before drawing");
        let mut selection = if let Some((index, _)) = shadow_matrix {
            self.shadow_selection[index as usize].borrow_mut()
        } else {
            self.camera_selection.borrow_mut()
        };
        selection.rebuild(
            &self.instances,
            &self.mesh_selection,
            shadow_matrix.map_or(frame.view_projection, |(_, matrix)| matrix),
            frame.camera_position_millimeters,
        );
        pass.set_bind_group(FRAME_GROUP, frame_group, &[]);
        for range in &selection.ranges {
            let (pipeline, mesh, material) = range.key;
            let batch = &self.instances.batches[&range.key];
            let lod = &self.meshes[mesh].lods[range.lod];
            pass.set_pipeline(&pipelines[pipeline]);
            pass.set_bind_group(MATERIAL_GROUP, &self.materials[material], &[]);
            pass.set_vertex_buffer(0, lod.vertices.slice(..));
            pass.set_vertex_buffer(
                1,
                batch
                    .buffer
                    .as_ref()
                    .expect("flushed instance buffer")
                    .slice(..),
            );
            pass.set_index_buffer(lod.indices.slice(..), IndexFormat::Uint32);
            pass.draw_indexed(0..lod.index_count, 0, range.slots.clone());
        }
    }

    /// Counts from the most recently recorded main-camera pass.
    /// Drop the returned guard before recording the next main-camera draw;
    /// holding it across that draw causes a runtime borrow panic.
    pub fn view_stats(&self) -> Ref<'_, ViewStats> {
        Ref::map(self.camera_selection.borrow(), |selection| &selection.stats)
    }

    /// Counts from the most recently recorded directional shadow cascade zero.
    /// Drop the returned guard before recording the next shadow draw;
    /// holding it across that draw causes a runtime borrow panic.
    pub fn shadow_view_stats(&self) -> Ref<'_, ViewStats> {
        self.cascade_view_stats(0)
    }

    /// Counts for one of the four supported shadow view indices.
    pub fn cascade_view_stats(&self, index: usize) -> Ref<'_, ViewStats> {
        Ref::map(self.shadow_selection[index].borrow(), |selection| {
            &selection.stats
        })
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
                .filter(|(key, batch)| key.0 == index && batch.has_instances())
                .count()
        })
    }
}

fn allocate_frame_slots(
    device: &Device,
    queue: &Queue,
    layout: &BindGroupLayout,
) -> Vec<(Buffer, BindGroup)> {
    (0..3)
        .map(|_| {
            let buffer = upload_buffer(
                device,
                queue,
                "frame slot uniform",
                BufferUsages::UNIFORM | BufferUsages::COPY_SRC,
                &[0u8; FRAME_UNIFORM_BYTES],
            );
            let group = frame_group(device, layout, &buffer);
            (buffer, group)
        })
        .collect()
}

fn frame_group(device: &Device, layout: &BindGroupLayout, buffer: &Buffer) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("frame"),
        layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

/// Resolves one validated handle to its table index.
///
/// `RenderScene::validate` has already proven every handle addresses its
/// table, so this cannot fail. It is a checked conversion regardless, because
/// an `as` cast would silently yield a wrong index on a target where `usize`
/// is narrower than the handle, and a wrong index samples the wrong texture.
fn table_index(handle: TextureHandle) -> usize {
    usize::try_from(handle.raw()).expect("a validated handle indexes its table")
}

/// Uploads every scene texture and creates a view of each.
fn upload_textures(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
) -> Result<(Vec<Texture>, Vec<TextureView>), DrawError> {
    let mut textures = Vec::new();
    let mut views = Vec::new();
    for (index, texture) in scene.resources().textures().iter().enumerate() {
        let prepared =
            prepare_texture(texture).map_err(|source| DrawError::Texture { index, source })?;
        let uploaded = upload_texture(device, queue, &prepared)?;
        views.push(uploaded.create_view(&TextureViewDescriptor::default()));
        textures.push(uploaded);
    }
    Ok((textures, views))
}

/// What every material bind group shares, as opposed to its own maps.
#[derive(Clone, Copy)]
struct MaterialBindings<'a> {
    layout: &'a BindGroupLayout,
    views: &'a [TextureView],
    placeholder: &'a TextureView,
    sampler: &'a Sampler,
}

/// Builds one bind group per material, filling absent maps with the placeholder.
fn upload_materials(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
    bindings: &MaterialBindings<'_>,
) -> (Vec<BindGroup>, Vec<Buffer>) {
    let MaterialBindings {
        layout,
        views,
        placeholder,
        sampler,
    } = *bindings;
    let mut groups = Vec::new();
    let mut uniforms = Vec::new();
    for material in scene.resources().materials() {
        let optional = |handle: Option<TextureHandle>| {
            handle.map_or(placeholder, |handle| &views[table_index(handle)])
        };
        let bound = [
            &views[table_index(material.albedo())],
            optional(material.normal()),
            optional(material.roughness_metallic()),
            optional(material.ambient_occlusion()),
            optional(material.self_illumination()),
        ];
        let prepared = worldrunner_render_gpu::resource::PreparedMaterial::new(
            device,
            queue,
            worldrunner_render_gpu::resource::MaterialDescriptor {
                label: "material",
                layout,
                bindings: None,
                uniform_bytes: &MaterialUniform::from_material(material).to_bytes(),
                textures: &bound,
                sampler,
            },
        );
        groups.push(prepared.bind_group);
        uniforms.push(prepared.uniform);
    }
    (groups, uniforms)
}

/// Packs and uploads every level of every mesh.
fn upload_meshes(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
) -> Result<Vec<GpuMesh>, DrawError> {
    let mut meshes = Vec::new();
    for (mesh, source) in scene.geometry().meshes().iter().enumerate() {
        let mut lods = Vec::new();
        for (level, lod) in source.lods().iter().enumerate() {
            let vertices = pack_vertices(lod).map_err(|source| DrawError::Geometry {
                mesh,
                level,
                source,
            })?;
            let indices = pack_indices(lod);
            lods.push(GpuLod {
                vertices: upload_buffer(
                    device,
                    queue,
                    "mesh vertices",
                    BufferUsages::VERTEX,
                    &vertices,
                ),
                indices: upload_buffer(
                    device,
                    queue,
                    "mesh indices",
                    BufferUsages::INDEX,
                    &indices,
                ),
                index_count: u32::try_from(lod.indices().len())
                    .expect("a validated index count fits a draw"),
            });
        }
        meshes.push(GpuMesh { lods });
    }
    Ok(meshes)
}

/// Builds the single terrain draw: its mesh, its uniform and its bind group.
///
/// Terrain used to enter as ordinary geometry, one mesh per material, and that
/// is exactly what produced the material mosaic this replaces. It now has a
/// path of its own because it needs state no object needs: the type grid, the
/// palette arrays and the surface table the fragment shader blends through.
///
/// Returns `None` for a scene without terrain rather than an empty draw, so a
/// terrainless scene costs no pipeline switch.
fn upload_terrain(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
    layout: &BindGroupLayout,
    sampler: &Sampler,
) -> Result<Option<GpuTerrain>, DrawError> {
    let Some(terrain) = scene.terrain() else {
        return Ok(None);
    };
    let mesh = build_terrain_mesh(terrain).map_err(|source| DrawError::Terrain { source })?;
    let uniform = TerrainUniformState::encode(terrain)
        .map_err(|source| DrawError::TerrainUniform { source })?;

    // The grid goes up at full resolution regardless of the stride the mesh was
    // built at. That is the entire point: the geometry may be decimated because
    // a vertex per sample is unaffordable, but the material authority must not
    // be, or the fragment blend would be reading a grid already coarsened by
    // exactly the process it exists to correct.
    let grid = upload_type_grid(
        device,
        queue,
        terrain.columns(),
        terrain.rows(),
        terrain.surface_indices(),
    )?;
    let palettes = upload_palettes(device, queue, scene, uniform.layer_materials())?;

    // The uniform buffer is built here rather than inside the bind group so
    // that helper takes only the resources it wires together, and so the
    // buffer's lifetime is visibly the caller's.
    let uniform_buffer = upload_buffer(
        device,
        queue,
        "terrain uniform",
        BufferUsages::UNIFORM,
        uniform.bytes(),
    );
    let bind_group = terrain_bind_group(device, layout, sampler, &uniform_buffer, &grid, &palettes);
    let mut retained = palettes;
    retained.push(grid);
    Ok(Some(GpuTerrain {
        bounds: visibility::terrain_bounds(&mesh.vertices),
        lod: GpuLod {
            vertices: upload_buffer(
                device,
                queue,
                "terrain vertices",
                BufferUsages::VERTEX,
                &mesh.vertices,
            ),
            indices: upload_buffer(
                device,
                queue,
                "terrain indices",
                BufferUsages::INDEX,
                &mesh.indices,
            ),
            index_count: mesh.index_count,
        },
        // The identity placement. Terrain vertices are already in scene space,
        // and giving terrain a transform would create a second place it could
        // be positioned from.
        instances: upload_buffer(
            device,
            queue,
            "terrain instance",
            BufferUsages::VERTEX,
            &pack_instance(&TERRAIN_PLACEMENT),
        ),
        bind_group,
        retained,
    }))
}
/// Binds the terrain uniform, the type grid, the three palette arrays and the
/// sampler into the one group the terrain shader reads.
///
/// This is lifted out of the upload so the correspondence between binding
/// numbers and resources sits in one readable block. That correspondence is
/// where a mismatch hides: every one of these slots is declared with the same
/// layout as its neighbour, so a transposition validates cleanly and only shows
/// up as terrain lit by the wrong map.
fn terrain_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    sampler: &Sampler,
    uniform_buffer: &Buffer,
    grid: &Texture,
    palettes: &[Texture],
) -> BindGroup {
    let grid_view = grid.create_view(&TextureViewDescriptor::default());
    // The dimension has to be stated. A one-layer array defaults to a plain 2D
    // view, so a palette that happens to hold a single surface would fail
    // against a layout declaring an array while every larger palette passed.
    let palette_views: Vec<TextureView> = palettes
        .iter()
        .map(|texture| {
            texture.create_view(&TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..TextureViewDescriptor::default()
            })
        })
        .collect();

    let mut entries = vec![
        BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        },
        BindGroupEntry {
            binding: 1,
            resource: BindingResource::TextureView(&grid_view),
        },
    ];
    // Albedo, normal, roughness-metallic, in the order the shader declares them.
    for (slot, view) in palette_views.iter().enumerate() {
        entries.push(BindGroupEntry {
            binding: u32::try_from(slot + 2)
                .expect("three palette slots number far below u32::MAX"),
            resource: BindingResource::TextureView(view),
        });
    }
    entries.push(BindGroupEntry {
        binding: 5,
        resource: BindingResource::Sampler(sampler),
    });
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("terrain"),
        layout,
        entries: &entries,
    })
}

/// Uploads the three palette array textures, one layer per deduplicated surface.
///
/// The layer order is the order [`TerrainUniformState`] assigned, and it must
/// stay so: the surface table stores each surface's layer as an index into
/// these arrays, so reordering here would leave every surface sampling some
/// other surface's texture.
///
/// The scene textures these layers name have already been uploaded once as
/// plain 2D textures for the object path. They are prepared and uploaded a
/// second time here because an array texture cannot be assembled from separate
/// texture objects; the duplication is in GPU memory, not in the decode result.
fn upload_palettes(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
    layers: &[MaterialHandle],
) -> Result<Vec<Texture>, DrawError> {
    let materials = scene.resources().materials();
    let textures = scene.resources().textures();
    let mut prepared: [Vec<PreparedTexture>; 3] = array::from_fn(|_| Vec::new());
    for (layer, handle) in layers.iter().enumerate() {
        let material = &materials
            [usize::try_from(handle.raw()).expect("a validated handle indexes its table")];
        let maps = [
            (Some(material.albedo()), "albedo"),
            (material.normal(), "normal"),
            (material.roughness_metallic(), "roughness-metallic"),
        ];
        for (slot, (handle, map)) in maps.into_iter().enumerate() {
            let handle = handle.ok_or(DrawError::TerrainPaletteMap { layer, map })?;
            let index = table_index(handle);
            prepared[slot].push(
                prepare_texture(&textures[index])
                    .map_err(|source| DrawError::Texture { index, source })?,
            );
        }
    }
    let labels = [
        "terrain-palette-albedo",
        "terrain-palette-normal",
        "terrain-palette-rough-metal",
    ];
    prepared
        .iter()
        .zip(labels)
        .map(|(layers, label)| {
            upload_palette_array(device, queue, label, layers).map_err(DrawError::from)
        })
        .collect()
}

#[cfg(test)]
mod tests;

fn validate_profile(scene: &RenderScene, pipelines: &ScenePipelines) -> Result<(), DrawError> {
    if let Some(effects) = scene.effects() {
        crate::frame::validate_stock_effects(effects)?;
        if crate::post::requested(effects)
            && pipelines.color_format != wgpu::TextureFormat::Rgba16Float
        {
            return Err(crate::frame::FrameError::UnsupportedEffect(
                "post-processing requires HDR",
            )
            .into());
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
