//! GPU-free SMAA 1x authoring, using integer micro-units at the scene boundary.
use super::{SceneValidationError, effects::require_range};

/// Contrast input for SMAA edge detection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSmaaEdgeDetection {
    /// Rec. 709 luma contrast.
    Luma,
    /// Largest per-channel contrast, including equal-luma color edges.
    Color,
}

/// SMAA 1x reference quality presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSmaaPreset {
    Low,
    Medium,
    High,
    Ultra,
}

/// SMAA 1x parameters, validated before a scene adopts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneSmaaSettings {
    pub edge_detection: SceneSmaaEdgeDetection,
    /// Edge threshold in millionths, within `1..=500_000`.
    pub threshold_micro: u32,
    /// Horizontal and vertical search steps per side, within `0..=112`.
    pub max_search_steps: u32,
    /// Diagonal search steps within `0..=20`; `None` disables diagonal handling.
    pub max_search_steps_diag: Option<u32>,
    /// Corner rounding within `0..=100`; `None` disables corner handling.
    pub corner_rounding: Option<u32>,
}

impl SceneSmaaSettings {
    /// Threshold 0.15, four search steps, no diagonal or corner handling.
    pub const LOW: Self = Self {
        edge_detection: SceneSmaaEdgeDetection::Luma,
        threshold_micro: 150_000,
        max_search_steps: 4,
        max_search_steps_diag: None,
        corner_rounding: None,
    };
    /// Threshold 0.1, eight search steps, no diagonal or corner handling.
    pub const MEDIUM: Self = Self {
        threshold_micro: 100_000,
        max_search_steps: 8,
        ..Self::LOW
    };
    /// Threshold 0.1, sixteen search steps, eight diagonal steps, corner rounding 25.
    pub const HIGH: Self = Self {
        threshold_micro: 100_000,
        max_search_steps: 16,
        max_search_steps_diag: Some(8),
        corner_rounding: Some(25),
        ..Self::LOW
    };
    /// Threshold 0.05, thirty-two search steps, sixteen diagonal steps, corner rounding 25.
    pub const ULTRA: Self = Self {
        threshold_micro: 50_000,
        max_search_steps: 32,
        max_search_steps_diag: Some(16),
        corner_rounding: Some(25),
        ..Self::LOW
    };

    #[must_use]
    pub const fn preset(preset: SceneSmaaPreset) -> Self {
        match preset {
            SceneSmaaPreset::Low => Self::LOW,
            SceneSmaaPreset::Medium => Self::MEDIUM,
            SceneSmaaPreset::High => Self::HIGH,
            SceneSmaaPreset::Ultra => Self::ULTRA,
        }
    }

    /// Validates the post executor's ranges in scene units.
    ///
    /// # Errors
    /// Returns [`SceneValidationError::RangeViolation`] for the first out-of-range field.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        require_range(
            "SMAA threshold_micro",
            u64::from(self.threshold_micro),
            1,
            500_000,
        )?;
        require_range(
            "SMAA max_search_steps",
            u64::from(self.max_search_steps),
            0,
            112,
        )?;
        if let Some(steps) = self.max_search_steps_diag {
            require_range("SMAA max_search_steps_diag", u64::from(steps), 0, 20)?;
        }
        if let Some(rounding) = self.corner_rounding {
            require_range("SMAA corner_rounding", u64::from(rounding), 0, 100)?;
        }
        Ok(())
    }
}
