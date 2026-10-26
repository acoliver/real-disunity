//! Scene-wide lighting choices. All coefficients are linear, before exposure.
use super::{SceneEffects, SceneValidationError};

/// Prepared degree-two diffuse irradiance divided by pi, in millionths.
/// Coefficients multiply [1,x,y,z,xy,xz,yz,x²-y²,z²] in scene space (+Z up).
/// Negative coefficients are valid; evaluation clamps the final RGB to zero.
/// Multiplication by diffuse albedo produces outgoing radiance with no further pi factor.
/// Ramamoorthi and Hanrahan (2001): <https://graphics.stanford.edu/papers/envmap/>.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSh9([[i32; 3]; 9]);
impl SceneSh9 {
    /// Adopts prepared coefficients, bounded to +/-1024 per channel.
    pub fn new(coefficients_micro: [[i32; 3]; 9]) -> Result<Self, SceneValidationError> {
        if coefficients_micro
            .iter()
            .flatten()
            .any(|c| c.unsigned_abs() > 1_024_000_000)
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "SH9 coefficient range",
            });
        }
        Ok(Self(coefficients_micro))
    }
    pub const fn coefficients_micro(self) -> [[i32; 3]; 9] {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SceneDiffuseEnvironment {
    #[default]
    Hemisphere,
    Sh9(SceneSh9),
}

impl SceneEffects {
    /// Selects diffuse lighting without changing sun, exposure or specular lighting.
    #[must_use]
    pub fn with_diffuse_environment(mut self, environment: SceneDiffuseEnvironment) -> Self {
        self.diffuse_environment = environment;
        self
    }
    pub const fn diffuse_environment(&self) -> SceneDiffuseEnvironment {
        self.diffuse_environment
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_coefficients_accept_signed_values_and_reject_extremes() {
        assert!(SceneSh9::new([[-1_024_000_000; 3]; 9]).is_ok());
        assert!(SceneSh9::new([[1_024_000_001; 3]; 9]).is_err());
        assert!(SceneSh9::new([[i32::MIN; 3]; 9]).is_err());
        assert_eq!(
            SceneDiffuseEnvironment::default(),
            SceneDiffuseEnvironment::Hemisphere
        );
    }
}
