use serde::{Deserialize, Serialize};
use worldrunner_world_schema::GenerationParameters;

use crate::{GenerationError, GenerationProfile, WorldGenerationRequest, validate_request};
use worldrunner_core_types::{AssetCatalogVersion, WorldDimensions, WorldSeed};

const HEIGHT_DIVISOR: i64 = i32::MAX as i64;
const LATTICE_MAX: i64 = u32::MAX as i64 - (u32::MAX / 2) as i64;
const MAX_RIDGE: i64 =
    LATTICE_MAX * LATTICE_MAX / HEIGHT_DIVISOR * (HEIGHT_DIVISOR / 2) / HEIGHT_DIVISOR;
// Interpolation and the weighted mean stay within the signed lattice range.
// The center has no falloff; its positive extreme dominates the negative edge.
const MAX_SHAPED: i64 = LATTICE_MAX + HEIGHT_DIVISOR / 3 + MAX_RIDGE;
// Integer division may discard a remainder, but the resulting height must fit i32.
const MAX_ELEVATION_AMPLITUDE: i64 = ((HEIGHT_DIVISOR + 1) * HEIGHT_DIVISOR - 1) / MAX_SHAPED;

/// Complete named generation inputs supplied by a consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationPreset {
    pub name: String,
    pub seed: u64,
    pub dimensions_meters: [u32; 2],
    pub terrain_samples: [u32; 2],
    pub parameters: GenerationParameters,
    pub asset_catalog_version: u32,
}

impl WorldGenerationRequest {
    /// Resolves a named request after validating the complete profile.
    pub fn from_profile(profile: GenerationProfile, name: &str) -> Result<Self, GenerationError> {
        profile
            .validate()
            .map_err(GenerationError::InvalidProfile)?;
        let preset = profile
            .requests
            .iter()
            .find(|preset| preset.name == name)
            .ok_or_else(|| {
                GenerationError::InvalidProfile(format!("missing generation request {name}"))
            })?;
        let request = Self {
            seed: WorldSeed(preset.seed),
            dimensions: WorldDimensions::new(
                preset.dimensions_meters[0],
                preset.dimensions_meters[1],
            ),
            terrain_samples: preset.terrain_samples,
            parameters: preset.parameters.clone(),
            asset_catalog_version: AssetCatalogVersion(preset.asset_catalog_version),
            profile,
        };
        validate_request(&request)?;
        Ok(request)
    }
}

pub(crate) fn validate_presets(presets: &[GenerationPreset]) -> Result<(), String> {
    if presets.is_empty() {
        return Err("generation profile requires at least one request".to_owned());
    }
    for (index, preset) in presets.iter().enumerate() {
        if preset.name.trim().is_empty() {
            return Err("generation request name must not be empty".to_owned());
        }
        if presets[..index]
            .iter()
            .any(|prior| prior.name == preset.name)
        {
            return Err(format!("duplicate generation request {}", preset.name));
        }
        validate_inputs(
            WorldDimensions::new(preset.dimensions_meters[0], preset.dimensions_meters[1]),
            preset.terrain_samples,
            &preset.parameters,
        )
        .map_err(|error| format!("generation request {}: {error}", preset.name))?;
    }
    Ok(())
}

pub(crate) fn validate_inputs(
    dimensions: WorldDimensions,
    samples: [u32; 2],
    parameters: &GenerationParameters,
) -> Result<(), GenerationError> {
    if dimensions.is_empty() {
        return Err(GenerationError::EmptyDimensions);
    }
    for meters in [dimensions.width_meters, dimensions.depth_meters] {
        let extent = meters
            .checked_mul(1_000)
            .ok_or(GenerationError::DimensionsTooLarge)?;
        let half = i64::from(extent / 2);
        i32::try_from(-half).map_err(|_| GenerationError::DimensionsTooLarge)?;
        i32::try_from(i64::from(extent - 1) - half)
            .map_err(|_| GenerationError::DimensionsTooLarge)?;
    }
    let [width, depth] = samples;
    if width < 2 || depth < 2 {
        return Err(GenerationError::TerrainTooSmall);
    }
    crate::sample_count(width, depth)?;
    if parameters.elevation_amplitude_millimeters == 0
        || parameters.terrain_frequency_microunits == 0
    {
        return Err(GenerationError::InvalidParameters);
    }
    if i64::from(parameters.elevation_amplitude_millimeters) > MAX_ELEVATION_AMPLITUDE {
        return Err(GenerationError::ElevationAmplitudeTooLarge);
    }
    candidate_count(
        dimensions,
        parameters.environment_density_per_square_kilometer,
    )?;
    Ok(())
}

pub(crate) fn candidate_count(
    dimensions: WorldDimensions,
    density: u32,
) -> Result<u64, GenerationError> {
    let area_square_meters = u64::from(dimensions.width_meters)
        .checked_mul(u64::from(dimensions.depth_meters))
        .ok_or(GenerationError::EnvironmentCandidateCountOverflow)?;
    let candidates = area_square_meters
        .checked_mul(u64::from(density))
        .ok_or(GenerationError::EnvironmentCandidateCountOverflow)?
        / 1_000_000;
    Ok(candidates.min(crate::ENVIRONMENT_LIMIT))
}
