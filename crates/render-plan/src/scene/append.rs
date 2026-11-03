//! Growing and patching the resource tables of a scene in place.
//!
//! Appending checks the new entries against the combined tables, so its cost follows the size
//! of the addition. The result equals the scene built from the combined tables in one step.
use super::{
    MeshHandle, RenderScene, SceneMaterial, SceneMesh, SceneTexture, SceneValidationError,
    TextureHandle, require_handle, table_limit,
};
use std::sync::Arc;

/// Textures, materials and meshes to add after the ones a scene holds. The new entries take the
/// next handles in each table, in order, and may refer to old and new entries alike.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneAppend {
    pub textures: Vec<SceneTexture>,
    pub materials: Vec<SceneMaterial>,
    pub meshes: Vec<SceneMesh>,
}

impl RenderScene {
    /// Adds the entries of `append` after the existing ones. Existing objects, terrain,
    /// decals and effects keep their meaning because no handle moves.
    ///
    /// The tables are shared copy-on-write, so the first append to a scene whose tables
    /// another scene still shares copies them once.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] for an invalid entry or a handle outside the combined
    /// tables. The scene is unchanged.
    pub fn append_resources(&mut self, append: SceneAppend) -> Result<(), SceneValidationError> {
        let SceneAppend {
            textures,
            materials,
            meshes,
        } = append;
        let named = |offset: usize, source| SceneValidationError::AppendedEntry {
            table: "mesh",
            index: self.geometry.meshes.len() + offset,
            source: Box::new(source),
        };
        for (offset, mesh) in meshes.iter().enumerate() {
            mesh.validate().map_err(|error| named(offset, error))?;
        }
        let meshes_before = self.geometry.meshes.len();
        if u32::try_from(meshes_before + meshes.len()).is_err() {
            return Err(SceneValidationError::ArithmeticOverflow { part: "mesh table" });
        }
        let limit = table_limit(
            self.resources.materials().len() + materials.len(),
            "materials",
        )?;
        for (offset, mesh) in meshes.iter().enumerate() {
            mesh_materials(mesh, limit).map_err(|error| named(offset, error))?;
        }
        self.resources.append(textures, materials)?;
        if !meshes.is_empty() {
            Arc::make_mut(&mut self.geometry.meshes).extend(meshes);
        }
        Ok(())
    }

    /// Replaces one texture. Every handle keeps its meaning.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when `handle` is outside the table, the texture is
    /// invalid, or a material, effect or terrain layer can no longer use it. The scene is
    /// unchanged.
    pub fn replace_texture(
        &mut self,
        handle: TextureHandle,
        texture: SceneTexture,
    ) -> Result<(), SceneValidationError> {
        let limit = table_limit(self.resources.textures().len(), "textures")?;
        require_handle("texture replacement", handle.raw(), "texture", limit)?;
        let index = usize::try_from(handle.raw()).expect("u32 table index fits usize");
        self.resources.replace_texture(index, texture)
    }

    /// Replaces one mesh. Every handle keeps its meaning.
    ///
    /// # Errors
    ///
    /// Returns [`SceneValidationError`] when `handle` is outside the table, the mesh is
    /// invalid, or a level material slot names a material outside the table. The scene is
    /// unchanged.
    pub fn replace_mesh(
        &mut self,
        handle: MeshHandle,
        mesh: SceneMesh,
    ) -> Result<(), SceneValidationError> {
        let limit = table_limit(self.geometry.meshes.len(), "meshes")?;
        require_handle("mesh replacement", handle.raw(), "mesh", limit)?;
        mesh.validate()?;
        let materials = table_limit(self.resources.materials().len(), "materials")?;
        mesh_materials(&mesh, materials)?;
        let index = usize::try_from(handle.raw()).expect("u32 table index fits usize");
        Arc::make_mut(&mut self.geometry.meshes)[index] = mesh;
        Ok(())
    }
}

/// Requires every level material slot of `mesh` to address a material table of `limit` entries.
fn mesh_materials(mesh: &SceneMesh, limit: u64) -> Result<(), SceneValidationError> {
    for slot in mesh
        .lods()
        .iter()
        .flat_map(|lod| lod.material_slots().into_iter().flatten())
    {
        require_handle(
            "LOD material slot",
            slot.material().raw(),
            "material",
            limit,
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "append_tests.rs"]
mod tests;
