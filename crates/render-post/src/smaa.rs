//! SMAA 1x settings. The shader and lookup textures follow the reference
//! implementation at <https://github.com/iryoku/smaa> (MIT).
use crate::PostError;

/// How edges are found in the first pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SmaaEdgeDetection {
    /// Contrast of Rec. 709 luma. Cheapest, and the reference default.
    #[default]
    Luma,
    /// Largest per-channel difference. Finds edges between colors of equal luma.
    Color,
}

/// The reference `SMAA_PRESET_*` quality levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SmaaPreset {
    Low,
    Medium,
    High,
    Ultra,
}
impl SmaaPreset {
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];
}

/// SMAA 1x controls.
///
/// Edge decisions use sRGB-encoded values, because the reference thresholds are
/// tuned for gamma-space input and the chain supplies display-linear color.
/// Blending stays linear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmaaSettings {
    pub edge_detection: SmaaEdgeDetection,
    /// Edge sensitivity, above 0 and at most 0.5. Lower finds more edges.
    pub threshold: f32,
    /// Steps of the horizontal and vertical line searches on each side of a pixel,
    /// each covering two pixels, within 0..=112.
    pub max_search_steps: u32,
    /// Steps of the diagonal line searches within 0..=20, or `None` to skip diagonals.
    pub max_search_steps_diag: Option<u32>,
    /// How much sharp corners are rounded, within 0..=100, or `None` to skip corners.
    pub corner_rounding: Option<u32>,
}
impl SmaaSettings {
    pub const MAX_SEARCH_STEPS: u32 = 112;
    pub const MAX_SEARCH_STEPS_DIAG: u32 = 20;
    pub const MAX_CORNER_ROUNDING: u32 = 100;

    /// Reference `SMAA_PRESET_LOW`: threshold 0.15, 4 steps, no diagonals or corners.
    pub const LOW: Self = Self {
        edge_detection: SmaaEdgeDetection::Luma,
        threshold: 0.15,
        max_search_steps: 4,
        max_search_steps_diag: None,
        corner_rounding: None,
    };
    /// Reference `SMAA_PRESET_MEDIUM`: threshold 0.1, 8 steps, no diagonals or corners.
    pub const MEDIUM: Self = Self {
        threshold: 0.1,
        max_search_steps: 8,
        ..Self::LOW
    };
    /// Reference `SMAA_PRESET_HIGH`: threshold 0.1, 16 steps, 8 diagonal steps, rounding 25.
    pub const HIGH: Self = Self {
        threshold: 0.1,
        max_search_steps: 16,
        max_search_steps_diag: Some(8),
        corner_rounding: Some(25),
        ..Self::LOW
    };
    /// Reference `SMAA_PRESET_ULTRA`: threshold 0.05, 32 steps, 16 diagonal steps, rounding 25.
    pub const ULTRA: Self = Self {
        threshold: 0.05,
        max_search_steps: 32,
        max_search_steps_diag: Some(16),
        corner_rounding: Some(25),
        ..Self::LOW
    };

    pub const fn preset(preset: SmaaPreset) -> Self {
        match preset {
            SmaaPreset::Low => Self::LOW,
            SmaaPreset::Medium => Self::MEDIUM,
            SmaaPreset::High => Self::HIGH,
            SmaaPreset::Ultra => Self::ULTRA,
        }
    }

    pub fn validate(self) -> Result<(), PostError> {
        let threshold = self.threshold.is_finite() && self.threshold > 0.0 && self.threshold <= 0.5;
        let steps = self.max_search_steps <= Self::MAX_SEARCH_STEPS;
        let diagonal = self
            .max_search_steps_diag
            .is_none_or(|steps| steps <= Self::MAX_SEARCH_STEPS_DIAG);
        let corners = self
            .corner_rounding
            .is_none_or(|rounding| rounding <= Self::MAX_CORNER_ROUNDING);
        if threshold && steps && diagonal && corners {
            Ok(())
        } else {
            Err(PostError::UnsupportedProfile("SMAA settings"))
        }
    }

    /// Specialization constants for `smaa.wgsl`.
    pub(crate) fn constants(self) -> [(&'static str, f64); 7] {
        [
            ("THRESHOLD", self.threshold.into()),
            ("MAX_SEARCH_STEPS", self.max_search_steps.into()),
            (
                "MAX_SEARCH_STEPS_DIAG",
                self.max_search_steps_diag.unwrap_or(0).into(),
            ),
            (
                "DIAGONAL",
                f64::from(u8::from(self.max_search_steps_diag.is_some())),
            ),
            (
                "CORNERS",
                f64::from(u8::from(self.corner_rounding.is_some())),
            ),
            (
                "CORNER_ROUNDING",
                f64::from(self.corner_rounding.unwrap_or(0)) / 100.0,
            ),
            (
                "COLOR_EDGES",
                f64::from(u8::from(self.edge_detection == SmaaEdgeDetection::Color)),
            ),
        ]
    }
}
impl Default for SmaaSettings {
    /// The reference defaults without a preset define, which equal the High preset.
    fn default() -> Self {
        Self::HIGH
    }
}
impl From<SmaaPreset> for SmaaSettings {
    fn from(preset: SmaaPreset) -> Self {
        Self::preset(preset)
    }
}
