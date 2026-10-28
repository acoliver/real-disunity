use super::{
    DrawError, MaterialHandle, MeshSelection, PreparedTexture, RenderScene, TerrainUniformState,
    array, build_terrain_mesh, contact, descriptors, mesh_selections, pack_indices, pack_vertices,
    prepare_texture, resources, visibility,
};

/// Owned CPU upload data. Preparing this value requires no device or GPU access.
#[derive(Debug)]
pub struct PreparedScene {
    pub(super) scene: RenderScene,
    pub(super) textures: Vec<crate::texture::OwnedPreparedTexture>,
    pub(super) meshes: Vec<PreparedMesh>,
    pub(super) selection: Vec<MeshSelection>,
    pub(super) terrain: Option<PreparedTerrain>,
    pub(super) contact: Vec<u8>,
}

#[derive(Debug)]
pub(super) struct PreparedMesh {
    pub lods: Vec<PreparedLod>,
}
#[derive(Debug)]
pub(super) struct PreparedLod {
    pub vertices: Vec<u8>,
    pub indices: Vec<u8>,
    pub transition: Option<Vec<u8>>,
    pub index_count: u32,
}
#[derive(Debug)]
pub(super) struct PreparedTerrain {
    pub mesh: crate::terrain::TerrainMesh,
    pub uniform: TerrainUniformState,
    pub palettes: [Vec<crate::texture::OwnedPreparedTexture>; 3],
    pub bounds: visibility::Bounds,
    pub elevation: [f32; 2],
    pub layered: Option<std::sync::Arc<crate::terrain::pages::layered_gpu::LayeredGpuSource>>,
}

impl PreparedScene {
    /// Prepares textures, mesh levels, transitions and terrain for the shared upload path.
    ///
    /// # Errors
    /// Returns the same CPU preparation failures as [`SceneGpu::upload`].
    pub fn new(scene: &RenderScene) -> Result<Self, DrawError> {
        descriptors::validate(scene)?;
        crate::water::surface::validate(scene)?;
        let textures = scene
            .resources()
            .textures()
            .iter()
            .enumerate()
            .map(|(index, texture)| {
                prepare_texture(texture)
                    .map(PreparedTexture::into_owned)
                    .map_err(|source| DrawError::Texture { index, source })
            })
            .collect::<Result<Vec<_>, _>>()?;
        descriptors::validate_atlases(scene, &textures)?;
        super::decals::validate_decal_materials(scene)?;
        let meshes = prepare_meshes(scene)?;
        let terrain = scene
            .terrain()
            .map(|terrain| {
                let mesh = if let Some(policy) = terrain.streaming_policy() {
                    crate::terrain::chunks::build(
                        terrain,
                        crate::terrain::streaming::TerrainChunk::root(terrain, policy),
                    )
                } else {
                    build_terrain_mesh(terrain)
                }
                .map_err(|source| DrawError::Terrain { source })?;
                let uniform = TerrainUniformState::encode(terrain)
                    .map_err(|source| DrawError::TerrainUniform { source })?;
                let palettes = prepare_palettes(scene, uniform.layer_materials())?;
                Ok::<_, DrawError>(PreparedTerrain {
                    bounds: terrain_bounds(terrain, &mesh.vertices),
                    elevation: resources::page_elevation(terrain),
                    mesh,
                    uniform,
                    palettes,
                    layered: terrain
                        .layered_source()
                        .map(|_| {
                            crate::terrain::pages::layered_gpu::LayeredGpuSource::new(
                                terrain,
                                scene.resources(),
                            )
                            .map(std::sync::Arc::new)
                        })
                        .transpose()?,
                })
            })
            .transpose()?;
        Ok(Self {
            scene: scene.clone(),
            textures,
            meshes,
            terrain,
            selection: mesh_selections(scene),
            contact: contact::prepare(scene)?,
        })
    }

    pub(super) fn validate_device(
        &self,
        device: &wgpu::Device,
        pipelines: &super::ScenePipelines,
    ) -> Result<(), DrawError> {
        // Six material maps, one shadow array and three screen-space inputs.
        if device.limits().max_sampled_textures_per_shader_stage < 10 {
            return Err(crate::frame::FrameError::UnsupportedEffect(
                "material texture-stage budget",
            )
            .into());
        }
        if let Some(source) = self
            .terrain
            .as_ref()
            .and_then(|terrain| terrain.layered.as_ref())
        {
            source.validate_device(device)?;
            if pipelines.layered.is_none() {
                return Err(DrawError::LayeredLimit);
            }
        }
        Ok(())
    }

    /// The immutable source generation associated with these prepared bytes.
    pub fn scene(&self) -> &RenderScene {
        &self.scene
    }
}

fn prepare_palettes(
    scene: &RenderScene,
    layers: &[MaterialHandle],
) -> Result<[Vec<crate::texture::OwnedPreparedTexture>; 3], DrawError> {
    if scene
        .terrain()
        .is_some_and(|terrain| terrain.layered_source().is_some())
    {
        let index = resources::table_index(
            scene.resources().materials()[layers[0].raw() as usize].albedo(),
        );
        let texture = prepare_texture(&scene.resources().textures()[index])
            .map_err(|source| DrawError::Texture { index, source })?;
        return Ok(array::from_fn(|_| vec![texture.clone().into_owned()]));
    }
    let mut prepared = array::from_fn(|_| Vec::new());
    for (layer, handle) in layers.iter().enumerate() {
        let material = &scene.resources().materials()[handle.raw() as usize];
        for (slot, (handle, map)) in [
            (Some(material.albedo()), "albedo"),
            (material.normal(), "normal"),
            (material.roughness_metallic(), "roughness-metallic"),
        ]
        .into_iter()
        .enumerate()
        {
            let index =
                resources::table_index(handle.ok_or(DrawError::TerrainPaletteMap { layer, map })?);
            prepared[slot].push(
                prepare_texture(&scene.resources().textures()[index])
                    .map(PreparedTexture::into_owned)
                    .map_err(|source| DrawError::Texture { index, source })?,
            );
        }
    }
    Ok(prepared)
}

fn prepare_meshes(scene: &RenderScene) -> Result<Vec<PreparedMesh>, DrawError> {
    scene
        .geometry()
        .meshes()
        .iter()
        .enumerate()
        .map(|(mesh, source)| {
            let lods = source
                .lods()
                .iter()
                .enumerate()
                .map(|(level, lod)| {
                    let error = |source| DrawError::Geometry {
                        mesh,
                        level,
                        source,
                    };
                    let transition = lod
                        .card()
                        .filter(|_| level > 0)
                        .map(|card| {
                            crate::geometry::pack_transition(
                                &source.lods()[level - 1],
                                card.coverage_micro(),
                            )
                        })
                        .transpose()
                        .map_err(error)?;
                    Ok(PreparedLod {
                        vertices: pack_vertices(lod).map_err(error)?,
                        indices: pack_indices(lod),
                        transition,
                        index_count: u32::try_from(lod.indices().len())
                            .expect("validated index count"),
                    })
                })
                .collect::<Result<Vec<_>, DrawError>>()?;
            Ok(PreparedMesh { lods })
        })
        .collect::<Result<Vec<_>, DrawError>>()
}

fn terrain_bounds(
    terrain: &worldrunner_render_plan::SceneTerrain,
    vertices: &[u8],
) -> visibility::Bounds {
    let mut bounds = visibility::terrain_bounds(vertices);
    if terrain.streaming_policy().is_some() {
        bounds.max.z = resources::page_elevation(terrain)[1];
    }
    bounds
}
