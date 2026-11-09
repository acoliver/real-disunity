use super::{
    BindGroup, BindGroupLayout, Buffer, BufferUsages, Device, DrawError, FRAME_UNIFORM_BYTES,
    GpuLod, GpuMesh, GpuTerrain, Queue, RenderScene, Sampler, TerrainUniformState, Texture,
    TextureHandle, TextureView, TextureViewDescriptor, upload_buffer, upload_palette_array,
    upload_texture, upload_type_grid,
};

pub(super) fn allocate_frame_slots(
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

pub(super) fn frame_group(device: &Device, layout: &BindGroupLayout, buffer: &Buffer) -> BindGroup {
    super::decals::empty_group(device, layout, buffer)
}

/// Resolves one validated handle to its table index.
///
/// `RenderScene::validate` has already proven every handle addresses its
/// table, so this cannot fail. It is a checked conversion regardless, because
/// an `as` cast would silently yield a wrong index on a target where `usize`
/// is narrower than the handle, and a wrong index samples the wrong texture.
pub(super) fn table_index(handle: TextureHandle) -> usize {
    usize::try_from(handle.raw()).expect("a validated handle indexes its table")
}

/// Uploads every scene texture and creates a view of each.
pub(super) fn upload_textures(
    device: &Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    prepared: &[crate::texture::OwnedPreparedTexture],
) -> Result<(Vec<Texture>, Vec<TextureView>), DrawError> {
    let mut textures = Vec::new();
    let mut views = Vec::new();
    for texture in prepared {
        let prepared = texture.as_prepared();
        let mut uploaded = upload_texture(device, authority.queue(), &prepared)?;
        if let Some(request) = prepared
            .cube()
            .and_then(worldrunner_render_plan::SceneSpecularCube::preparation)
        {
            uploaded = super::source_environment::filter(device, authority, &uploaded, request)?;
        }
        views.push(uploaded.create_view(&TextureViewDescriptor::default()));
        textures.push(uploaded);
    }
    Ok((textures, views))
}

/// What every material bind group shares, as opposed to its own maps.
#[derive(Clone, Copy)]
pub(super) struct MaterialBindings<'a> {
    pub(super) layout: &'a BindGroupLayout,
    pub(super) views: &'a [TextureView],
    pub(super) placeholder: &'a TextureView,
    pub(super) heights: &'a Buffer,
}

/// What one material bind group reads besides the material and its own uniform.
pub(super) struct GroupInputs<'a> {
    pub(super) layout: &'a BindGroupLayout,
    pub(super) placeholder: &'a TextureView,
    pub(super) heights: &'a Buffer,
    /// The view of one scene texture. Views are reference counted, so a lookup is cheap.
    pub(super) view: &'a dyn Fn(TextureHandle) -> TextureView,
}

/// Binds one material's maps, its uniform and the shared contact heights, filling absent maps
/// with the placeholder.
pub(super) fn material_group(
    device: &Device,
    inputs: &GroupInputs<'_>,
    material: &worldrunner_render_plan::SceneMaterial,
    uniform: &Buffer,
    sampler: &Sampler,
) -> BindGroup {
    let optional = |handle: Option<TextureHandle>| {
        handle.map_or_else(
            || inputs.placeholder.clone(),
            |handle| (inputs.view)(handle),
        )
    };
    let bound = [
        (inputs.view)(material.albedo()),
        optional(material.normal()),
        optional(material.roughness_metallic()),
        optional(material.ambient_occlusion()),
        optional(material.self_illumination()),
    ];
    let atlas = material.impostor();
    let extra = [
        optional(atlas.and_then(|a| a.relief().map(|v| v.0))),
        optional(atlas.and_then(worldrunner_render_plan::SceneImpostor::borders)),
        optional(
            material
                .features()
                .and_then(|f| f.wind)
                .and_then(worldrunner_render_plan::SceneWind::field)
                .map(worldrunner_render_plan::ScenePlanarWindField::texture),
        ),
    ];
    crate::upload::extended_material_group(
        device,
        inputs.layout,
        uniform,
        crate::upload::MaterialTextureBindings {
            maps: [&bound[0], &bound[1], &bound[2], &bound[3], &bound[4]],
            extra: [&extra[0], &extra[1], &extra[2]],
        },
        sampler,
        inputs.heights,
    )
}

/// Builds one bind group per material, filling absent maps with the placeholder.
pub(super) fn upload_materials(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
    bindings: &MaterialBindings<'_>,
) -> (Vec<BindGroup>, Vec<Buffer>) {
    let views = bindings.views;
    let lookup = |handle: TextureHandle| views[table_index(handle)].clone();
    let inputs = GroupInputs {
        layout: bindings.layout,
        placeholder: bindings.placeholder,
        heights: bindings.heights,
        view: &lookup,
    };
    let mut samplers = crate::sampling::MaterialSamplers::default();
    let mut groups = Vec::new();
    let mut uniforms = Vec::new();
    for material in scene.resources().materials() {
        let uniform = upload_buffer(
            device,
            queue,
            "material",
            BufferUsages::UNIFORM,
            &super::fade::uniform(scene, material),
        );
        let sampler = samplers.get(device, material);
        groups.push(material_group(device, &inputs, material, &uniform, sampler));
        uniforms.push(uniform);
    }
    (groups, uniforms)
}

/// Uploads already packed mesh levels without processing source geometry.
pub(super) fn upload_meshes(
    device: &Device,
    queue: &Queue,
    meshes: &[super::prepared::PreparedMesh],
) -> Vec<GpuMesh> {
    meshes
        .iter()
        .map(|mesh| GpuMesh {
            lods: mesh
                .lods
                .iter()
                .map(|lod| GpuLod {
                    transition: lod.transition.as_ref().map(|bytes| {
                        upload_buffer(
                            device,
                            queue,
                            "mesh card complement",
                            BufferUsages::VERTEX,
                            bytes,
                        )
                    }),
                    vertices: upload_buffer(
                        device,
                        queue,
                        "mesh vertices",
                        BufferUsages::VERTEX,
                        &lod.vertices,
                    ),
                    indices: upload_buffer(
                        device,
                        queue,
                        "mesh indices",
                        BufferUsages::INDEX,
                        &lod.indices,
                    ),
                    index_count: lod.index_count,
                })
                .collect(),
        })
        .collect()
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
pub(super) fn upload_terrain(
    device: &Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    scene: &RenderScene,
    sampler: &Sampler,
    sources: (
        Option<&super::prepared::PreparedTerrain>,
        Option<&super::SceneGpu>,
    ),
) -> Result<Option<GpuTerrain>, DrawError> {
    let (prepared, previous) = sources;
    let queue = authority.queue();
    let Some(terrain) = scene.terrain() else {
        return Ok(None);
    };
    let prepared = prepared.expect("prepared terrain matches scene");
    let mesh = &prepared.mesh;
    let uniform = &prepared.uniform;

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
    let palettes = upload_palettes(device, queue, &prepared.palettes)?;

    // The uniform buffer is built here rather than inside the bind group so
    // that helper takes only the resources it wires together, and so the
    // buffer's lifetime is visibly the caller's.
    let uniform_buffer = terrain_uniform_buffer(device, queue, uniform);
    let source_planes = crate::terrain::planes::HeightfieldPlanes::new(terrain);
    let instances = terrain_instances(device, queue, &source_planes.chunks)?;
    let planes = crate::terrain::planes::upload(device, queue, &source_planes.bytes)?;
    let layered = prepared
        .layered
        .as_ref()
        .map(|source| source.resident(device, queue))
        .transpose()?;
    let resources = std::sync::Arc::new(crate::terrain::prepared::Resources {
        uniform: uniform_buffer.clone(),
        planes: planes.clone(),
        sampler: sampler.clone(),
        grid_view: grid.create_view(&TextureViewDescriptor::default()),
        grid: grid.clone(),
        // The dimension is stated for the same reason as in the frozen bind group: a palette
        // of one surface would otherwise get a plain 2D view.
        palettes: palettes
            .iter()
            .map(|texture| {
                let view = texture.create_view(&TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..TextureViewDescriptor::default()
                });
                (texture.clone(), view)
            })
            .collect(),
        layered: layered.clone(),
    });
    let (lod, streaming) = terrain_geometry(device, authority, scene, mesh, previous);
    Ok(Some(GpuTerrain {
        resources,
        prepared: crate::terrain::prepared::PreparedTerrain::default(),
        authority: authority.clone(),
        page_bounds: page_bounds(terrain),
        page_elevation: prepared.elevation,
        pages: None,
        bounds: prepared.bounds,
        lod,
        streaming,
        instances,
        layered: prepared.layered.clone(),
    }))
}
fn terrain_geometry(
    device: &Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    scene: &RenderScene,
    mesh: &crate::terrain::TerrainMesh,
    previous: Option<&super::SceneGpu>,
) -> (Option<GpuLod>, Option<super::streaming::Streaming>) {
    let terrain = scene.terrain().expect("terrain");
    let queue = authority.queue();
    let reused = previous
        .and_then(|gpu| gpu.terrain.as_ref())
        .and_then(|gpu| gpu.streaming.as_ref())
        .and_then(|old| super::streaming::Streaming::replacement(scene.clone(), old));
    if let Some(streaming) = reused {
        (None, Some(streaming))
    } else {
        let lod = terrain_lod(device, queue, mesh);
        let streaming = terrain
            .streaming_policy()
            .map(|_| super::streaming::Streaming::new(scene.clone(), lod.clone(), authority));
        (streaming.is_none().then_some(lod), streaming)
    }
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
pub(super) fn upload_palettes(
    device: &Device,
    queue: &Queue,
    prepared: &[Vec<crate::texture::OwnedPreparedTexture>; 3],
) -> Result<Vec<Texture>, DrawError> {
    let labels = [
        "terrain-palette-albedo",
        "terrain-palette-normal",
        "terrain-palette-rough-metal",
    ];
    prepared
        .iter()
        .zip(labels)
        .map(|(layers, label)| {
            upload_palette_array(
                device,
                queue,
                label,
                &layers
                    .iter()
                    .map(crate::texture::OwnedPreparedTexture::as_prepared)
                    .collect::<Vec<_>>(),
            )
            .map_err(DrawError::from)
        })
        .collect()
}

fn page_bounds(terrain: &worldrunner_render_plan::SceneTerrain) -> [f32; 4] {
    use crate::geometry::millimeter_to_meter;
    let spacing = i64::from(terrain.sample_spacing_millimeters());
    [
        millimeter_to_meter(terrain.origin_east_millimeters()),
        millimeter_to_meter(terrain.origin_north_millimeters()),
        millimeter_to_meter(i64::from(terrain.columns() - 1) * spacing),
        millimeter_to_meter(i64::from(terrain.rows() - 1) * spacing),
    ]
}

fn terrain_uniform_buffer(device: &Device, queue: &Queue, uniform: &TerrainUniformState) -> Buffer {
    upload_buffer(
        device,
        queue,
        "terrain uniform",
        BufferUsages::UNIFORM,
        uniform.bytes(),
    )
}

pub(super) fn page_elevation(terrain: &worldrunner_render_plan::SceneTerrain) -> [f32; 2] {
    let heights = terrain.heights_millimeters();
    [
        *heights.iter().min().expect("terrain heights"),
        *heights.iter().max().expect("terrain heights"),
    ]
    .map(|height| crate::geometry::millimeter_to_meter(i64::from(height)))
}

pub(super) fn post_settings(
    scene: &RenderScene,
) -> Result<worldrunner_render_post::HdrPostSettings, crate::frame::FrameError> {
    scene
        .effects()
        .map(crate::post::settings)
        .transpose()
        .map_err(|_| crate::frame::FrameError::UnsupportedEffect("post settings"))
        .map(Option::unwrap_or_default)
}

fn terrain_lod(device: &Device, queue: &Queue, mesh: &crate::terrain::TerrainMesh) -> GpuLod {
    GpuLod {
        transition: None,
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
    }
}

fn terrain_instances(
    device: &Device,
    queue: &Queue,
    chunks: &std::collections::BTreeMap<crate::terrain::streaming::TerrainChunk, u32>,
) -> Result<Buffer, DrawError> {
    let size = crate::terrain::planes::instance_size(chunks.len(), &device.limits())?;
    let mut bytes = vec![0; size];
    for index in chunks.values() {
        let offset =
            usize::try_from(*index).expect("chunk index") * crate::geometry::INSTANCE_BYTES;
        bytes[offset..offset + crate::geometry::INSTANCE_BYTES]
            .copy_from_slice(&crate::terrain::planes::instance_bytes(*index));
    }
    Ok(upload_buffer(
        device,
        queue,
        "terrain instance",
        BufferUsages::VERTEX,
        &bytes,
    ))
}

pub(super) fn scene_materials(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
    pipelines: &crate::renderer::ScenePipelines,
    textures: (&[TextureView], &TextureView),
    heights: &Buffer,
) -> (Vec<BindGroup>, Vec<Buffer>) {
    upload_materials(
        device,
        queue,
        scene,
        &MaterialBindings {
            layout: pipelines.material_layout(),
            views: textures.0,
            placeholder: textures.1,
            heights,
        },
    )
}

impl super::SceneGpu {
    pub(super) fn finish_upload(
        mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        scene: &worldrunner_render_plan::RenderScene,
    ) -> Result<Self, super::DrawError> {
        self.install_terrain_page_policy(device, authority, scene)?;
        self.flush_instances(device, authority)?;
        Ok(self)
    }
}
