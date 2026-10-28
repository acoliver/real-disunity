//! Explicit, bounded screen-space reflection policy.
use super::SceneValidationError;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "ssr_footprint_tests.rs"]
mod footprint_tests;

/// How reflected radiance contributes to the forward-lit scene before exposure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneSsrComposition {
    #[default]
    Additive,
    ReplaceEnvironment,
}

/// Depth-grid stepping and reduction used by reflection tracing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneSsrTraversal {
    #[default]
    Conservative,
    /// Major-axis grid stepping with averaged depth levels and two endpoint samples.
    Grid,
}

/// Pixel coordinates used by grid traversal, including skip and hit offsets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneSsrGridDomain {
    #[default]
    FullResolution,
    /// Uses reflection output pixels and depth mip 1 for half-resolution output.
    ReflectionOutput,
}

/// Roughness-dependent allowance for traversing behind visible depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneSsrPassBehind {
    pub minimum_steps: u32,
    pub maximum_steps: u32,
    pub roughness_start_micro: u32,
    pub roughness_end_micro: u32,
}

/// Application colour LOD. The roughness cutoff still controls fading and rejection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SceneSsrLodMapping {
    #[default]
    CutoffNormalized,
    /// max(roughness * (level count - 1) - log2(downsample), 0).
    RoughnessDownsample,
}

/// Optional scene/document reflection settings. No temporal history is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneSsr {
    /// Whether reflection passes are installed.
    pub enabled: bool,
    /// Hierarchical traversal budget, within 1..=1024.
    /// Per frame, at most output width * output height * MSAA samples * `max_steps`
    /// iterations run. Grid iterations perform two depth loads, independent of level count.
    pub max_steps: u32,
    #[serde(default)]
    pub traversal: SceneSsrTraversal,
    #[serde(default)]
    pub grid_domain: SceneSsrGridDomain,
    /// Grid levels starting at the domain's base mip. None retains four levels
    /// (or fewer on small extents); Some must fit the available ceil-divided chain.
    #[serde(default)]
    pub grid_level_count: Option<u32>,
    /// Base accepted depth penetration, within 1..=1000 millimetres.
    pub thickness_millimeters: u32,
    /// Additional thickness per metre of surface depth, within 0..=1000000.
    #[serde(default)]
    pub thickness_slope_micro: u32,
    /// Maximum ray length, within 1..=4000000 millimetres.
    #[serde(default = "default_distance")]
    pub max_distance_millimeters: u32,
    /// Trace-domain pixels skipped at the origin, within 0..=4096.
    #[serde(default = "default_skip")]
    pub origin_skip_pixels: u32,
    /// Number of too-deep finest-level spans allowed before terminating, within 0..=256.
    /// Zero rejects the first such span.
    #[serde(default)]
    pub pass_behind_steps: u32,
    /// Overrides the constant count with a clamped, rounded roughness interpolation.
    #[serde(default)]
    pub roughness_pass_behind: Option<SceneSsrPassBehind>,
    /// Gather application AO with exponential depth weights relative to the first tap.
    #[serde(default)]
    pub depth_relative_ao: bool,
    /// Optional start of the smooth roughness fade. None uses 75% of the cutoff.
    #[serde(default)]
    pub roughness_fade_start_micro: Option<u32>,
    /// Surfaces at or above this roughness receive no SSR, within 1..=1000000.
    pub roughness_cutoff_micro: u32,
    /// Optional reflected linear luminance ceiling, within 1..=1000000000 milli-units.
    /// When enabled, positive luminance below 0.0001 attenuates RGB by luminance / 0.0001,
    /// even below the ceiling, matching the reference formula. Confidence is unchanged.
    #[serde(default)]
    pub luminance_limit_milli: Option<u32>,
    /// Trace resolution: 1000000 for full or 500000 for half resolution.
    pub resolution_scale_micro: u32,
    /// Rounding of the reflection output and each colour reduction level.
    #[serde(default)]
    pub radiance_rounding: super::SceneDownsampleRounding,
    #[serde(default)]
    pub application_lod: SceneSsrLodMapping,
    #[serde(default)]
    pub composition: SceneSsrComposition,
}
const fn default_distance() -> u32 {
    100_000
}
const fn default_skip() -> u32 {
    2
}
impl Default for SceneSsr {
    fn default() -> Self {
        Self {
            enabled: true,
            max_steps: 256,
            traversal: SceneSsrTraversal::Conservative,
            grid_domain: SceneSsrGridDomain::FullResolution,
            grid_level_count: None,
            thickness_millimeters: 100,
            thickness_slope_micro: 0,
            max_distance_millimeters: 100_000,
            origin_skip_pixels: 2,
            pass_behind_steps: 0,
            roughness_pass_behind: None,
            depth_relative_ao: false,
            roughness_fade_start_micro: None,
            roughness_cutoff_micro: 600_000,
            luminance_limit_milli: None,
            resolution_scale_micro: 1_000_000,
            radiance_rounding: super::SceneDownsampleRounding::Ceil,
            application_lod: SceneSsrLodMapping::CutoffNormalized,
            composition: SceneSsrComposition::Additive,
        }
    }
}
impl SceneSsr {
    /// Validated, nonzero output dimensions and up to six colour levels.
    pub fn radiance_extents(&self, size: [u32; 2]) -> Vec<[u32; 2]> {
        assert!(!size.contains(&0), "nonzero reflection extent");
        self.validate().expect("validated reflection policy");
        let mut extent = if self.resolution_scale_micro == 500_000 {
            self.radiance_rounding.half_extent(size)
        } else {
            size
        };
        let mut extents = Vec::new();
        loop {
            extents.push(extent);
            if extent == [1, 1] || extents.len() == 6 {
                break;
            }
            extent = self.radiance_rounding.half_extent(extent);
        }
        extents
    }
    /// Validates even disabled settings, so enabling never changes their interpretation.
    pub fn validate(&self) -> Result<(), SceneValidationError> {
        self.validate_traversal()?;
        if let Some(levels) = self.grid_level_count {
            check("ssr grid level count", levels, 1, 33)?;
        }
        if let Some(curve) = self.roughness_pass_behind {
            check("ssr pass behind minimum", curve.minimum_steps, 0, 256)?;
            check(
                "ssr pass behind maximum",
                curve.maximum_steps,
                curve.minimum_steps,
                256,
            )?;
            check(
                "ssr pass behind roughness end",
                curve.roughness_end_micro,
                1,
                1_000_000,
            )?;
            check(
                "ssr pass behind roughness start",
                curve.roughness_start_micro,
                0,
                curve.roughness_end_micro - 1,
            )?;
        }
        let start = self.roughness_fade_start_micro.unwrap_or(0);
        check(
            "ssr roughness fade start",
            start,
            0,
            self.roughness_cutoff_micro - 1,
        )?;
        if let Some(limit) = self.luminance_limit_milli {
            check("ssr luminance limit", limit, 1, 1_000_000_000)?;
        }
        if !matches!(self.resolution_scale_micro, 500_000 | 1_000_000) {
            return Err(SceneValidationError::RangeViolation {
                part: "ssr resolution_scale_micro",
                observed: self.resolution_scale_micro.into(),
                minimum: 500_000,
                maximum: 1_000_000,
            });
        }
        Ok(())
    }
    fn validate_traversal(&self) -> Result<(), SceneValidationError> {
        for (part, value, min, max) in [
            ("ssr max_steps", self.max_steps, 1, 1024),
            (
                "ssr thickness_millimeters",
                self.thickness_millimeters,
                1,
                1000,
            ),
            (
                "ssr thickness slope",
                self.thickness_slope_micro,
                0,
                1_000_000,
            ),
            (
                "ssr max distance",
                self.max_distance_millimeters,
                1,
                4_000_000,
            ),
            ("ssr origin skip", self.origin_skip_pixels, 0, 4096),
            ("ssr pass behind", self.pass_behind_steps, 0, 256),
            (
                "ssr roughness_cutoff_micro",
                self.roughness_cutoff_micro,
                1,
                1_000_000,
            ),
        ] {
            check(part, value, min, max)?;
        }
        Ok(())
    }
}
fn check(part: &'static str, value: u32, min: u32, max: u32) -> Result<(), SceneValidationError> {
    if value < min || value > max {
        return Err(SceneValidationError::RangeViolation {
            part,
            observed: value.into(),
            minimum: min.into(),
            maximum: max.into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_traversal_fails_during_deserialization() {
        let mut value = serde_json::to_value(SceneSsr::default()).unwrap();
        value["traversal"] = serde_json::json!("Unknown");
        assert!(serde_json::from_value::<SceneSsr>(value).is_err());
        let mut value = serde_json::to_value(SceneSsr::default()).unwrap();
        value["grid_domain"] = serde_json::json!("Unknown");
        assert!(serde_json::from_value::<SceneSsr>(value).is_err());
    }

    #[test]
    fn controls_validate_even_when_disabled() {
        let invalid = [
            SceneSsr {
                max_steps: 1025,
                ..Default::default()
            },
            SceneSsr {
                grid_level_count: Some(0),
                ..Default::default()
            },
            SceneSsr {
                grid_level_count: Some(34),
                ..Default::default()
            },
            SceneSsr {
                max_distance_millimeters: 0,
                ..Default::default()
            },
            SceneSsr {
                max_distance_millimeters: 4_000_001,
                ..Default::default()
            },
            SceneSsr {
                thickness_slope_micro: 1_000_001,
                ..Default::default()
            },
            SceneSsr {
                origin_skip_pixels: 4097,
                ..Default::default()
            },
            SceneSsr {
                pass_behind_steps: 257,
                ..Default::default()
            },
            SceneSsr {
                roughness_fade_start_micro: Some(600_000),
                ..Default::default()
            },
            SceneSsr {
                roughness_cutoff_micro: 0,
                ..Default::default()
            },
            SceneSsr {
                luminance_limit_milli: Some(0),
                ..Default::default()
            },
            SceneSsr {
                luminance_limit_milli: Some(1_000_000_001),
                ..Default::default()
            },
        ];
        for policy in invalid {
            for enabled in [false, true] {
                assert!(SceneSsr { enabled, ..policy }.validate().is_err());
            }
        }
    }

    #[test]
    fn defaults_and_authored_controls_roundtrip() {
        let defaults: SceneSsr = serde_json::from_str(r#"{"enabled":true,"max_steps":256,"thickness_millimeters":100,"roughness_cutoff_micro":600000,"resolution_scale_micro":1000000}"#).unwrap();
        assert_eq!(defaults, SceneSsr::default());
        assert!(defaults.validate().is_ok());
        let authored = SceneSsr {
            traversal: SceneSsrTraversal::Grid,
            grid_domain: SceneSsrGridDomain::ReflectionOutput,
            grid_level_count: Some(8),
            depth_relative_ao: true,
            roughness_pass_behind: Some(SceneSsrPassBehind {
                minimum_steps: 1,
                maximum_steps: 9,
                roughness_start_micro: 100_000,
                roughness_end_micro: 700_000,
            }),
            composition: SceneSsrComposition::ReplaceEnvironment,
            max_distance_millimeters: 25_000,
            thickness_slope_micro: 20_000,
            origin_skip_pixels: 0,
            pass_behind_steps: 8,
            roughness_fade_start_micro: Some(100_000),
            luminance_limit_milli: Some(4000),
            ..defaults
        };
        assert!(authored.validate().is_ok());
        let bytes = serde_json::to_vec(&authored).unwrap();
        assert_eq!(
            serde_json::from_slice::<SceneSsr>(&bytes).unwrap(),
            authored
        );
    }
}
