use super::{SceneValidationError, SceneWaterSettings, effects::require_range};

/// Reflected environment controls, separate from the analytic specular amplitude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterEnvironment {
    /// Linear environment amplitude, in millionths (`0..=10_000_000`).
    pub amplitude_micro: u32,
    /// Simplex frequency per metre, in millionths; zero disables edge noise.
    pub edge_frequency_micro: u32,
    /// Attenuate reflected radiance at shallow depths by `clamp(depth * 20, 0, 1)`.
    pub depth_self_occlusion: bool,
}

/// Analytic solar highlight controls, applied before the material's squared gain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterSunDisk {
    /// Highlight irradiance multiplier, in millionths (`0..=10_000_000`).
    pub amplitude_micro: u32,
    /// Linear luminance ceiling, in millionths; zero suppresses the highlight.
    pub peak_micro: u32,
}

/// Planar seed coordinates independent of the normal map's world-space repetition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterFlowCoordinates {
    pub origin_millimeters: [i32; 2],
    /// Positive repetition lengths along east and north.
    pub tile_millimeters: [u32; 2],
}

/// Vertex-sampled planar scene wind, with a spatial and temporal perturbation band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneWaterWind {
    pub field: super::ScenePlanarWindField,
    pub band: super::SceneWindBand,
    /// Blend from unchanged authored flow to wind-shaped seeds, in millionths.
    pub amplitude_micro: u32,
}

pub(super) fn validate(settings: &SceneWaterSettings) -> Result<(), SceneValidationError> {
    if let Some(environment) = settings.environment {
        require_range(
            "water environment amplitude",
            u64::from(environment.amplitude_micro),
            0,
            10_000_000,
        )?;
        require_range(
            "water edge frequency",
            u64::from(environment.edge_frequency_micro),
            0,
            100_000_000,
        )?;
    }
    if let Some(disk) = settings.sun_disk {
        require_range(
            "water sun amplitude",
            u64::from(disk.amplitude_micro),
            0,
            10_000_000,
        )?;
    }
    if let Some(coordinates) = settings.flow_coordinates
        && (coordinates.tile_millimeters.contains(&0) || settings.flow.is_none())
    {
        return Err(SceneValidationError::UnsupportedInput {
            part: "water flow coordinates require positive tiles and a flow map",
        });
    }
    if let Some(wind) = settings.wind {
        require_range(
            "water wind amplitude",
            u64::from(wind.amplitude_micro),
            0,
            1_000_000,
        )?;
        if settings.flow.is_none() || settings.flow_period_milliseconds.is_none() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water wind requires flow seeds and a two-phase period",
            });
        }
        if wind.field.modifiers().iter().any(Option::is_some) {
            return Err(SceneValidationError::UnsupportedInput {
                part: "water wind does not apply foliage force modifiers",
            });
        }
    }
    Ok(())
}
