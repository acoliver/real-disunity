use super::descriptor::logical_total;
use super::{SceneMaterial, SceneResources, SceneTexture, SceneValidationError};
use std::sync::Arc;

impl SceneResources {
    /// Validates the textures, their aggregate logical length and then the materials, and
    /// adopts the tables with the checked total.
    pub(super) fn adopt(
        textures: Arc<Vec<SceneTexture>>,
        materials: Arc<Vec<SceneMaterial>>,
    ) -> Result<Self, SceneValidationError> {
        for texture in textures.iter() {
            texture.validate()?;
        }
        let logical_bytes = logical_total(&textures)?;
        let resources = Self {
            textures,
            materials,
            logical_bytes,
            reducible_textures: Arc::new([]),
        };
        resources.validate_from(resources.textures.len(), 0)?;
        Ok(resources)
    }

    /// Replaces the materials while sharing the unchanged texture table.
    ///
    /// No texture element or payload is cloned, even if another resource value
    /// shares the textures. The complete result is validated as in [`Self::new`].
    ///
    /// # Errors
    ///
    /// Returns the same [`SceneValidationError`] as [`Self::new`] for an invalid
    /// texture payload, material channel, or material reference.
    pub fn with_materials(
        self,
        materials: Vec<SceneMaterial>,
    ) -> Result<Self, SceneValidationError> {
        let resources = Self {
            textures: self.textures,
            materials: Arc::new(materials),
            logical_bytes: self.logical_bytes,
            reducible_textures: self.reducible_textures,
        };
        resources.validate()?;
        Ok(resources)
    }

    /// Replaces the textures while sharing the unchanged material table.
    ///
    /// The complete result is validated as in [`Self::new`]; on error the receiver is
    /// unchanged because it is only borrowed.
    ///
    /// # Errors
    ///
    /// Returns the same [`SceneValidationError`] as [`Self::new`] for an invalid
    /// texture or a material reference the new table no longer satisfies.
    pub fn with_textures(&self, textures: Vec<SceneTexture>) -> Result<Self, SceneValidationError> {
        let mut resources = Self::adopt(Arc::new(textures), Arc::clone(&self.materials))?;
        resources.reducible_textures = Arc::clone(&self.reducible_textures);
        Ok(resources)
    }

    /// Returns both owned tables without copying their storage or payloads.
    ///
    /// Both tables must be uniquely owned. Successful extraction preserves the
    /// vectors' allocations and capacities, and every texture payload address.
    ///
    /// # Errors
    ///
    /// Returns this value unchanged if either table is shared. Neither table is
    /// extracted and no texture element, material, or payload is cloned.
    pub fn try_into_tables(mut self) -> Result<(Vec<SceneTexture>, Vec<SceneMaterial>), Self> {
        match (
            Arc::get_mut(&mut self.textures),
            Arc::get_mut(&mut self.materials),
        ) {
            (Some(textures), Some(materials)) => {
                Ok((std::mem::take(textures), std::mem::take(materials)))
            }
            _ => Err(self),
        }
    }
}
