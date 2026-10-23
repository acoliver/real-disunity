//! Optional complete HDR post recipes. Omission preserves the fixed-exposure path.
use super::SceneValidationError;
use serde::{Deserialize, Serialize};

/// Histogram metering and temporal adaptation, in explicit integer units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneAdaptiveExposure {
    /// Log2 luminance range endpoints in milli-stops, within `-24_000..=24_000`.
    pub log_min_milli: i32,
    pub log_max_milli: i32,
    /// Retained histogram percentile interval, in millionths.
    pub low_percentile_micro: u32,
    pub high_percentile_micro: u32,
    /// Target middle-grey luminance, in millionths.
    pub key_micro: u32,
    /// Exposure bounds, in millionths.
    pub min_exposure_micro: u32,
    pub max_exposure_micro: u32,
    /// Exponential approach rates per second, in milli-units.
    pub speed_up_milli: u32,
    pub speed_down_milli: u32,
    /// A fixed step makes captures independent of wall-clock timing.
    #[serde(deserialize_with = "required_option")]
    pub fixed_delta_microseconds: Option<u32>,
    /// Bypass metering/adaptation for deterministic fixed-exposure captures.
    #[serde(deserialize_with = "required_option")]
    pub fixed_exposure_micro: Option<u32>,
}

/// Bloom filter policy. The downsample and tent weights are normalized.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneBloom {
    pub threshold_micro: u32,
    pub soft_knee_micro: u32,
    pub intensity_micro: u32,
    /// Maximum pyramid levels, within 1..=8.
    pub levels: u32,
}

/// Optional v4 document extension. All members of a present recipe are explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneHdrPost {
    #[serde(deserialize_with = "required_option")]
    pub adaptive_exposure: Option<SceneAdaptiveExposure>,
    #[serde(deserialize_with = "required_option")]
    pub bloom: Option<SceneBloom>,
    /// FXAA directional preset, with subpixel strength in millionths.
    #[serde(deserialize_with = "required_option")]
    pub fxaa_subpixel_micro: Option<u32>,
}
impl SceneHdrPost {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        if let Some(a) = self.adaptive_exposure {
            check(
                "exposure log minimum",
                a.log_min_milli.into(),
                -24_000,
                23_999,
            )?;
            check(
                "exposure log maximum",
                a.log_max_milli.into(),
                i64::from(a.log_min_milli) + 1,
                24_000,
            )?;
            check(
                "exposure low percentile",
                a.low_percentile_micro.into(),
                0,
                999_999,
            )?;
            check(
                "exposure high percentile",
                a.high_percentile_micro.into(),
                i64::from(a.low_percentile_micro) + 1,
                1_000_000,
            )?;
            check("exposure key", a.key_micro.into(), 1, 1_000_000)?;
            check(
                "exposure minimum",
                a.min_exposure_micro.into(),
                1,
                64_000_000,
            )?;
            check(
                "exposure maximum",
                a.max_exposure_micro.into(),
                a.min_exposure_micro.into(),
                64_000_000,
            )?;
            for speed in [a.speed_up_milli, a.speed_down_milli] {
                check("exposure speed", speed.into(), 1, 100_000)?;
            }
            if let Some(delta) = a.fixed_delta_microseconds {
                check("exposure fixed delta", delta.into(), 0, 10_000_000)?;
            }
            if let Some(exposure) = a.fixed_exposure_micro {
                check("capture exposure", exposure.into(), 0, 64_000_000)?;
            }
        }
        if let Some(b) = self.bloom {
            check("bloom threshold", b.threshold_micro.into(), 0, 64_000_000)?;
            check("bloom knee", b.soft_knee_micro.into(), 0, 1_000_000)?;
            check("bloom intensity", b.intensity_micro.into(), 0, 2_000_000)?;
            check("bloom levels", b.levels.into(), 1, 8)?;
        }
        if let Some(weight) = self.fxaa_subpixel_micro {
            check("FXAA subpixel", weight.into(), 0, 1_000_000)?;
        }
        Ok(())
    }
}
fn check(
    part: &'static str,
    observed: i64,
    minimum: i64,
    maximum: i64,
) -> Result<(), SceneValidationError> {
    if !(minimum..=maximum).contains(&observed) {
        return Err(SceneValidationError::PostSettings { part });
    }
    Ok(())
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}
