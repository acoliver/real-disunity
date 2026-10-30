use super::SceneValidationError;
use serde::{Deserialize, Serialize};

/// Scene FXAA controls in millionths. Omission retains the existing filter policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneFxaaQuality {
    pub edge_threshold_micro: u32,
    pub minimum_threshold_micro: u32,
    pub subpixel_blend_micro: u32,
    pub luminance: SceneFxaaLuminance,
    pub kernel: SceneFxaaKernel,
}
impl Default for SceneFxaaQuality {
    fn default() -> Self {
        Self {
            edge_threshold_micro: 100_000,
            minimum_threshold_micro: 15_625,
            subpixel_blend_micro: 1_000_000,
            luminance: SceneFxaaLuminance::Linear,
            kernel: SceneFxaaKernel::Sobel,
        }
    }
}
impl SceneFxaaQuality {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        for (part, value) in [
            ("FXAA edge threshold", self.edge_threshold_micro),
            ("FXAA minimum threshold", self.minimum_threshold_micro),
            ("FXAA subpixel blend", self.subpixel_blend_micro),
        ] {
            if value > 1_000_000 {
                return Err(SceneValidationError::PostSettings { part });
            }
        }
        if let SceneFxaaKernel::Quality {
            preset,
            sample_scale_micro,
        } = self.kernel
            && (preset != 12 || sample_scale_micro.iter().any(|v| *v == 0 || *v > 4_000_000))
        {
            return Err(SceneValidationError::PostSettings {
                part: "FXAA quality preset or sample scale",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneFxaaLuminance {
    #[default]
    Linear,
    Srgb,
    Green,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneFxaaKernel {
    #[default]
    Sobel,
    SecondDifference,
    /// Preset 12 with search and final offset scales in millionths of a texel.
    Quality {
        preset: u8,
        sample_scale_micro: [u32; 2],
    },
}
