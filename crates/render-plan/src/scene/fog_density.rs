//! Validated extinction in inverse metres, including sub-micro densities.
use super::SceneValidationError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneFogDensity(u32);
impl SceneFogDensity {
    pub fn new(per_meter: f32) -> Result<Self, SceneValidationError> {
        if !per_meter.is_finite() || per_meter < 0.0 {
            return Err(SceneValidationError::UnsupportedInput {
                part: "fog density",
            });
        }
        Ok(Self(per_meter.max(0.0).to_bits()))
    }
    pub const fn per_meter(self) -> f32 {
        f32::from_bits(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_zero_and_sub_micro_values_survive_validation() {
        for value in [0.0, f32::MIN_POSITIVE, f32::EPSILON, 0.000_000_5, 1.0] {
            assert_eq!(
                SceneFogDensity::new(value).unwrap().per_meter().to_bits(),
                value.to_bits()
            );
        }
        assert_eq!(SceneFogDensity::new(-0.0).unwrap().per_meter().to_bits(), 0);
        for value in [-1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(SceneFogDensity::new(value).is_err());
        }
    }
}
