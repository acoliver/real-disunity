//! Appending and replacing textures, materials and meshes of a resident scene.
//!
//! The CPU half ([`PreparedAppend`] and the replacement types) needs no device. It prepares only
//! the new bytes. The GPU half uploads only those bytes, builds the imports only for the new or
//! affected materials and extends the per-resource tables, so its cost follows the size of the
//! edit. Anything that depends on state shared by the whole scene (impostor atlases, fade
//! thresholds and references, terrain contact, specular cubes, effects that hold texture or
//! mesh views) is refused with [`DrawError::NeedsRebuild`] before any upload.
use std::collections::BTreeSet;

use wgpu::{BufferUsages, Device, TextureView, TextureViewDescriptor};
use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_plan::{
    MeshHandle, RenderScene, SceneAppend, SceneFadeCoverage, SceneMaterial, SceneMesh,
    SceneTexture, SkeletonHandle, TextureHandle,
};

use super::resources::{GroupInputs, material_imports, table_index, upload_textures};
use super::visibility::MeshSelection;
use super::{DrawError, PreparedScene, SceneGpu, descriptors, prepared};
use crate::instances::skin::{SkinNeed, skin_needs};
use crate::object::prepared::LentMaterial;
use crate::renderer::ScenePipelines;
use crate::texture::{OwnedPreparedTexture, prepare_texture};
use crate::upload::upload_texture;
use worldrunner_render_gpu::resource::upload_buffer;

/// The table sizes of the resident scene an append was prepared against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceBase {
    pub textures: usize,
    pub materials: usize,
    pub meshes: usize,
}

fn needs_rebuild(item: &'static str, index: usize, reason: &'static str) -> DrawError {
    DrawError::NeedsRebuild {
        item,
        index,
        reason,
    }
}

/// Appended items in upload form. Preparing one needs no device or GPU access.
#[derive(Debug)]
pub struct PreparedAppend {
    base: ResourceBase,
    textures: Vec<OwnedPreparedTexture>,
    meshes: Vec<prepared::PreparedMesh>,
    selection: Vec<MeshSelection>,
    /// The skeleton of each skinned level of each appended mesh.
    skeletons: Vec<Vec<SkeletonHandle>>,
    /// What each appended mesh asks of the resident skeleton table.
    needs: Vec<Vec<SkinNeed>>,
    materials: Vec<SceneMaterial>,
}

impl PreparedAppend {
    /// Prepares the textures and meshes of `append` and checks its materials, whose handles
    /// follow the `base` tables.
    ///
    /// # Errors
    ///
    /// Returns [`DrawError::NeedsRebuild`] naming the first item whose features need the whole
    /// resident scene rebuilt, and the preparation errors of a full upload for any other item.
    pub fn new(base: ResourceBase, append: &SceneAppend) -> Result<Self, DrawError> {
        descriptors::validate_materials(&append.materials)?;
        for (offset, material) in append.materials.iter().enumerate() {
            admit_material(base.materials + offset, material)?;
        }
        let mut textures = Vec::with_capacity(append.textures.len());
        for (offset, texture) in append.textures.iter().enumerate() {
            textures.push(prepare_plain_texture(base.textures + offset, texture)?);
        }
        let mut meshes = Vec::with_capacity(append.meshes.len());
        let mut selection = Vec::with_capacity(append.meshes.len());
        for (offset, mesh) in append.meshes.iter().enumerate() {
            meshes.push(prepared::prepare_mesh(base.meshes + offset, mesh)?);
            selection.push(MeshSelection::new(mesh));
        }
        Ok(Self {
            base,
            textures,
            meshes,
            selection,
            skeletons: append
                .meshes
                .iter()
                .map(crate::instances::skin::mesh_skeletons)
                .collect(),
            needs: append.meshes.iter().map(skin_needs).collect(),
            materials: append.materials.clone(),
        })
    }

    /// The device storage this append asks for, for admission against a byte budget before
    /// anything is uploaded.
    #[must_use]
    pub fn allocations(&self) -> AllocationSet {
        let mut set = AllocationSet::default();
        for texture in &self.textures {
            texture_allocation(&mut set, &texture.as_prepared());
        }
        for mesh in &self.meshes {
            mesh_allocations(&mut set, mesh);
        }
        set
    }

    /// The device features the prepared textures need.
    #[must_use]
    pub fn required_features(&self) -> wgpu::Features {
        texture_features(&self.textures)
    }

    /// Whether the append has anything to upload or bind.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty() && self.meshes.is_empty() && self.materials.is_empty()
    }

    fn texture_bytes(&self) -> Vec<u64> {
        self.textures
            .iter()
            .map(|texture| texture.as_prepared().payload_bytes())
            .collect()
    }

    fn mesh_bytes(&self) -> Vec<u64> {
        self.meshes.iter().map(mesh_payload_bytes).collect()
    }
}

impl PreparedScene {
    /// The device storage the whole scene keeps resident: every plain texture and mesh, counted
    /// as an append counts them, and the specular cubes. The presentation budget reserves this
    /// when the scene is built and again for a full rebuild, so appended resources stay counted.
    #[must_use]
    pub fn resident_allocations(&self) -> AllocationSet {
        let mut set = super::source_environment::resident_allocations(&self.scene);
        let sources = self.scene.resources().textures();
        for (source, texture) in sources.iter().zip(&self.textures) {
            if !matches!(source.payload(), SceneTexture::SpecularCube(_)) {
                texture_allocation(&mut set, &texture.as_prepared());
            }
        }
        for mesh in &self.meshes {
            mesh_allocations(&mut set, mesh);
        }
        if !self.palette.is_empty() {
            set.buffer("scene skin palette", self.palette.bytes().len() as u64, 1);
        }
        set
    }
}

fn texture_features(textures: &[OwnedPreparedTexture]) -> wgpu::Features {
    textures
        .iter()
        .fold(wgpu::Features::empty(), |features, texture| {
            features | texture.as_prepared().format().required_features()
        })
}

fn texture_allocation(set: &mut AllocationSet, texture: &crate::texture::PreparedTexture<'_>) {
    let mips = u32::try_from(texture.mips().len()).unwrap_or(u32::MAX);
    set.texture(
        "scene material texture",
        [texture.width(), texture.height(), 1],
        texture.format(),
        [mips, 1],
        1,
    );
}

fn mesh_allocations(set: &mut AllocationSet, mesh: &prepared::PreparedMesh) {
    for lod in &mesh.lods {
        let buffers = [
            ("scene mesh vertices", Some(&lod.vertices)),
            ("scene mesh indices", Some(&lod.indices)),
            ("scene mesh card complement", lod.transition.as_ref()),
        ];
        for (name, bytes) in buffers {
            if let Some(bytes) = bytes.filter(|bytes| !bytes.is_empty()) {
                set.buffer(name, (bytes.len() as u64).next_multiple_of(4), 1);
            }
        }
    }
}

fn mesh_payload_bytes(mesh: &prepared::PreparedMesh) -> u64 {
    mesh.lods
        .iter()
        .map(|lod| {
            (lod.vertices.len() + lod.indices.len() + lod.transition.as_ref().map_or(0, Vec::len))
                as u64
        })
        .sum()
}

fn prepare_plain_texture(
    index: usize,
    texture: &SceneTexture,
) -> Result<OwnedPreparedTexture, DrawError> {
    if matches!(texture.payload(), SceneTexture::SpecularCube(_)) {
        return Err(needs_rebuild(
            "texture",
            index,
            "specular cubes are filtered and reserved with the whole scene",
        ));
    }
    prepare_texture(texture)
        .map(crate::texture::PreparedTexture::into_owned)
        .map_err(|source| DrawError::Texture { index, source })
}

/// Refuses materials whose bind groups or uniforms depend on state shared by all materials.
fn admit_material(index: usize, material: &SceneMaterial) -> Result<(), DrawError> {
    let reason = if material.impostor().is_some() {
        Some("impostor atlases are validated and bound with the whole scene")
    } else if !matches!(material.fade_coverage(), SceneFadeCoverage::Hashed)
        || material.fade_reference().is_some()
    {
        Some("fade thresholds and references share one storage buffer")
    } else if material.terrain_contact().is_some() {
        Some("terrain contact reads the scene's height buffer")
    } else if material
        .features()
        .and_then(|features| features.wind)
        .and_then(worldrunner_render_plan::SceneWind::field)
        .is_some()
    {
        Some("wind fields are validated and bound with the whole scene")
    } else {
        None
    };
    reason.map_or(Ok(()), |reason| {
        Err(needs_rebuild("material", index, reason))
    })
}

/// Refuses effects that keep views of textures or meshes the replacement would leave stale.
fn require_plain_effects(
    scene: &RenderScene,
    item: &'static str,
    index: usize,
) -> Result<(), DrawError> {
    let Some(effects) = scene.effects() else {
        return Ok(());
    };
    if effects.atmosphere().is_some()
        || effects.environment().is_some()
        || effects.water_settings().is_some()
        || !effects.water_bodies().is_empty()
    {
        return Err(needs_rebuild(
            item,
            index,
            "sky, environment and water effects keep their own views of resources",
        ));
    }
    Ok(())
}

/// A texture swapped in place, with the materials whose bind groups must be rebuilt.
#[derive(Debug)]
pub struct PreparedTextureReplacement {
    index: usize,
    texture: OwnedPreparedTexture,
    materials: Vec<(usize, SceneMaterial)>,
}

impl PreparedTextureReplacement {
    /// Prepares `texture` as the new contents of `handle` in `scene`, which still holds the old
    /// contents.
    ///
    /// # Errors
    ///
    /// Returns [`DrawError::NeedsRebuild`] when the old or new texture is a specular cube, or
    /// when something other than the plain maps of ordinary object materials uses the texture:
    /// terrain, decals, impostors, fade thresholds, wind fields or sky effects.
    pub fn new(
        scene: &RenderScene,
        handle: TextureHandle,
        texture: &SceneTexture,
    ) -> Result<Self, DrawError> {
        let index = table_index(handle);
        let old = scene.resources().textures().get(index);
        if matches!(
            old.map(SceneTexture::payload),
            Some(SceneTexture::SpecularCube(_))
        ) {
            return Err(needs_rebuild(
                "texture",
                index,
                "the old texture is a specular cube",
            ));
        }
        require_plain_effects(scene, "texture", index)?;
        let uses = material_uses(scene, handle)?;
        let copied = copied_materials(scene)?;
        if let Some((material, _)) = uses
            .iter()
            .find(|(material, _)| copied.contains(&u32::try_from(*material).unwrap_or(u32::MAX)))
        {
            return Err(needs_rebuild(
                "material",
                *material,
                "terrain palettes and decals hold their own copies of texture contents",
            ));
        }
        Ok(Self {
            index,
            texture: prepare_plain_texture(index, texture)?,
            materials: uses,
        })
    }

    /// The device features the new texture needs.
    #[must_use]
    pub fn required_features(&self) -> wgpu::Features {
        texture_features(std::slice::from_ref(&self.texture))
    }

    /// The device storage the replacement asks for.
    #[must_use]
    pub fn allocations(&self) -> AllocationSet {
        let mut set = AllocationSet::default();
        texture_allocation(&mut set, &self.texture.as_prepared());
        set
    }
}

/// The materials that sample `handle`, each with its handle, or the first one that uses it
/// through shared state.
fn material_uses(
    scene: &RenderScene,
    handle: TextureHandle,
) -> Result<Vec<(usize, SceneMaterial)>, DrawError> {
    let mut uses = Vec::new();
    for (index, material) in scene.resources().materials().iter().enumerate() {
        let maps = [
            Some(material.albedo()),
            material.normal(),
            material.roughness_metallic(),
            material.ambient_occlusion(),
            material.self_illumination(),
        ];
        if maps.contains(&Some(handle)) || uses_texture(material, handle) {
            admit_material(index, material)?;
            uses.push((index, *material));
        }
    }
    Ok(uses)
}

/// The materials terrain palettes and decals copy texture contents for.
fn copied_materials(scene: &RenderScene) -> Result<BTreeSet<u32>, DrawError> {
    if scene
        .terrain()
        .is_some_and(|terrain| terrain.layered_source().is_some())
    {
        return Err(needs_rebuild(
            "texture",
            0,
            "layered terrain keeps its own copies of texture contents",
        ));
    }
    Ok(scene
        .terrain()
        .into_iter()
        .flat_map(|terrain| terrain.surfaces().iter().map(|s| s.material().raw()))
        .chain(scene.decals().iter().map(|decal| decal.material.raw()))
        .collect())
}

/// Whether a material refused for shared state names `handle` among its extra maps.
fn uses_texture(material: &SceneMaterial, handle: TextureHandle) -> bool {
    let atlas = material.impostor();
    let field = material
        .features()
        .and_then(|features| features.wind)
        .and_then(worldrunner_render_plan::SceneWind::field)
        .map(worldrunner_render_plan::ScenePlanarWindField::texture);
    let fade = match material.fade_coverage() {
        SceneFadeCoverage::Hashed => None,
        SceneFadeCoverage::Threshold { texture, .. } => Some(texture),
    };
    [
        atlas.and_then(|atlas| atlas.relief().map(|relief| relief.0)),
        atlas.and_then(worldrunner_render_plan::SceneImpostor::borders),
        field,
        fade,
    ]
    .contains(&Some(handle))
}

/// A mesh swapped in place.
#[derive(Debug)]
pub struct PreparedMeshReplacement {
    index: usize,
    mesh: prepared::PreparedMesh,
    selection: MeshSelection,
    skeletons: Vec<SkeletonHandle>,
    needs: Vec<SkinNeed>,
}

impl PreparedMeshReplacement {
    /// Prepares `mesh` as the new contents of `handle` in `scene`.
    ///
    /// # Errors
    ///
    /// Returns [`DrawError::NeedsRebuild`] when sky, environment or water effects are present,
    /// because they keep their own copies of mesh buffers, and the preparation errors of a full
    /// upload otherwise.
    pub fn new(
        scene: &RenderScene,
        handle: MeshHandle,
        mesh: &SceneMesh,
    ) -> Result<Self, DrawError> {
        let index = usize::try_from(handle.raw()).expect("u32 table index fits usize");
        require_plain_effects(scene, "mesh", index)?;
        Ok(Self {
            index,
            mesh: prepared::prepare_mesh(index, mesh)?,
            selection: MeshSelection::new(mesh),
            skeletons: crate::instances::skin::mesh_skeletons(mesh),
            needs: skin_needs(mesh),
        })
    }

    /// The device storage the replacement asks for.
    #[must_use]
    pub fn allocations(&self) -> AllocationSet {
        let mut set = AllocationSet::default();
        mesh_allocations(&mut set, &self.mesh);
        set
    }
}

impl SceneGpu {
    /// The table sizes an append to this scene must have been prepared against.
    #[must_use]
    pub fn resource_base(&self) -> ResourceBase {
        ResourceBase {
            textures: self.retained.textures.len(),
            materials: self.retained.material_uniforms.len(),
            meshes: self.meshes.len(),
        }
    }

    /// Keeps the storage of appended or replaced resources admitted until this resident is
    /// retired.
    pub fn retain_append_reservation(
        &mut self,
        reservation: worldrunner_render_gpu::configuration::AllocationReservation,
    ) {
        self.resource_counters.appended.push(reservation);
    }

    /// Refuses an append whose meshes name a skeleton or joint the resident tables lack, which
    /// would leave placements of the mesh that cannot be admitted.
    fn admit_skins(&self, prepared: &PreparedAppend) -> Result<(), DrawError> {
        for (offset, needs) in prepared.needs.iter().enumerate() {
            self.instances
                .skin
                .admit(needs)
                .map_err(|source| DrawError::SkinReference {
                    mesh: prepared.base.meshes + offset,
                    source,
                })?;
        }
        Ok(())
    }

    /// Refuses a replacement of mesh `replacement.index` that names a skeleton or joint the
    /// resident tables lack, or that a live placement's pose cannot drive, so that no placement
    /// of the mesh is left unable to draw.
    fn admit_replacement(&self, replacement: &PreparedMeshReplacement) -> Result<(), DrawError> {
        let index = replacement.index;
        let skin = &self.instances.skin;
        let refuse = |source| DrawError::SkinReference {
            mesh: index,
            source,
        };
        skin.admit(&replacement.needs).map_err(refuse)?;
        if replacement.skeletons.is_empty() {
            return Ok(());
        }
        for (_, object, _) in self
            .instances
            .batches
            .iter()
            .filter(|(key, _)| key.1 == index)
            .flat_map(|(_, batch)| batch.placements())
        {
            skin.check_pose(&replacement.skeletons, &object)
                .map_err(|error| refuse(error.into()))?;
        }
        Ok(())
    }

    /// Uploads the textures and meshes of `prepared`, binds its materials and makes them
    /// addressable after the resident ones. `source` is the append `prepared` was made from.
    ///
    /// Nothing changes when this fails.
    ///
    /// # Errors
    ///
    /// Returns [`DrawError`] for an append prepared against other table sizes, one whose meshes
    /// name skeletons or joints the resident tables lack, or a failed upload.
    pub fn append_prepared(
        &mut self,
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        pipelines: &ScenePipelines,
        prepared: PreparedAppend,
        source: &SceneAppend,
    ) -> Result<(), DrawError> {
        if prepared.base != self.resource_base() {
            return Err(needs_rebuild(
                "append",
                prepared.base.textures,
                "it was prepared against other resident tables",
            ));
        }
        self.admit_skins(&prepared)?;
        let queue = authority.queue();
        let (textures, new_views) = upload_textures(device, authority, &prepared.textures)?;
        let old_count = self.retained.textures.len();
        let (uniforms, tables) = {
            let resident = &self.retained.textures;
            let lookup = |handle: TextureHandle| -> TextureView {
                let index = table_index(handle);
                new_views.get(index.wrapping_sub(old_count)).map_or_else(
                    || resident[index].create_view(&TextureViewDescriptor::default()),
                    Clone::clone,
                )
            };
            let placeholder = self
                .retained
                .placeholder
                .create_view(&TextureViewDescriptor::default());
            let inputs = GroupInputs {
                placeholder: &placeholder,
                heights: &self.retained.contact_heights,
                view: &lookup,
            };
            new_material_uploads(device, queue, &inputs, &prepared.materials)
        };
        let meshes = self
            .arena
            .upload(device, authority.census(), &prepared.meshes);
        self.decals.append(source)?;
        self.resource_counters
            .append(&prepared.texture_bytes(), &prepared.mesh_bytes());
        self.resource_counters
            .impostors
            .extend(prepared.materials.iter().map(|m| m.impostor().is_some()));
        self.retained.textures.extend(textures);
        self.retained.lent_textures.get_mut().take();
        self.retained.material_uniforms.extend(uniforms);
        self.objects.extend(tables);
        self.objects.warm(pipelines.object_context());
        self.meshes.extend(meshes);
        self.retained.lent_meshes.get_mut().take();
        self.mesh_selection.extend(prepared.selection);
        self.instances
            .extend_resources(&prepared.materials, self.meshes.len());
        self.instances.skin.extend_meshes(prepared.skeletons);
        Ok(())
    }

    /// Uploads the new contents of one texture and rebinds the materials that sample it.
    ///
    /// Nothing changes when this fails.
    ///
    /// # Errors
    ///
    /// Returns [`DrawError`] for a handle outside the resident table or a failed upload.
    pub fn replace_texture_prepared(
        &mut self,
        device: &Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        pipelines: &ScenePipelines,
        replacement: &PreparedTextureReplacement,
    ) -> Result<(), DrawError> {
        let index = replacement.index;
        if index >= self.retained.textures.len() {
            return Err(needs_rebuild("texture", index, "it is not resident"));
        }
        let bytes = replacement.texture.as_prepared().payload_bytes();
        let texture = upload_texture(
            device,
            authority.queue(),
            &replacement.texture.as_prepared(),
        )?;
        let texture = authority.census().track_texture(
            worldrunner_render_gpu::census::ResourceClass::Texture,
            texture,
        );
        let fresh = texture.create_view(&TextureViewDescriptor::default());
        let tables = {
            let resident = &self.retained.textures;
            let lookup = |handle: TextureHandle| -> TextureView {
                let at = table_index(handle);
                if at == index {
                    fresh.clone()
                } else {
                    resident[at].create_view(&TextureViewDescriptor::default())
                }
            };
            let placeholder = self
                .retained
                .placeholder
                .create_view(&TextureViewDescriptor::default());
            let inputs = GroupInputs {
                placeholder: &placeholder,
                heights: &self.retained.contact_heights,
                view: &lookup,
            };
            let mut samplers = crate::sampling::MaterialSamplers::default();
            replacement
                .materials
                .iter()
                .map(|(at, material)| {
                    let sampler = samplers.get(device, material);
                    let uniform = &self.retained.material_uniforms[*at];
                    (*at, material_imports(&inputs, material, uniform, sampler))
                })
                .collect::<Vec<_>>()
        };
        self.resource_counters.replace_texture(index, bytes);
        self.retained.textures[index] = texture;
        self.retained.lent_textures.get_mut().take();
        for (at, table) in tables {
            self.objects.replace(at, table);
        }
        self.objects.warm(pipelines.object_context());
        // Alpha-tested casters cut their shadow from the replaced texels.
        self.casters.global();
        Ok(())
    }

    /// Uploads the new contents of one mesh. Placements keep drawing it from the same handle.
    ///
    /// Nothing changes when this fails.
    ///
    /// # Errors
    ///
    /// Returns [`DrawError`] for a handle outside the resident table, a replacement whose skin
    /// changes how the mesh's placements batch, or a skinned replacement that names skeletons or
    /// joints the resident tables lack or that the pose of a live placement cannot drive.
    pub fn replace_mesh_prepared(
        &mut self,
        device: &Device,
        replacement: PreparedMeshReplacement,
    ) -> Result<(), DrawError> {
        let index = replacement.index;
        if index >= self.meshes.len() {
            return Err(needs_rebuild("mesh", index, "it is not resident"));
        }
        if self.instances.skin.is_skinned(index) == replacement.skeletons.is_empty() {
            // Placements batch by mesh, and a skinned mesh's batches carry a skin stream.
            return Err(needs_rebuild(
                "mesh",
                index,
                "its skinning changed, and placements keep skin streams per mesh",
            ));
        }
        self.admit_replacement(&replacement)?;
        let bytes = mesh_payload_bytes(&replacement.mesh);
        let uploaded = self
            .arena
            .upload(
                device,
                &self.instances.census,
                std::slice::from_ref(&replacement.mesh),
            )
            .pop()
            .expect("one mesh uploaded");
        self.resource_counters.replace_mesh(index, bytes);
        self.meshes[index] = uploaded;
        self.retained.lent_meshes.get_mut().take();
        self.mesh_selection[index] = replacement.selection;
        self.instances
            .skin
            .replace_mesh(index, replacement.skeletons);
        self.instances.forget_mesh_bounds(index);
        self.casters.global();
        Ok(())
    }
}

/// One uniform buffer and one table of imports for each new material.
fn new_material_uploads(
    device: &Device,
    queue: &wgpu::Queue,
    inputs: &GroupInputs<'_>,
    materials: &[worldrunner_render_plan::SceneMaterial],
) -> (Vec<wgpu::Buffer>, Vec<LentMaterial>) {
    let mut samplers = crate::sampling::MaterialSamplers::default();
    let mut uniforms = Vec::with_capacity(materials.len());
    let mut tables = Vec::with_capacity(materials.len());
    for material in materials {
        let bytes = crate::pipeline::MaterialUniform::from_material(material).to_bytes();
        let uniform = upload_buffer(device, queue, "material", BufferUsages::UNIFORM, &bytes);
        let sampler = samplers.get(device, material);
        tables.push(LentMaterial {
            table: material_imports(inputs, material, &uniform, sampler),
            uniform: bytes,
        });
        uniforms.push(uniform);
    }
    (uniforms, tables)
}
