use super::SceneGpu;
use crate::sky::preparation::{PreparationError, PreparedEnvironmentMaps};
use worldrunner_render_plan::{SceneEnvironment, TextureHandle};

impl SceneGpu {
    /// Borrows already uploaded scene storage; no texture allocation or filtering occurs.
    pub fn environment_maps(
        &self,
        environment: SceneEnvironment,
    ) -> Result<PreparedEnvironmentMaps, PreparationError> {
        let texture = |handle: TextureHandle| {
            self.retained
                .textures
                .get(handle.raw() as usize)
                .ok_or(PreparationError::Source)
        };
        PreparedEnvironmentMaps::from_uploaded(
            texture(environment.cube)?,
            texture(environment.brdf_lut)?,
            environment.convention,
            environment.specular_intensity_micro,
        )
    }
}
