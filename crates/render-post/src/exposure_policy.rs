//! Optional percentile metering over the existing histogram and history.
use crate::{AdaptiveExposure, PostError};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ExposureMetering {
    #[default]
    TrimmedMean,
    InterpolatedPercentile(PercentileExposure),
}

/// Rates use natural exponentials. For base-2 decay, multiply rates by ln(2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExposureAdaptation {
    #[default]
    Exposure,
    /// Adapt luminance before evaluating the key. Speeds select rising/falling luminance.
    /// Reset initializes luminance to the current measurement; zero delta holds history.
    Luminance,
    /// Positive steps snap to the measurement; zero delta holds initialized history.
    Instantaneous,
}

/// Nonblack bins use `floor(normalized_log * 255 + 1)`, saturated to 255.
/// Reconstruction uses `(fractional_bin - 1) / 256`, including bin zero.
/// Empty histograms use fractional bin zero. Black samples remain in the percentile count.
/// The enclosing exposure minimum is the floor; this policy has no upper exposure clamp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PercentileExposure {
    pub black_threshold: f32,
    /// Fraction in [0, 1). The sample target is truncated to an integer.
    pub percentile: f32,
    /// Multiplier for the luminance-dependent key of Krawczyk et al. (2005).
    pub key_bias: f32,
    /// Applied after the exposure floor, before bloom addition.
    pub output_scale: f32,
    pub adaptation: ExposureAdaptation,
}
impl ExposureMetering {
    pub(crate) fn validate(self) -> Result<(), PostError> {
        if let Self::InterpolatedPercentile(p) = self
            && (!p.black_threshold.is_finite()
                || !(f32::MIN_POSITIVE..=64.0).contains(&p.black_threshold)
                || !p.percentile.is_finite()
                || !(0.0..1.0).contains(&p.percentile)
                || !p.key_bias.is_finite()
                || !(f32::MIN_POSITIVE..=64.0).contains(&p.key_bias)
                || !p.output_scale.is_finite()
                || !(f32::MIN_POSITIVE..=64.0).contains(&p.output_scale))
        {
            return Err(PostError::UnsupportedProfile(
                "percentile exposure settings",
            ));
        }
        Ok(())
    }
}

/// CPU reference with the same finite-input policy as the existing histogram.
pub fn metering_bin(luminance: f32, a: AdaptiveExposure) -> usize {
    let ExposureMetering::InterpolatedPercentile(p) = a.metering else {
        return crate::histogram_bin(luminance, a.log_range);
    };
    let l = if luminance.is_finite() {
        luminance.max(0.0)
    } else {
        0.0
    };
    if l < p.black_threshold {
        return 0;
    }
    let t = ((l.log2() - a.log_range[0]) / (a.log_range[1] - a.log_range[0])).clamp(0.0, 1.0);
    (1u16..255)
        .find(|&i| t * 255.0 + 1.0 < f32::from(i + 1))
        .map_or(255, usize::from)
}

/// Strictly-greater cumulative selection with interpolation from the upper bin edge.
pub fn percentile_luminance(bins: &[u32; 256], range: [f32; 2], percentile: f32) -> f64 {
    let total: u32 = bins.iter().sum();
    let target = f64::from((count_float(total) * percentile).floor());
    let mut cumulative = 0u32;
    let mut position = 0.0;
    for (i, &count) in bins.iter().enumerate() {
        cumulative += count;
        if f64::from(cumulative) > target {
            position = f64::from(u16::try_from(i).unwrap())
                + (f64::from(cumulative) - target) / f64::from(count);
            break;
        }
    }
    ((position - 1.0) / 256.0 * f64::from(range[1] - range[0]) + f64::from(range[0])).exp2()
}

fn count_float(v: u32) -> f32 {
    f32::from(u16::try_from(v >> 16).unwrap()) * 65536.0
        + f32::from(u16::try_from(v & 65535).unwrap())
}
/// Published automatic key fit: Krawczyk, Myszkowski and Seidel (2005),
/// “Perceptual effects in real-time tone mapping”.
pub fn luminance_key(luminance: f32, bias: f32) -> f32 {
    (1.03 - 2.0 / (2.0 + (luminance + 1.0).log10())) * bias
}

pub(crate) fn constants(a: AdaptiveExposure) -> [(&'static str, f64); 5] {
    let ExposureMetering::InterpolatedPercentile(p) = a.metering else {
        return [
            ("PERCENTILE_METER", 0.0),
            ("BLACK_THRESHOLD", 0.005),
            ("PERCENTILE", 0.575),
            ("KEY_BIAS", 1.0),
            ("ADAPTATION", 0.0),
        ];
    };
    [
        ("PERCENTILE_METER", 1.0),
        ("BLACK_THRESHOLD", p.black_threshold.into()),
        ("PERCENTILE", p.percentile.into()),
        ("KEY_BIAS", p.key_bias.into()),
        (
            "ADAPTATION",
            match p.adaptation {
                ExposureAdaptation::Exposure => 0.0,
                ExposureAdaptation::Luminance => 1.0,
                ExposureAdaptation::Instantaneous => 2.0,
            },
        ),
    ]
}

pub(crate) fn output_constants(a: Option<AdaptiveExposure>) -> [(&'static str, f64); 2] {
    match a.map(|a| a.metering) {
        Some(ExposureMetering::InterpolatedPercentile(p)) => [
            ("EXPOSURE_SCALE", p.output_scale.into()),
            ("PERCENTILE_OUTPUT", 1.0),
        ],
        _ => [("EXPOSURE_SCALE", 1.0), ("PERCENTILE_OUTPUT", 0.0)],
    }
}
