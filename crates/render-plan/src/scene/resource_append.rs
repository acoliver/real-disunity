use super::{SceneMaterial, SceneResources, SceneTexture, SceneValidationError};
use std::sync::Arc;

impl SceneResources {
    /// Adds textures and materials after the existing ones and validates only the new entries
    /// against the combined tables. On error the tables are as they were.
    ///
    /// The tables are shared copy-on-write: appending to a value whose tables another value
    /// still shares copies the existing entries once.
    pub(in crate::scene) fn append(
        &mut self,
        textures: Vec<SceneTexture>,
        materials: Vec<SceneMaterial>,
    ) -> Result<(), SceneValidationError> {
        if textures.is_empty() && materials.is_empty() {
            return Ok(());
        }
        let (first_texture, first_material) = (self.textures.len(), self.materials.len());
        for (part, count) in [
            ("texture table", first_texture + textures.len()),
            ("material table", first_material + materials.len()),
        ] {
            if u32::try_from(count).is_err() {
                return Err(SceneValidationError::ArithmeticOverflow { part });
            }
        }
        Arc::make_mut(&mut self.textures).extend(textures);
        Arc::make_mut(&mut self.materials).extend(materials);
        let result = self.validate_from(first_texture, first_material);
        if result.is_err() {
            Arc::make_mut(&mut self.textures).truncate(first_texture);
            Arc::make_mut(&mut self.materials).truncate(first_material);
        }
        result
    }

    /// Replaces one texture and revalidates the materials against it. On error the table is as
    /// it was.
    pub(in crate::scene) fn replace_texture(
        &mut self,
        index: usize,
        texture: SceneTexture,
    ) -> Result<(), SceneValidationError> {
        texture.validate()?;
        let old = std::mem::replace(&mut Arc::make_mut(&mut self.textures)[index], texture);
        let result = self.validate_from(self.textures.len(), 0);
        if result.is_err() {
            Arc::make_mut(&mut self.textures)[index] = old;
        }
        result
    }
}
