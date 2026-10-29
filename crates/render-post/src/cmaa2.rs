//! CMAA2 settings. The shader follows the reference implementation at
//! <https://github.com/GameTechDev/CMAA2> (Apache-2.0, see `cmaa2.wgsl`).
use crate::PostError;

/// The reference `CMAA2_STATIC_QUALITY_PRESET` levels. They differ only in the edge threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmaa2Preset {
    Low,
    Medium,
    High,
    Ultra,
}
impl Cmaa2Preset {
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];
}

/// CMAA2 controls for the single-sample path.
///
/// Edge decisions use the square root of display-linear color as a luma approximation of
/// sRGB, as the reference does for linear input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cmaa2Settings {
    /// Smallest luma step treated as an edge, above 0 and at most 0.5. Lower finds more edges.
    pub edge_threshold: f32,
    /// The reference `CMAA2_EXTRA_SHARPNESS`: stronger local contrast adaptation and
    /// weaker shape blending, which keeps more text and small detail at the cost of less smoothing.
    pub extra_sharpness: bool,
}
impl Cmaa2Settings {
    pub const MAX_EDGE_THRESHOLD: f32 = 0.5;

    /// Reference preset 0, edge threshold 0.15.
    pub const LOW: Self = Self {
        edge_threshold: 0.15,
        extra_sharpness: false,
    };
    /// Reference preset 1, edge threshold 0.10.
    pub const MEDIUM: Self = Self {
        edge_threshold: 0.10,
        ..Self::LOW
    };
    /// Reference preset 2 and the reference default, edge threshold 0.07.
    pub const HIGH: Self = Self {
        edge_threshold: 0.07,
        ..Self::LOW
    };
    /// Reference preset 3, edge threshold 0.05.
    pub const ULTRA: Self = Self {
        edge_threshold: 0.05,
        ..Self::LOW
    };

    pub const fn preset(preset: Cmaa2Preset) -> Self {
        match preset {
            Cmaa2Preset::Low => Self::LOW,
            Cmaa2Preset::Medium => Self::MEDIUM,
            Cmaa2Preset::High => Self::HIGH,
            Cmaa2Preset::Ultra => Self::ULTRA,
        }
    }

    /// The same settings with the reference `CMAA2_EXTRA_SHARPNESS` option on.
    #[must_use]
    pub const fn with_extra_sharpness(self) -> Self {
        Self {
            extra_sharpness: true,
            ..self
        }
    }

    pub fn validate(self) -> Result<(), PostError> {
        if self.edge_threshold.is_finite()
            && self.edge_threshold > 0.0
            && self.edge_threshold <= Self::MAX_EDGE_THRESHOLD
        {
            Ok(())
        } else {
            Err(PostError::UnsupportedProfile("CMAA2 settings"))
        }
    }

    /// Specialization constants for `cmaa2.wgsl`.
    pub(crate) fn constants(self) -> [(&'static str, f64); 2] {
        [
            ("THRESHOLD", self.edge_threshold.into()),
            ("EXTRA_SHARPNESS", f64::from(u8::from(self.extra_sharpness))),
        ]
    }
}
impl Default for Cmaa2Settings {
    /// The reference default preset, High.
    fn default() -> Self {
        Self::HIGH
    }
}
impl From<Cmaa2Preset> for Cmaa2Settings {
    fn from(preset: Cmaa2Preset) -> Self {
        Self::preset(preset)
    }
}
