//! Independent radiometric contributions for a celestial mesh.
use super::{SceneValidationError, material_features::bounded};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneCelestialLighting([u32; 3]);
impl SceneCelestialLighting {
    pub const NEUTRAL: Self = Self([1_000_000; 3]);
    /// Direct diffuse, environment and self-illumination multipliers, in millionths.
    /// Direct specular retains the scene sun's separate highlight convention.
    pub fn new(
        direct_diffuse_micro: u32,
        environment_micro: u32,
        emission_micro: u32,
    ) -> Result<Self, SceneValidationError> {
        let values = [direct_diffuse_micro, environment_micro, emission_micro];
        for value in values {
            bounded("celestial lighting", value, 64_000_000)?;
        }
        Ok(Self(values))
    }
    pub const fn contributions_micro(self) -> [u32; 3] {
        self.0
    }
}
impl Default for SceneCelestialLighting {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contributions_validate_independently_and_default_to_identity() {
        assert_eq!(
            SceneCelestialLighting::default().contributions_micro(),
            [1_000_000; 3]
        );
        for values in [[0, 64_000_000, 0], [64_000_000, 0, 64_000_000]] {
            assert_eq!(
                SceneCelestialLighting::new(values[0], values[1], values[2])
                    .unwrap()
                    .contributions_micro(),
                values
            );
        }
        for values in [[64_000_001, 0, 0], [0, 64_000_001, 0], [0, 0, 64_000_001]] {
            assert!(SceneCelestialLighting::new(values[0], values[1], values[2]).is_err());
        }
    }
}
