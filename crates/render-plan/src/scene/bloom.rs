//! Explicit policies for the existing separable bloom pyramid.
use super::SceneValidationError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneBloomRecipe {
    pub extraction: SceneBrightExtraction,
    pub reduction: SceneBloomReduction,
    pub kernel: SceneGaussianKernelRule,
    pub selection: SceneBloomLevelSelection,
    pub tint_pass: SceneBloomTintPass,
    pub terminal: SceneBloomTerminal,
    /// The first `SceneBloom::levels` entries are used. All entries are validated.
    pub levels: [SceneGaussianLevel; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneBrightExtraction {
    SoftKnee,
    ToneCurve(SceneToneCurveBright),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneBrightCurve {
    AcesFilmic,
    Reinhard,
}

/// Scalars in millionths, up to the largest finite half float (65504).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneToneCurveBright {
    pub curve: SceneBrightCurve,
    pub maximum_micro: u64,
    pub luminance_micro: [u32; 3],
    pub exposure_scale_micro: u64,
    pub contrast_micro: u64,
    pub epsilon_micro: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneBloomReduction {
    BoxPerTexel,
    QuarterTexelAfterAverage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneGaussianKernelRule {
    RadiusThird,
    AxisClampedPairs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneBloomLevelSelection {
    pub first_blurred: u32,
    pub output: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneBloomTintPass {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneBloomTerminal {
    Reject,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneGaussianLevel {
    /// Discrete support radius in this level's texels, within 0..=64.
    pub radius: u32,
    /// Linear tint and weight in millionths, within `0..=64_000_000`.
    pub tint_micro: [u32; 3],
    pub weight_micro: u32,
}

impl SceneBloomRecipe {
    pub fn validate(self, count: u32) -> Result<(), SceneValidationError> {
        let s = self.selection;
        if !(1..=8).contains(&count)
            || s.first_blurred >= count
            || s.output < s.first_blurred
            || s.output >= count
        {
            return invalid("bloom level selection");
        }
        for level in self.levels {
            if level.radius > crate::post::MAX_GAUSSIAN_RADIUS
                || level
                    .tint_micro
                    .into_iter()
                    .chain([level.weight_micro])
                    .any(|v| v > 64_000_000)
            {
                return invalid("bloom Gaussian level");
            }
        }
        if let SceneBrightExtraction::ToneCurve(curve) = self.extraction {
            curve.validate()?;
        }
        Ok(())
    }
}

impl SceneToneCurveBright {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        if [self.maximum_micro, self.epsilon_micro]
            .into_iter()
            .any(|v| !(1..=65_504_000_000).contains(&v))
            || [self.exposure_scale_micro, self.contrast_micro]
                .into_iter()
                .any(|v| v > 65_504_000_000)
            || self.luminance_micro.into_iter().any(|v| v > 1_000_000)
            || self.luminance_micro.into_iter().map(u64::from).sum::<u64>() != 1_000_000
        {
            return invalid("bloom tone-curve extraction");
        }
        Ok(())
    }
}

fn invalid(part: &'static str) -> Result<(), SceneValidationError> {
    Err(SceneValidationError::PostSettings { part })
}

#[cfg(test)]
#[path = "bloom_tests.rs"]
mod tests;
