//! Submitted uploads share the same maps and bindings as prepared environments.
use super::{PreparationError, PreparedEnvironmentMaps};
use worldrunner_render_plan::SceneSpecularConvention;

impl PreparedEnvironmentMaps {
    /// Caller must have uploaded both textures on the rendering queue before installation.
    pub fn from_uploaded(
        cube: &wgpu::Texture,
        lut: &wgpu::Texture,
        convention: SceneSpecularConvention,
    ) -> Result<Self, PreparationError> {
        if cube.width() != cube.height()
            || cube.depth_or_array_layers() != 6
            || cube.dimension() != wgpu::TextureDimension::D2
            || cube.sample_count() != 1
            || !matches!(
                cube.format(),
                wgpu::TextureFormat::Rgba16Float | wgpu::TextureFormat::Bc6hRgbUfloat
            )
            || !cube.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(PreparationError::Source);
        }
        super::AuthoredBrdfTable::new(
            std::num::NonZeroU64::MIN,
            lut,
            convention,
            super::BrdfTableAxes::ViewCosineRoughness,
        )?;
        Ok(Self {
            cube: cube.clone(),
            lut: lut.clone(),
            authored_brdf: None,
            specular_intensity_micro: 1_000_000,
            transaction: None,
            convention,
        })
    }
}
