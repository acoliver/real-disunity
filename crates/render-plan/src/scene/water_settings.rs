//! Optional water geometry and sampled surface inputs use the scene resource tables.
use super::{MeshHandle, SceneValidationError, TextureHandle};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneWaterGeometry {
    /// The body's east/north rectangle at its authored elevation.
    Rectangle,
    /// An authored surface mesh. Elevation is applied in the scene Z-up basis.
    Mesh(MeshHandle),
}

/// Surface inputs, separate from a body's location and optical coefficients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterSettings {
    pub geometry: SceneWaterGeometry,
    pub normal: Option<TextureHandle>,
    pub flow: Option<TextureHandle>,
    /// Authored maximum refraction displacement in millimetres, including zero.
    /// `None` preserves stock procedural refraction without an authored bound.
    /// Stock rendering rejects every `Some` bound until displacement limiting is implemented.
    pub refraction_millimeters: Option<u32>,
}
impl SceneWaterSettings {
    pub const STOCK: Self = Self {
        geometry: SceneWaterGeometry::Rectangle,
        normal: None,
        flow: None,
        refraction_millimeters: None,
    };
    pub(super) fn validate_handles(
        self,
        textures: u64,
        meshes: u64,
    ) -> Result<(), SceneValidationError> {
        if let SceneWaterGeometry::Mesh(mesh) = self.geometry {
            super::require_handle("water geometry", mesh.raw(), "mesh", meshes)?;
        }
        for texture in [self.normal, self.flow].into_iter().flatten() {
            super::require_handle("water surface", texture.raw(), "texture", textures)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn water_settings_share_existing_resource_bounds() {
        assert!(SceneWaterSettings::STOCK.validate_handles(0, 0).is_ok());
        for settings in [
            SceneWaterSettings {
                geometry: SceneWaterGeometry::Mesh(MeshHandle::from_raw(1)),
                ..SceneWaterSettings::STOCK
            },
            SceneWaterSettings {
                normal: Some(TextureHandle::from_raw(1)),
                ..SceneWaterSettings::STOCK
            },
            SceneWaterSettings {
                flow: Some(TextureHandle::from_raw(1)),
                ..SceneWaterSettings::STOCK
            },
        ] {
            assert!(settings.validate_handles(1, 1).is_err());
            assert!(settings.validate_handles(2, 2).is_ok());
        }
    }
}
