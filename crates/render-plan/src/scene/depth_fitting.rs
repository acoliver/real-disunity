//! CPU-owned depth-fitting request. Quantities use scene millimetres and elapsed milliseconds.
use super::{SceneEffects, SceneValidationError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneDepthSplits {
    Practical {
        lambda_micro: u32,
    },
    OverlapRecurrence {
        multiplier_micro: u32,
        overlap_micro: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneDepthAdoption {
    FrameIndexed {
        latency_frames: u64,
        contraction_frames: u64,
    },
    QuantizedPerBound {
        latency_frames: u64,
        contraction_delay_ms: u64,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneDepthFitting {
    pub near_step_mm: u32,
    pub far_step_mm: u32,
    pub minimum_span_mm: u32,
    pub splits: SceneDepthSplits,
    pub adoption: SceneDepthAdoption,
}
impl SceneDepthFitting {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        let valid_splits = match self.splits {
            SceneDepthSplits::Practical { lambda_micro } => lambda_micro <= 1_000_000,
            SceneDepthSplits::OverlapRecurrence {
                multiplier_micro,
                overlap_micro,
            } => (1_000_000..=16_000_000).contains(&multiplier_micro) && overlap_micro < 1_000_000,
        };
        let latency = match self.adoption {
            SceneDepthAdoption::FrameIndexed { latency_frames, .. }
            | SceneDepthAdoption::QuantizedPerBound { latency_frames, .. } => latency_frames,
        };
        if !valid_splits
            || latency == 0
            || self.near_step_mm == 0
            || self.far_step_mm == 0
            || self.minimum_span_mm == 0
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "depth-fitting policy",
            });
        }
        Ok(())
    }
}
impl SceneEffects {
    /// Interactive frames adopt delayed evidence; captures always refit their own frame.
    pub fn with_depth_fitting(
        mut self,
        settings: SceneDepthFitting,
    ) -> Result<Self, SceneValidationError> {
        settings.validate()?;
        self.depth_fitting = Some(settings);
        Ok(self)
    }
    pub const fn depth_fitting(&self) -> Option<&SceneDepthFitting> {
        self.depth_fitting.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_named_policies_and_requires_interactive_delay() {
        let mut settings = SceneDepthFitting {
            near_step_mm: 100,
            far_step_mm: 1000,
            minimum_span_mm: 1000,
            splits: SceneDepthSplits::OverlapRecurrence {
                multiplier_micro: 2_000_000,
                overlap_micro: 100_000,
            },
            adoption: SceneDepthAdoption::QuantizedPerBound {
                latency_frames: 2,
                contraction_delay_ms: 500,
            },
        };
        assert!(settings.validate().is_ok());
        settings.adoption = SceneDepthAdoption::FrameIndexed {
            latency_frames: 0,
            contraction_frames: 4,
        };
        assert!(settings.validate().is_err());
        settings.adoption = SceneDepthAdoption::FrameIndexed {
            latency_frames: 1,
            contraction_frames: 4,
        };
        settings.splits = SceneDepthSplits::OverlapRecurrence {
            multiplier_micro: 999_999,
            overlap_micro: 0,
        };
        assert!(settings.validate().is_err());
    }
}
