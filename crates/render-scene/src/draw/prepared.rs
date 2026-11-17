use super::{
    DrawError, MaterialHandle, MeshSelection, PreparedTexture, RenderScene, TerrainUniformState,
    array, build_terrain_mesh, contact, descriptors, mesh_selections, pack_indices, pack_vertices,
    prepare_texture, resources, visibility,
};

use crate::object::layout::GeometryLayout;

/// Owned CPU upload data. Preparing this value requires no device or GPU access.
#[derive(Debug)]
pub struct PreparedScene {
    pub(super) scene: RenderScene,
    pub(super) textures: Vec<crate::texture::OwnedPreparedTexture>,
    pub(super) meshes: Vec<PreparedMesh>,
    pub(super) selection: Vec<MeshSelection>,
    pub(super) terrain: Option<PreparedTerrain>,
    pub(super) contact: Vec<u8>,
    /// Every pose's palettes, laid out for the one scene palette buffer.
    pub(super) palette: crate::skinning::ScenePalette,
}

#[derive(Debug)]
pub(super) struct PreparedMesh {
    pub lods: Vec<PreparedLod>,
}
#[derive(Debug)]
pub(super) struct PreparedLod {
    /// The vertex format of `vertices`, and of `transition`, which repacks the preceding level.
    pub layout: crate::object::layout::GeometryLayout,
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
    /// Returns the same CPU preparation failures as [`SceneGpu::upload`](super::SceneGpu::upload).
    pub fn new(scene: &RenderScene) -> Result<Self, DrawError> {
        descriptors::validate(scene)?;
        let palette = crate::skinning::ScenePalette::new(scene.geometry().skin_poses()).map_err(
            |overflow| DrawError::SkinPalette {
                entries: overflow.entries,
            },
        )?;
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
            palette,
        })
    }

    pub(super) fn validate_device(
        &self,
        device: &wgpu::Device,
        pipelines: &super::ScenePipelines,
    ) -> Result<(), DrawError> {
        super::validate_profile(&self.scene, pipelines)?;
        super::source_environment::allocations(&self.scene)?
            .validate(&device.limits())
            .map_err(|_| crate::sky::preparation::PreparationError::Budget)?;
        if let Some(policy) = self
            .scene
            .terrain()
            .and_then(worldrunner_render_plan::SceneTerrain::page_policy)
        {
            crate::terrain::pages::residency::validate_layers(&device.limits(), policy.slots(), 3)?;
        }
        crate::skinning::admission::validate(&self.palette, &device.limits())?;
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
            source
                .allocations()
                .validate_array_layers::<worldrunner_render_gpu::configuration::DriverAllocationError>(
                    &device.limits(),
                )?;
            source.validate_device(device)?;
            if !pipelines.layered_terrain_supported() {
                return Err(DrawError::LayeredLimit);
            }
        }
        Ok(())
    }

    /// The immutable source generation associated with these prepared bytes.
    pub fn scene(&self) -> &RenderScene {
        &self.scene
    }

    /// Backing bytes of prepared scene textures selected by `include`, before upload.
    #[must_use]
    pub fn texture_bytes_where(
        &self,
        include: impl Fn(worldrunner_render_plan::TextureHandle) -> bool,
    ) -> u64 {
        self.textures
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                u32::try_from(*index)
                    .is_ok_and(|raw| include(worldrunner_render_plan::TextureHandle::from_raw(raw)))
            })
            .map(|(_, texture)| texture.as_prepared().tier_bytes(0))
            .sum()
    }

    /// Chooses one texture tier for the textures `eligible` accepts and drops that many top
    /// levels from each before upload (GitHub #376 step 3). `other_resident` is the memory
    /// already counted outside these textures. Without a policy or a device-local size this
    /// changes nothing and reports the authored bytes.
    pub fn apply_texture_tier(
        &mut self,
        policy: Option<crate::texture::TextureTierPolicy>,
        device_local: Option<u64>,
        other_resident: u64,
        eligible: impl Fn(worldrunner_render_plan::TextureHandle) -> bool,
    ) -> crate::texture::TierChoice {
        let selected: Vec<usize> = (0..self.textures.len())
            .filter(|&index| {
                u32::try_from(index).is_ok_and(|raw| {
                    eligible(worldrunner_render_plan::TextureHandle::from_raw(raw))
                })
            })
            .collect();
        let choice = {
            let borrowed: Vec<_> = selected
                .iter()
                .map(|&index| self.textures[index].as_prepared())
                .collect();
            crate::texture::choose_tier(policy, device_local, other_resident, &borrowed)
        };
        for index in selected {
            self.textures[index].drop_top_levels(choice.dropped_levels);
        }
        choice
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
        .map(|(mesh, source)| prepare_mesh(mesh, source))
        .collect()
}

/// Packs the levels of mesh `mesh` of the scene's mesh table.
pub(super) fn prepare_mesh(
    mesh: usize,
    source: &worldrunner_render_plan::SceneMesh,
) -> Result<PreparedMesh, DrawError> {
    let lods = source
        .lods()
        .iter()
        .enumerate()
        .map(|(level, lod)| prepare_lod(mesh, source, level, lod))
        .collect::<Result<Vec<_>, DrawError>>()?;
    Ok(PreparedMesh { lods })
}

/// Packs one level: its vertices, indices and the card complement of the level before it.
fn prepare_lod(
    mesh: usize,
    source: &worldrunner_render_plan::SceneMesh,
    level: usize,
    lod: &worldrunner_render_plan::SceneMeshLod,
) -> Result<PreparedLod, DrawError> {
    let error = |source| DrawError::Geometry {
        mesh,
        level,
        source,
    };
    let transition = lod
        .card()
        .filter(|_| level > 0)
        .map(|card| {
            let preceding = &source.lods()[level - 1];
            let bytes = crate::geometry::pack_transition(preceding, card.coverage_micro())?;
            // The complement draws the preceding level, so it carries that level's influences.
            Ok(match preceding.skin_influences() {
                Some(influences) => crate::skinning::append_skin_words(&bytes, influences),
                None => bytes,
            })
        })
        .transpose()
        .map_err(error)?;
    let skinned = crate::skinning::pack_skinned_vertices(lod).map_err(error)?;
    let layout = GeometryLayout::of(skinned.is_some());
    let vertices = match skinned {
        Some(bytes) => bytes,
        None => pack_vertices(lod).map_err(error)?,
    };
    Ok(PreparedLod {
        layout,
        vertices,
        indices: pack_indices(lod),
        transition,
        index_count: u32::try_from(lod.indices().len()).expect("validated index count"),
    })
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
