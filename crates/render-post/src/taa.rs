//! Temporal antialiasing settings.
//!
//! TAA accumulates jittered frames into a history image. It runs on the scene-referred
//! color before metering, bloom and tone mapping, so it needs no display-referred
//! intermediate and adaptive exposure never ghosts into the history.
use crate::PostError;

/// Longest accepted history weight. A weight of 1 would never admit a new sample.
const MAX_HISTORY_WEIGHT: f32 = 0.99;

/// Accepted neighborhood clip scale, in standard deviations of the 3x3 neighborhood.
const CLIP_SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;

/// Temporal antialiasing controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TaaSettings {
    /// Weight of the reprojected history in the blend, finite and within 0..=0.99. Larger
    /// values smooth more and respond more slowly; 0 keeps only the current frame.
    pub history_weight: f32,
    /// Scale of the neighborhood color box that history is clipped to, in standard
    /// deviations, finite and within 0.5..=2. Smaller values reject more ghosting.
    pub clip_scale: f32,
}
impl TaaSettings {
    /// A 90 percent history with a one-sigma clip box.
    pub const DEFAULT: Self = Self {
        history_weight: 0.9,
        clip_scale: 1.0,
    };

    /// The default clip box with the given history weight.
    pub const fn new(history_weight: f32) -> Self {
        Self {
            history_weight,
            ..Self::DEFAULT
        }
    }

    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] when a value is not finite or is out of range.
    pub fn validate(self) -> Result<(), PostError> {
        let weight_ok = self.history_weight.is_finite()
            && (0.0..=MAX_HISTORY_WEIGHT).contains(&self.history_weight);
        let clip_ok = self.clip_scale.is_finite() && CLIP_SCALE_RANGE.contains(&self.clip_scale);
        if weight_ok && clip_ok {
            Ok(())
        } else {
            Err(PostError::UnsupportedProfile("TAA settings"))
        }
    }
}
impl Default for TaaSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}
