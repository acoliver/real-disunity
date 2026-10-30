//! Shared prefiltered radiance and split-sum coefficients addressed by scene handles.
use super::{
    SceneEffects, SceneResources, SceneSpecularConvention, SceneTexture, SceneValidationError,
    TexelFormat, TextureColorSpace, TextureHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneEnvironmentConsumers {
    Water,
    Reflections,
    Both,
}
impl SceneEnvironmentConsumers {
    pub const fn water(self) -> bool {
        matches!(self, Self::Water | Self::Both)
    }
    pub const fn reflections(self) -> bool {
        matches!(self, Self::Reflections | Self::Both)
    }
}

/// Cube metadata belongs to `SceneSpecularCube`. LUT RG stores A,B at
/// x = view cosine, y = roughness, under the declared integration convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneEnvironment {
    pub cube: TextureHandle,
    pub brdf_lut: TextureHandle,
    pub convention: SceneSpecularConvention,
    pub consumers: SceneEnvironmentConsumers,
}
impl SceneEnvironment {
    pub fn validate(
        self,
        resources: &SceneResources,
        selected: SceneSpecularConvention,
    ) -> Result<(), SceneValidationError> {
        let cube = lookup(resources, self.cube)?;
        let lut = lookup(resources, self.brdf_lut)?;
        if !matches!(cube.payload(), SceneTexture::SpecularCube(_)) {
            return Err(invalid("environment cube format"));
        }
        if !matches!(lut.payload(), SceneTexture::Texels {
            format: TexelFormat::Rg16Float | TexelFormat::Rgba16Float,
            color_space: TextureColorSpace::Linear, width, height, mips: 1, ..
        } if width == height)
        {
            return Err(invalid("environment BRDF LUT format"));
        }
        if self.convention.roughness != selected.roughness
            || self.convention.visibility != selected.visibility
        {
            return Err(invalid("environment BRDF convention mismatch"));
        }
        Ok(())
    }
}
fn lookup(
    resources: &SceneResources,
    handle: TextureHandle,
) -> Result<&SceneTexture, SceneValidationError> {
    resources
        .textures()
        .get(handle.raw() as usize)
        .ok_or(SceneValidationError::HandleOutOfRange {
            part: "environment",
            resource: "texture",
            handle: handle.raw(),
            limit: resources.textures().len() as u64,
        })
}
fn invalid(part: &'static str) -> SceneValidationError {
    SceneValidationError::UnsupportedInput { part }
}
impl SceneEffects {
    pub(super) fn validate_environment_consumers(&self) -> Result<(), SceneValidationError> {
        let Some(environment) = self.environment else {
            return Ok(());
        };
        if environment.consumers.water() && self.water_bodies().is_empty()
            || environment.consumers.reflections() && !self.ssr().is_some_and(|ssr| ssr.enabled)
        {
            return Err(invalid("environment consumer is not enabled"));
        }
        Ok(())
    }
    /// Cross-table references and integration conventions are checked by `RenderScene`.
    #[must_use]
    pub const fn with_environment(mut self, environment: Option<SceneEnvironment>) -> Self {
        self.environment = environment;
        self
    }
    pub const fn environment(&self) -> Option<SceneEnvironment> {
        self.environment
    }
}
