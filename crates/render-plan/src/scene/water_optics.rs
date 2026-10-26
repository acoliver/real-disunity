//! Optional optical and depth-shaped surface terms.
use super::{SceneValidationError, effects::require_range};

/// Linear optical controls. Body colour and absorption remain the medium terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterOptics {
    /// Normal-incidence reflectance, in millionths.
    pub fresnel_micro: u32,
    /// Specular amplitude, squared when applied, in millionths.
    pub specular_micro: u32,
    /// Additional view-dependent scattering radiance, in millionths.
    pub scattering_micro: [u32; 3],
    /// Optional two-depth colour ramps, evaluated before shared surface lighting.
    pub tint: Option<SceneWaterTint>,
}

/// Two base-100 depth ramps blending gamma-authored colours and refracted radiance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterTint {
    /// Deep and shallow gamma-encoded RGB endpoints, in millionths.
    pub colors_micro: [[u32; 3]; 2],
    /// Base-100 decay coefficients per metre for radiance mixing and endpoint mixing.
    pub decay_micro: [u32; 2],
    /// Signed offsets applied before and after the inner radiance-ramp clamp.
    pub offsets_micro: [i32; 2],
}

/// Depth below the surface drives flow, normal shape and surface opacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterShoreline {
    /// Depth normalization length in millimetres; must be positive.
    pub depth_millimeters: u32,
    /// Signed directional gain after absolute normal shaping, in millionths.
    pub normal_gain_micro: i32,
    /// Maximum surface opacity, in millionths.
    pub opacity_micro: u32,
}

pub(super) fn validate(
    optics: Option<SceneWaterOptics>,
    shoreline: Option<SceneWaterShoreline>,
) -> Result<(), SceneValidationError> {
    if let Some(optics) = optics {
        require_range(
            "water fresnel",
            u64::from(optics.fresnel_micro),
            0,
            1_000_000,
        )?;
        require_range(
            "water specular",
            u64::from(optics.specular_micro),
            0,
            10_000_000,
        )?;
        if let Some(tint) = optics.tint {
            for value in tint.colors_micro.into_iter().flatten() {
                require_range("water tint color", u64::from(value), 0, 1_000_000)?;
            }
            for value in tint.decay_micro {
                require_range("water tint decay", u64::from(value), 0, 100_000_000)?;
            }
            for value in tint.offsets_micro {
                require_range(
                    "water tint offset",
                    u64::from(value.unsigned_abs()),
                    0,
                    1_000_000,
                )?;
            }
        }
        for value in optics.scattering_micro {
            require_range("water scattering", u64::from(value), 0, 10_000_000)?;
        }
    }
    if let Some(shoreline) = shoreline {
        require_range(
            "water shoreline depth",
            u64::from(shoreline.depth_millimeters),
            1,
            u64::from(u32::MAX),
        )?;
        require_range(
            "water shoreline normal gain",
            u64::from(shoreline.normal_gain_micro.unsigned_abs()),
            0,
            10_000_000,
        )?;
        require_range(
            "water shoreline opacity",
            u64::from(shoreline.opacity_micro),
            0,
            1_000_000,
        )?;
    }
    Ok(())
}
