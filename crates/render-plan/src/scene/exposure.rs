//! Optional percentile exposure recipe, in explicit integer units.
use super::SceneValidationError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneExposureAdaptation {
    Exposure,
    Luminance,
    Instantaneous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenePercentileExposure {
    pub black_threshold_micro: u32,
    /// Strictly-greater cumulative selection; must be below one million.
    pub percentile_micro: u32,
    /// Krawczyk et al. (2005) automatic key multiplier.
    pub key_bias_micro: u32,
    /// Exposure multiplier before bloom addition.
    pub output_scale_micro: u32,
    /// Natural-exponential rates use the enclosing recipe's speeds.
    /// Luminance adaptation initializes to the measurement on reset.
    pub adaptation: SceneExposureAdaptation,
}
impl ScenePercentileExposure {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        if !(1..=64_000_000).contains(&self.black_threshold_micro)
            || self.percentile_micro >= 1_000_000
            || !(1..=64_000_000).contains(&self.key_bias_micro)
            || !(1..=64_000_000).contains(&self.output_scale_micro)
        {
            return Err(SceneValidationError::PostSettings {
                part: "percentile exposure",
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recipe_round_trip_and_validation() {
        let p = ScenePercentileExposure {
            black_threshold_micro: 5000,
            percentile_micro: 575_000,
            key_bias_micro: 575_608,
            output_scale_micro: 4_000_000,
            adaptation: SceneExposureAdaptation::Luminance,
        };
        p.validate().unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(
            serde_json::from_str::<ScenePercentileExposure>(&json).unwrap(),
            p
        );
        for invalid in [
            ScenePercentileExposure {
                percentile_micro: 1_000_000,
                ..p
            },
            ScenePercentileExposure {
                black_threshold_micro: 0,
                ..p
            },
            ScenePercentileExposure {
                key_bias_micro: 0,
                ..p
            },
            ScenePercentileExposure {
                output_scale_micro: 64_000_001,
                ..p
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}
