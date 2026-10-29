use super::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer,
    BufferUsages, Device, DrawError, FRAME_UNIFORM_BYTES, GpuLod, GpuMesh, GpuTerrain, Queue,
    RenderScene, Sampler, TERRAIN_PLACEMENT, TerrainUniformState, Texture, TextureHandle,
    TextureView, TextureViewDescriptor, pack_instance, upload_buffer, upload_palette_array,
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
    queue: &Queue,
    prepared: &[crate::texture::OwnedPreparedTexture],
) -> Result<(Vec<Texture>, Vec<TextureView>), DrawError> {
    let mut textures = Vec::new();
    let mut views = Vec::new();
    for texture in prepared {
        let uploaded = upload_texture(device, queue, &texture.as_prepared())?;
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
    pub(super) sampler: &'a Sampler,
    pub(super) heights: &'a Buffer,
}

/// Builds one bind group per material, filling absent maps with the placeholder.
pub(super) fn upload_materials(
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
        heights,
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
        let uniform = upload_buffer(
            device,
            queue,
            "material",
            BufferUsages::UNIFORM,
            &super::fade::uniform(scene, material),
        );
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
        groups.push(crate::upload::extended_material_group(
            device,
            layout,
            &uniform,
            crate::upload::MaterialTextureBindings { maps: bound, extra },
            sampler,
            heights,
        ));
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
    layout: &BindGroupLayout,
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
    let layered = prepared
        .layered
        .as_ref()
        .map(|source| source.resident(device, queue))
        .transpose()?;
    let bind_group = terrain_bind_group(
        device,
        layout,
        sampler,
        (&uniform_buffer, &grid, &palettes, layered.as_deref()),
        None,
    );
    let mut retained = palettes;
    retained.push(grid);
    let reused = previous
        .and_then(|gpu| gpu.terrain.as_ref())
        .and_then(|gpu| gpu.streaming.as_ref())
        .and_then(|old| super::streaming::Streaming::replacement(scene.clone(), old));
    let (lod, streaming) = if let Some(streaming) = reused {
        (None, Some(streaming))
    } else {
        let lod = terrain_lod(device, queue, mesh);
        let streaming = terrain
            .streaming_policy()
            .map(|_| super::streaming::Streaming::new(scene.clone(), lod.clone(), authority));
        (streaming.is_none().then_some(lod), streaming)
    };
    Ok(Some(GpuTerrain {
        page_bounds: page_bounds(terrain),
        page_elevation: prepared.elevation,
        uniform: uniform_buffer,
        sampler: sampler.clone(),
        pages: None,
        bounds: prepared.bounds,
        lod,
        streaming,
        instances: terrain_instances(device, queue),
        bind_group,
        retained,
        layered: prepared.layered.clone(),
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
pub(super) fn terrain_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    sampler: &Sampler,
    resources: (
        &Buffer,
        &Texture,
        &[Texture],
        Option<&crate::terrain::pages::layered_gpu::ResidentSource>,
    ),
    pages: Option<(&TextureView, &Buffer)>,
) -> BindGroup {
    let (uniform_buffer, grid, palettes, layered) = resources;
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
    if pages.is_some() {
        entries.retain(|entry| entry.binding != 1);
    }
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
    if let Some((view, table)) = pages {
        entries.push(BindGroupEntry {
            binding: 6,
            resource: BindingResource::TextureView(view),
        });
        entries.push(BindGroupEntry {
            binding: 7,
            resource: table.as_entire_binding(),
        });
    }
    if let Some(source) = layered {
        entries.retain(|entry| !matches!(entry.binding, 2..=4));
        entries.extend(layered_entries(source));
    }
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

pub(super) fn prepared_terrain(
    device: &Device,
    authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
    prepared: &super::PreparedScene,
    pipelines: &super::ScenePipelines,
    sampler: &Sampler,
    previous: Option<&super::SceneGpu>,
) -> Result<Option<GpuTerrain>, DrawError> {
    upload_terrain(
        device,
        authority,
        &prepared.scene,
        if prepared
            .terrain
            .as_ref()
            .is_some_and(|terrain| terrain.layered.is_some())
        {
            &pipelines
                .layered
                .as_ref()
                .ok_or(DrawError::LayeredLimit)?
                .layout
        } else {
            pipelines.terrain_layout()
        },
        sampler,
        (prepared.terrain.as_ref(), previous),
    )
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

fn terrain_instances(device: &Device, queue: &Queue) -> Buffer {
    upload_buffer(
        device,
        queue,
        "terrain instance",
        BufferUsages::VERTEX,
        &pack_instance(&TERRAIN_PLACEMENT),
    )
}

fn layered_entries(
    source: &crate::terrain::pages::layered_gpu::ResidentSource,
) -> Vec<BindGroupEntry<'_>> {
    let mut entries: Vec<_> = source
        .buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| BindGroupEntry {
            binding: 8 + u32::try_from(index).expect("three inputs"),
            resource: buffer.as_entire_binding(),
        })
        .collect();
    entries.extend(
        source
            .views
            .iter()
            .enumerate()
            .map(|(index, view)| BindGroupEntry {
                binding: 11 + u32::try_from(index).expect("texture slot"),
                resource: BindingResource::TextureView(view),
            }),
    );
    entries.extend(
        source
            .samplers
            .iter()
            .enumerate()
            .map(|(index, sampler)| BindGroupEntry {
                binding: 21 + u32::try_from(index).expect("sampler slot"),
                resource: BindingResource::Sampler(sampler),
            }),
    );
    entries
}

pub(super) fn scene_materials(
    device: &Device,
    queue: &Queue,
    scene: &RenderScene,
    pipelines: &crate::renderer::ScenePipelines,
    textures: (&[TextureView], &TextureView),
    shared: (&Sampler, &Buffer),
) -> (Vec<BindGroup>, Vec<Buffer>) {
    upload_materials(
        device,
        queue,
        scene,
        &MaterialBindings {
            layout: pipelines.material_layout(),
            views: textures.0,
            placeholder: textures.1,
            sampler: shared.0,
            heights: shared.1,
        },
    )
}
