//! Optional water geometry and sampled surface inputs use the scene resource tables.
use super::{SceneValidationError, TextureHandle};

/// Surface inputs, separate from a body's location and optical coefficients.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterSettings {
    /// Linear RG encodes east/north normal components in [0, 1]; Z is reconstructed.
    pub normal: Option<TextureHandle>,
    /// Repeating world-space tile extent along east/north, in millimetres.
    pub tile_millimeters: [u32; 2],
    /// Signed RG flow at unit encoding speed, added to constant body velocity.
    pub flow_speed_millimeters_per_second: u32,
    pub flow: Option<TextureHandle>,
    /// Authored maximum refraction displacement in millimetres, including zero.
    /// `None` preserves stock procedural refraction without an authored bound.
    /// Stock rendering rejects every `Some` bound until displacement limiting is implemented.
    pub refraction_millimeters: Option<u32>,
}
impl SceneWaterSettings {
    pub const STOCK: Self = Self {
        normal: None,
        tile_millimeters: [1000; 2],
        flow_speed_millimeters_per_second: 1000,
        flow: None,
        refraction_millimeters: None,
    };
    pub(super) fn validate_handles(
        self,
        textures: u64,
        _meshes: u64,
    ) -> Result<(), SceneValidationError> {
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
