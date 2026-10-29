use super::{SceneMaterial, SceneResources, SceneTexture, SceneValidationError};
use std::sync::Arc;

impl SceneResources {
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
        };
        resources.validate()?;
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
