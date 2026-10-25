//! Authored policies for the existing separable bloom pyramid.
use crate::PostError;

/// Largest finite half-float value; tone-curve scalars and thresholds may use the full range.
pub(crate) const HALF_MAX: f32 = 65504.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrightCurve {
    #[default]
    AcesFilmic,
    Reinhard,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneCurveBright {
    pub curve: BrightCurve,
    pub maximum: f32,
    pub luminance: [f32; 3],
    pub exposure_scale: f32,
    pub contrast: f32,
    pub epsilon: f32,
}
impl ToneCurveBright {
    pub fn validate(self) -> Result<(), PostError> {
        let positive = [self.maximum, self.epsilon];
        let nonnegative = [self.exposure_scale, self.contrast];
        if positive
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.0 || *v > HALF_MAX)
            || nonnegative
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=HALF_MAX).contains(v))
            || self
                .luminance
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            || (self.luminance.iter().sum::<f32>() - 1.0).abs() > 0.0001
        {
            return Err(PostError::UnsupportedProfile("tone-curve bloom extraction"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BrightExtraction {
    #[default]
    SoftKnee,
    ToneCurve(ToneCurveBright),
}
impl BrightExtraction {
    /// Largest accepted `Bloom::threshold`. The soft knee compares colour values directly;
    /// a tone curve maps its threshold into `[0, 1)`, so any half-float value is meaningful.
    pub const fn threshold_limit(self) -> f32 {
        match self {
            Self::SoftKnee => 64.0,
            Self::ToneCurve(_) => HALF_MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BloomReduction {
    #[default]
    BoxPerTexel,
    /// Four bilinear samples at destination centre +/- one quarter source texel.
    QuarterTexelAfterAverage,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GaussianKernelRule {
    #[default]
    RadiusThird,
    AxisClampedPairs,
}

/// Zero-based indices into the ceiling-half reduction pyramid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BloomLevelSelection {
    pub first_blurred: u32,
    pub output: u32,
}
impl BloomLevelSelection {
    pub fn validate(self, count: u32) -> Result<(), PostError> {
        if count == 0
            || count > 8
            || self.first_blurred >= count
            || self.output < self.first_blurred
            || self.output >= count
        {
            return Err(PostError::UnsupportedProfile("bloom level selection"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BloomTintPass {
    #[default]
    Vertical,
    Horizontal,
}

/// Controls logical levels beyond the first 1x1 reduction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BloomTerminal {
    /// Reject a level count that would repeat 1x1.
    #[default]
    Reject,
    /// Keep reducing, blurring and folding each remaining level at 1x1.
    Repeat,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BloomPolicies {
    pub extraction: BrightExtraction,
    pub reduction: BloomReduction,
    pub kernel: GaussianKernelRule,
    pub selection: BloomLevelSelection,
    pub tint_pass: BloomTintPass,
    pub terminal: BloomTerminal,
}
impl BloomPolicies {
    pub fn validate(self, count: u32) -> Result<(), PostError> {
        self.selection.validate(count)?;
        if let BrightExtraction::ToneCurve(policy) = self.extraction {
            policy.validate()?;
        }
        Ok(())
    }
}
