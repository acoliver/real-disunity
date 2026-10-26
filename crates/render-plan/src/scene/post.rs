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
    /// Omission preserves the existing tent bloom and default policies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<super::SceneBloomRecipe>,
}

/// Optional v4 document extension. All members of a present recipe are explicit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneHdrPost {
    /// Optional display curve override; omission preserves the host selection.
    #[serde(default)]
    pub tone_map: Option<SceneToneMap>,
    /// Optional complete grading recipe; omission is identity.
    #[serde(default)]
    pub grading: Option<SceneDisplayGrading>,
    #[serde(deserialize_with = "required_option")]
    pub adaptive_exposure: Option<SceneAdaptiveExposure>,
    #[serde(deserialize_with = "required_option")]
    pub bloom: Option<SceneBloom>,
    /// FXAA directional preset, with subpixel strength in millionths.
    #[serde(deserialize_with = "required_option")]
    pub fxaa_subpixel_micro: Option<u32>,
    /// Optional quality override, independent of the FXAA enable/strength control.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fxaa_quality: Option<super::SceneFxaaQuality>,
}
impl SceneHdrPost {
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        self.grading
            .map(SceneDisplayGrading::validate)
            .transpose()?;
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
        self.validate_bloom()?;
        self.fxaa_quality
            .map(super::SceneFxaaQuality::validate)
            .transpose()?;
        if let Some(weight) = self.fxaa_subpixel_micro {
            check("FXAA subpixel", weight.into(), 0, 1_000_000)?;
        }
        Ok(())
    }
    fn validate_bloom(&self) -> Result<(), SceneValidationError> {
        if let Some(b) = self.bloom {
            check("bloom threshold", b.threshold_micro.into(), 0, 64_000_000)?;
            check("bloom knee", b.soft_knee_micro.into(), 0, 1_000_000)?;
            check("bloom intensity", b.intensity_micro.into(), 0, 2_000_000)?;
            check("bloom levels", b.levels.into(), 1, 8)?;
            if let Some(recipe) = b.recipe {
                recipe.validate(b.levels)?;
            }
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

/// Scene-owned display curve, independent of the GPU implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneToneMap {
    Reinhard,
    Filmic,
    AcesFitted,
}

/// Linear scale and per-channel power in millionths, applied around the curve.
/// Order: exposure, bloom addition, linear scale, curve, powers, vignette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneDisplayGrading {
    pub linear_scale_micro: u32,
    pub display_power_micro: [u32; 3],
    pub vignette: Option<SceneVignette>,
}
/// Display-linear radial tint and darkening, all quantities in millionths.
/// Radius is `length((uv - 0.5) * axis_scale) / sqrt(0.5)`, at pixel centres.
/// Each smoothstep accepts either endpoint order, but not equal endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneVignette {
    pub axis_scale_micro: [u32; 2],
    pub tint_range_micro: [u32; 2],
    pub tint_micro: [u32; 4],
    pub darken_range_micro: [u32; 2],
    pub darken_opacity_micro: u32,
}
impl SceneDisplayGrading {
    pub fn validate(self) -> Result<(), SceneValidationError> {
        check(
            "linear scale",
            self.linear_scale_micro.into(),
            0,
            64_000_000,
        )?;
        for p in self.display_power_micro {
            check("display power", p.into(), 10_000, 16_000_000)?;
        }
        if let Some(v) = self.vignette {
            for x in v.axis_scale_micro {
                check("vignette axis", x.into(), 0, 16_000_000)?;
            }
            for x in v.tint_micro.into_iter().chain([v.darken_opacity_micro]) {
                check("vignette color or opacity", x.into(), 0, 1_000_000)?;
            }
            for r in [v.tint_range_micro, v.darken_range_micro] {
                if r[0] == r[1] {
                    return Err(SceneValidationError::PostSettings {
                        part: "vignette endpoints",
                    });
                }
                for x in r {
                    check("vignette range", x.into(), 0, 16_000_000)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grading_recipe_round_trip_and_invalid_ranges() {
        let grading = SceneDisplayGrading {
            linear_scale_micro: 2_000_000,
            display_power_micro: [1_200_000, 900_000, 1_100_000],
            vignette: Some(SceneVignette {
                axis_scale_micro: [1_000_000, 750_000],
                tint_range_micro: [0, 1_000_000],
                tint_micro: [250_000; 4],
                darken_range_micro: [1_000_000, 0],
                darken_opacity_micro: 500_000,
            }),
        };
        let recipe = SceneHdrPost {
            tone_map: Some(SceneToneMap::AcesFitted),
            grading: Some(grading),
            adaptive_exposure: None,
            bloom: None,
            fxaa_subpixel_micro: None,
            fxaa_quality: None,
        };
        recipe.validate().unwrap();
        let json = serde_json::to_string(&recipe).unwrap();
        assert_eq!(serde_json::from_str::<SceneHdrPost>(&json).unwrap(), recipe);
        assert!(
            SceneDisplayGrading {
                linear_scale_micro: 64_000_001,
                ..grading
            }
            .validate()
            .is_err()
        );
        assert!(
            SceneDisplayGrading {
                display_power_micro: [0; 3],
                ..grading
            }
            .validate()
            .is_err()
        );
        let v = grading.vignette.unwrap();
        assert!(
            SceneDisplayGrading {
                vignette: Some(SceneVignette {
                    darken_range_micro: [1; 2],
                    ..v
                }),
                ..grading
            }
            .validate()
            .is_err()
        );
    }
}
