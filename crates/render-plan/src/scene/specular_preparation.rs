//! CPU request for the existing GGX environment preparation kernel.
use super::{SceneSpecularConvention, SceneValidationError};

/// Cube-only preparation. The scene retains its separately authored BRDF table
/// and applies environment intensity at shading time, without baking either here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSpecularPreparation {
    pub convention: SceneSpecularConvention,
    pub cube_size: u32,
    pub mip_count: u32,
    pub samples: u32,
    /// Cap source LOD at max(0, source mip count minus this value).
    pub source_lod_tail: u32,
}
impl SceneSpecularPreparation {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        if self.cube_size == 0
            || !self.cube_size.is_power_of_two()
            || self.cube_size > 1024
            || self.mip_count == 0
            || self.mip_count > self.cube_size.ilog2() + 1
            || self.samples == 0
            || self.samples > 4096
            || self.source_lod_tail == 0
            || self.source_lod_tail > 32
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "specular cube preparation settings",
            });
        }
        Ok(())
    }
}
