//! Retained scene uploads, pipeline-compatible batches, and draw submission.
use std::array;
use std::collections::BTreeMap;

use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer,
    BufferUsages, Device, IndexFormat, Queue, RenderPass, Sampler, Texture, TextureView,
    TextureViewDescriptor,
};

use worldrunner_render_plan::{MaterialHandle, RenderScene, TextureHandle};

use crate::frame::{FRAME_UNIFORM_BYTES, FrameUniform};
use crate::geometry::{GeometryError, pack_indices, pack_instance, pack_vertices};
use crate::pipeline::{MaterialUniform, PipelineKey};
use crate::renderer::ScenePipelines;
use crate::terrain::{TERRAIN_PLACEMENT, TerrainError, build_terrain_mesh};
use crate::terrain_uniform::{TerrainUniformError, TerrainUniformState};
use crate::texture::{PreparedTexture, TextureError, prepare_texture};
use crate::upload::{
    FRAME_GROUP, MATERIAL_GROUP, TERRAIN_GROUP, material_sampler, placeholder_texture,
    upload_buffer, upload_palette_array, upload_texture, upload_type_grid,
};

/// The mesh level this renderer draws until selection by distance exists.
const NEAREST_LEVEL: usize = 0;

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
    #[expect(
        dead_code,
        reason = "owns the grid and palette memory the bind group reads"
    )]
    retained: Vec<Texture>,
}

/// Every object sharing one mesh level and one material.
#[derive(Debug)]
struct Batch {
    mesh: usize,
    level: usize,
    material: usize,
    instances: Buffer,
    instance_count: u32,
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
    frame_buffer: Buffer,
    frame_group: BindGroup,
    materials: Vec<BindGroup>,
    meshes: Vec<GpuMesh>,
    batches: [Vec<Batch>; 4],
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
        if let Some(effects) = scene.effects() {
            crate::frame::validate_stock_effects(effects)?;
        }
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
        let batches = build_batches(device, queue, scene);
        let terrain = upload_terrain(device, queue, scene, pipelines.terrain_layout(), &sampler)?;
        let frame_buffer = upload_buffer(
            device,
            queue,
            "frame uniform",
            BufferUsages::UNIFORM,
            &[0u8; FRAME_UNIFORM_BYTES],
        );
        let frame_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("frame"),
            layout: pipelines.frame_layout(),
            entries: &[BindGroupEntry {
                binding: 0,
                resource: frame_buffer.as_entire_binding(),
            }],
        });
        Ok(Self {
            frame_buffer,
            frame_group,
            materials,
            meshes,
            batches,
            terrain,
            retained: Retained {
                textures,
                placeholder,
                sampler,
                material_uniforms,
            },
        })
    }

    /// Writes the per-frame uniform the next draw will read.
    pub fn set_frame(&self, queue: &Queue, frame: FrameUniform) {
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
        self.draw_objects(pass, &self.frame_group, pipelines.object_pipelines());
        if let Some(terrain) = &self.terrain {
            pass.set_pipeline(pipelines.terrain());
            pass.set_bind_group(TERRAIN_GROUP, &terrain.bind_group, &[]);
            pass.set_vertex_buffer(0, terrain.lod.vertices.slice(..));
            pass.set_vertex_buffer(1, terrain.instances.slice(..));
            pass.set_index_buffer(terrain.lod.indices.slice(..), IndexFormat::Uint32);
            pass.draw_indexed(0..terrain.lod.index_count, 0, 0..1);
        }
    }

    pub(crate) fn draw_objects(
        &self,
        pass: &mut RenderPass<'_>,
        frame_group: &BindGroup,
        pipelines: &[wgpu::RenderPipeline; 4],
    ) {
        pass.set_bind_group(FRAME_GROUP, frame_group, &[]);
        for key in PipelineKey::ALL {
            let batches = &self.batches[key.index()];
            if batches.is_empty() {
                continue;
            }
            pass.set_pipeline(&pipelines[key.index()]);
            for batch in batches {
                let lod = &self.meshes[batch.mesh].lods[batch.level];
                pass.set_bind_group(MATERIAL_GROUP, &self.materials[batch.material], &[]);
                pass.set_vertex_buffer(0, lod.vertices.slice(..));
                pass.set_vertex_buffer(1, batch.instances.slice(..));
                pass.set_index_buffer(lod.indices.slice(..), IndexFormat::Uint32);
                pass.draw_indexed(0..lod.index_count, 0, 0..batch.instance_count);
            }
        }
    }

    /// Returns how many instanced draws each pipeline variant will issue.
    #[must_use]
    pub fn batch_counts(&self) -> [usize; 4] {
        array::from_fn(|index| self.batches[index].len())
    }
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

/// Collects objects sharing a mesh and material into instanced draws.
fn build_batches(device: &Device, queue: &Queue, scene: &RenderScene) -> [Vec<Batch>; 4] {
    let meshes = scene.geometry().meshes();
    let mut grouped: BTreeMap<(u32, u32), (Vec<u8>, u32)> = BTreeMap::new();
    for object in scene.geometry().objects() {
        // The billboard policy belongs to the level being drawn, not to the
        // object, so it is read from the same LOD the draw will reach for. A
        // level 0 authored as a card therefore turns to face the camera today,
        // without waiting for distance-based selection to exist.
        let mesh =
            usize::try_from(object.mesh().raw()).expect("a validated handle indexes its table");
        let billboard = meshes[mesh].lods()[NEAREST_LEVEL].billboard();
        let entry = grouped
            .entry((object.mesh().raw(), object.material().raw()))
            .or_default();
        entry
            .0
            .extend_from_slice(&pack_instance(object.transform(), billboard));
        entry.1 += 1;
    }
    let materials = scene.resources().materials();
    let mut batches: [Vec<Batch>; 4] = array::from_fn(|_| Vec::new());
    for ((mesh, material), (bytes, instance_count)) in grouped {
        let mesh = usize::try_from(mesh).expect("a validated handle indexes its table");
        let material = usize::try_from(material).expect("a validated handle indexes its table");
        let key = PipelineKey::from_material(&materials[material]);
        batches[key.index()].push(Batch {
            mesh,
            level: NEAREST_LEVEL,
            material,
            instances: upload_buffer(
                device,
                queue,
                "placement instances",
                BufferUsages::VERTEX,
                &bytes,
            ),
            instance_count,
        });
    }
    batches
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
            &pack_instance(&TERRAIN_PLACEMENT, None),
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
