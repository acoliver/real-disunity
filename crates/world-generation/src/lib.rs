use thiserror::Error;
use uuid::Uuid;
use worldrunner_asset_id::LogicalAssetId;
use worldrunner_core_types::{
    AssetCatalogVersion, WorldDimensions, WorldId, WorldPosition, WorldSeed, WorldStateHash,
};
use worldrunner_world_schema::{
    BiomeDefinition, BiomeRegion, CURRENT_GENERATOR_VERSION, CURRENT_WORLD_SCHEMA, CatalogBinding,
    EnvironmentInstance, EnvironmentPlacement, GenerationParameters, TerrainDocument,
    TerrainMaterialSlot, WorldCoordinateFrame, WorldDocument, WorldValidationError,
    canonical_world_hash,
};

mod profile;
mod request;
mod rotation;

pub use request::GenerationPreset;

pub use profile::{
    BiomeChoice, BiomeProfile, CatalogProfile, GenerationProfile, MaterialChoice, PlacementProfile,
    ProfileAssetError, ProfileDigest, PropFamily, SurfaceProfile,
};

// Random streams remain at revision 1 when only the output rotation algorithm changes.
const GENERATION_STREAM_VERSION: u32 = 1;
const FIXED_ONE: i64 = 65_536;
const ENVIRONMENT_LIMIT: u64 = 100_000;
const BIOME_GRID: u32 = 8;

#[derive(Clone, Debug)]
pub struct WorldGenerationRequest {
    pub seed: WorldSeed,
    pub dimensions: WorldDimensions,
    pub terrain_samples: [u32; 2],
    pub parameters: GenerationParameters,
    pub asset_catalog_version: AssetCatalogVersion,
    /// Caller-supplied content profile; there is no compiled-in recipe.
    pub profile: GenerationProfile,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedWorld {
    pub document: WorldDocument,
    pub canonical_hash: WorldStateHash,
    /// Authored profile name recorded alongside the generator version.
    pub profile_name: String,
    /// Canonical profile digest recorded alongside the generator version.
    pub profile_digest: ProfileDigest,
}

#[derive(Debug, Error)]
pub enum GenerationError {
    #[error("world dimensions must be non-zero")]
    EmptyDimensions,
    #[error("world dimensions exceed millimetre or centered coordinate limits")]
    DimensionsTooLarge,
    #[error("terrain samples must be at least two on each axis")]
    TerrainTooSmall,
    #[error("terrain sample allocation exceeds host limits")]
    TerrainTooLarge,
    #[error("world generation parameters are invalid")]
    InvalidParameters,
    #[error("elevation amplitude exceeds the terrain height representation")]
    ElevationAmplitudeTooLarge,
    #[error("environment candidate count arithmetic exceeds u64 limits")]
    EnvironmentCandidateCountOverflow,
    #[error("generation profile is invalid: {0}")]
    InvalidProfile(String),
    #[error("generated world failed validation: {0}")]
    InvalidWorld(#[from] WorldValidationError),
}

pub fn generate_world(request: &WorldGenerationRequest) -> Result<WorldDocument, GenerationError> {
    Ok(generate_world_with_hash(request)?.document)
}

pub fn generate_world_with_hash(
    request: &WorldGenerationRequest,
) -> Result<GeneratedWorld, GenerationError> {
    validate_request(request)?;
    request
        .profile
        .validate()
        .map_err(GenerationError::InvalidProfile)?;
    let [width, depth] = request.terrain_samples;
    let sample_count = sample_count(width, depth)?;
    let spacing = terrain_spacing(request, width, depth);

    let heights = generate_macro_elevation(request, width, depth, sample_count);
    let climate = generate_climate(request, width, depth, &heights);
    let material_indices = classify_surfaces(request, width, depth, spacing, &heights, &climate);
    let biome_definitions = profile_biome_definitions(&request.profile);
    let biome_regions = generate_biome_regions(request, width, depth, &heights, &climate);
    let environment_instances =
        place_environment(request, width, depth, spacing, &heights, &material_indices)?;

    let document = WorldDocument {
        schema_version: CURRENT_WORLD_SCHEMA,
        generator_version: CURRENT_GENERATOR_VERSION,
        coordinate_frame: WorldCoordinateFrame::RightHandedYUp,
        world_id: deterministic_world_id(request),
        seed: request.seed,
        dimensions: request.dimensions,
        generation_parameters: request.parameters.clone(),
        terrain: TerrainDocument {
            width_samples: width,
            depth_samples: depth,
            sample_spacing_millimeters: spacing,
            heights_millimeters: heights,
            material_palette: profile_material_palette(&request.profile),
            material_indices,
        },
        biome_definitions,
        biome_regions,
        environment_instances,
        catalog: CatalogBinding {
            version: request.asset_catalog_version,
            preferred_art_sets: request.profile.catalog.preferred_art_sets.clone(),
            preferred_platform_profiles: request
                .profile
                .catalog
                .preferred_platform_profiles
                .clone(),
            maximum_quality: request.profile.catalog.maximum_quality,
        },
        authored_overrides: Vec::new(),
    };
    let canonical_hash = canonical_world_hash(&document)?;
    Ok(GeneratedWorld {
        document,
        canonical_hash,
        profile_name: request.profile.name.clone(),
        profile_digest: request.profile.digest(),
    })
}

fn validate_request(request: &WorldGenerationRequest) -> Result<(), GenerationError> {
    request::validate_inputs(
        request.dimensions,
        request.terrain_samples,
        &request.parameters,
    )
}

fn sample_count(width: u32, depth: u32) -> Result<usize, GenerationError> {
    usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(depth)
                .ok()
                .and_then(|depth| width.checked_mul(depth))
                .filter(|count| *count <= isize::MAX as usize / size_of::<i32>())
        })
        .ok_or(GenerationError::TerrainTooLarge)
}

fn terrain_spacing(request: &WorldGenerationRequest, width: u32, depth: u32) -> u32 {
    let east_spacing = request
        .dimensions
        .width_meters
        .saturating_mul(1_000)
        .checked_div(width - 1)
        .unwrap_or(1);
    let north_spacing = request
        .dimensions
        .depth_meters
        .saturating_mul(1_000)
        .checked_div(depth - 1)
        .unwrap_or(1);
    east_spacing.min(north_spacing).max(1)
}

fn generate_macro_elevation(
    request: &WorldGenerationRequest,
    width: u32,
    depth: u32,
    sample_count: usize,
) -> Vec<i32> {
    let frequency = request.parameters.terrain_frequency_microunits;
    let base_period = (1_000_000 / frequency).clamp(16, 512);
    let amplitude = i64::from(request.parameters.elevation_amplitude_millimeters);
    let mut heights = Vec::with_capacity(sample_count);
    for north in 0..depth {
        for east in 0..width {
            let mut value = 0_i64;
            let mut weight = 0_i64;
            for octave in 0..4 {
                let period = (base_period >> octave).max(4);
                let octave_weight = 8_i64 >> octave;
                value += value_noise(request.seed.0, "elevation", east, north, period, octave)
                    * octave_weight;
                weight += octave_weight;
            }
            let normalized = value / weight + i64::from(i32::MAX) / 3;
            let ridge_noise = value_noise(
                request.seed.0,
                "elevation-ridge",
                east,
                north,
                (base_period * 2).min(1_024),
                0,
            )
            .abs();
            let ridge = ridge_noise.saturating_mul(ridge_noise) / i64::from(i32::MAX);
            let ridge = ridge.saturating_mul(i64::from(i32::MAX) / 2) / i64::from(i32::MAX);
            let shaped =
                apply_island_falloff(normalized.saturating_add(ridge), east, north, width, depth);
            let height = shaped * amplitude / i64::from(i32::MAX);
            heights.push(clamp_i32(height));
        }
    }
    heights
}

fn generate_climate(
    request: &WorldGenerationRequest,
    width: u32,
    depth: u32,
    heights: &[i32],
) -> Vec<ClimateSample> {
    let mut climate = Vec::with_capacity(heights.len());
    for north in 0..depth {
        let latitude = i64::from(north) * i64::from(u16::MAX) / i64::from(depth - 1);
        let equator_heat = i64::from(u16::MAX) - (latitude * 2 - i64::from(u16::MAX)).abs();
        for east in 0..width {
            let index = index(width, east, north);
            let moisture_noise = hash_u64(request.seed.0, "climate-moisture", &[east, north]);
            let temperature_noise = hash_u64(request.seed.0, "climate-temperature", &[east, north]);
            let moisture = (moisture_noise >> 48) as u16;
            let variation = i64::from((temperature_noise >> 52) as u16) - 2_048;
            let elevation_penalty = i64::from(heights[index].max(0)) / 4;
            let temperature =
                (equator_heat + variation - elevation_penalty).clamp(0, i64::from(u16::MAX));
            climate.push(ClimateSample {
                moisture,
                temperature: u16::try_from(temperature).expect("temperature is clamped to u16"),
            });
        }
    }
    climate
}

fn classify_surfaces(
    request: &WorldGenerationRequest,
    width: u32,
    depth: u32,
    spacing: u32,
    heights: &[i32],
    climate: &[ClimateSample],
) -> Vec<u16> {
    let sea_level = request.parameters.sea_level_millimeters;
    let amplitude =
        i32::try_from(request.parameters.elevation_amplitude_millimeters).unwrap_or(i32::MAX);
    let mut materials = Vec::with_capacity(heights.len());
    for north in 0..depth {
        for east in 0..width {
            let sample_index = index(width, east, north);
            let height = heights[sample_index];
            let east_neighbor = heights[index(width, (east + 1).min(width - 1), north)];
            let north_neighbor = heights[index(width, east, (north + 1).min(depth - 1))];
            let slope = height
                .abs_diff(east_neighbor)
                .max(height.abs_diff(north_neighbor));
            let dry = climate[sample_index].moisture < request.profile.surfaces.dry_moisture_max;
            let material = if height < sea_level {
                0
            } else if height > amplitude / 2 || slope > spacing.saturating_mul(2) || dry {
                2
            } else {
                1
            };
            materials.push(material);
        }
    }
    materials
}

fn generate_biome_regions(
    request: &WorldGenerationRequest,
    width: u32,
    depth: u32,
    heights: &[i32],
    climate: &[ClimateSample],
) -> Vec<BiomeRegion> {
    let world_width_mm = u64::from(request.dimensions.width_meters) * 1_000;
    let world_depth_mm = u64::from(request.dimensions.depth_meters) * 1_000;
    let radius = (world_width_mm.min(world_depth_mm) / u64::from(BIOME_GRID)).max(1);
    let mut regions = Vec::with_capacity(usize::try_from(BIOME_GRID * BIOME_GRID).unwrap_or(64));
    for north_cell in 0..BIOME_GRID {
        for east_cell in 0..BIOME_GRID {
            let east_sample = east_cell * (width - 1) / (BIOME_GRID - 1);
            let north_sample = north_cell * (depth - 1) / (BIOME_GRID - 1);
            let sample = index(width, east_sample, north_sample);
            let biomes = &request.profile.biomes;
            let coastal_max_height = i64::from(request.parameters.sea_level_millimeters)
                + biomes.coastal_max_height_above_sea_millimeters;
            let biome_definition_id = if i64::from(heights[sample]) < coastal_max_height {
                biomes.coastal.definition_id
            } else if i64::from(climate[sample].temperature)
                < i64::from(biomes.highland_max_temperature)
                || i64::from(heights[sample]) > biomes.highland_min_height_millimeters
            {
                biomes.highland.definition_id
            } else {
                biomes.temperate.definition_id
            };
            let cell_index = north_cell * BIOME_GRID + east_cell;
            regions.push(BiomeRegion {
                region_id: cell_index + 1,
                biome_definition_id,
                center_east_millimeters: centered_millimeters(
                    east_cell,
                    BIOME_GRID,
                    world_width_mm,
                ),
                center_north_millimeters: centered_millimeters(
                    north_cell,
                    BIOME_GRID,
                    world_depth_mm,
                ),
                radius_millimeters: radius,
            });
        }
    }
    regions
}

fn place_environment(
    request: &WorldGenerationRequest,
    width: u32,
    depth: u32,
    spacing: u32,
    heights: &[i32],
    materials: &[u16],
) -> Result<Vec<EnvironmentInstance>, GenerationError> {
    let candidates = environment_candidate_count(request)?;
    let world_width_mm = u64::from(request.dimensions.width_meters) * 1_000;
    let world_depth_mm = u64::from(request.dimensions.depth_meters) * 1_000;
    let mut instances = Vec::new();
    for candidate in 0..candidates {
        let east_mm = hash_u64(
            request.seed.0,
            "environment-east",
            &[low_u32(candidate), high_u32(candidate)],
        ) % world_width_mm;
        let north_mm = hash_u64(
            request.seed.0,
            "environment-north",
            &[low_u32(candidate), high_u32(candidate)],
        ) % world_depth_mm;
        let east_sample = u32::try_from((east_mm / u64::from(spacing)).min(u64::from(width - 1)))
            .expect("sample coordinate is bounded by u32 terrain width");
        let north_sample = u32::try_from((north_mm / u64::from(spacing)).min(u64::from(depth - 1)))
            .expect("sample coordinate is bounded by u32 terrain depth");
        let sample = index(width, east_sample, north_sample);
        if materials[sample] == 0 {
            continue;
        }
        let random = hash_u64(
            request.seed.0,
            "environment-kind",
            &[low_u32(candidate), high_u32(candidate)],
        );
        let rock_terrain = materials[sample] == 2;
        let Some(choice) = choose_prop(random, rock_terrain, &request.profile.placement) else {
            continue;
        };
        let instance_id = u64::try_from(instances.len())
            .map_err(|_| GenerationError::TerrainTooLarge)?
            .checked_add(1)
            .ok_or(GenerationError::TerrainTooLarge)?;
        instances.push(EnvironmentInstance {
            instance_id,
            asset_id: logical_id(&choice.asset),
            position: WorldPosition {
                east_meters: millimeters_to_centered_meters(east_mm, world_width_mm),
                elevation_meters: f64::from(heights[sample]) / 1_000.0,
                north_meters: millimeters_to_centered_meters(north_mm, world_depth_mm),
            },
            orientation: rotation::from_draw(random),
            placement: EnvironmentPlacement::BoundsGrounded,
            uniform_scale_millionths: choice.scale,
            visible: true,
        });
    }
    Ok(instances)
}

fn environment_candidate_count(request: &WorldGenerationRequest) -> Result<u64, GenerationError> {
    request::candidate_count(
        request.dimensions,
        request.parameters.environment_density_per_square_kilometer,
    )
}

/// The asset and scale selected for one candidate site.
struct PropChoice {
    asset: String,
    scale: u32,
}

/// Derives the prop for a candidate from its hash draw and the authored placement
/// profile, or `None` when the candidate is skipped. Every bit slice of `random`
/// keeps its original meaning so generated worlds stay deterministic per seed.
fn choose_prop(
    random: u64,
    rock_terrain: bool,
    placement: &PlacementProfile,
) -> Option<PropChoice> {
    let rock_denominator = if rock_terrain {
        placement.rock_denominator_rocky_terrain
    } else {
        placement.rock_denominator_other_terrain
    };
    let rock = random.is_multiple_of(rock_denominator);
    let grass = !rock
        && if rock_terrain {
            (random >> 8).is_multiple_of(placement.grass_denominator_rocky_terrain)
        } else {
            true
        };
    if !rock && !grass {
        return None;
    }
    let tree = !rock && !rock_terrain && (random >> 16).is_multiple_of(placement.tree_denominator);
    let family = if rock {
        if (random >> 24).is_multiple_of(placement.rock_split_denominator) {
            &placement.rock_primary
        } else {
            &placement.rock_secondary
        }
    } else if tree {
        &placement.tree
    } else {
        &placement.grass
    };
    let scale = family.base_scale_millionths
        + u32::try_from((random >> 32) % u64::from(family.scale_jitter_modulus))
            .expect("scale jitter draw is bounded by the modulus");
    Some(PropChoice {
        asset: family.asset_id.clone(),
        scale,
    })
}

fn deterministic_world_id(request: &WorldGenerationRequest) -> WorldId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"REAL-DISUNITY-WORLD-ID-V2");
    hasher.update(&request.seed.0.to_le_bytes());
    hasher.update(&request.dimensions.width_meters.to_le_bytes());
    hasher.update(&request.dimensions.depth_meters.to_le_bytes());
    hasher.update(&request.terrain_samples[0].to_le_bytes());
    hasher.update(&request.terrain_samples[1].to_le_bytes());
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    WorldId::from_uuid(Uuid::from_bytes(bytes))
}

fn value_noise(seed: u64, domain: &str, east: u32, north: u32, period: u32, octave: u32) -> i64 {
    let east_cell = east / period;
    let north_cell = north / period;
    let east_fraction = smooth_fraction(east % period, period);
    let north_fraction = smooth_fraction(north % period, period);
    let southwest = signed_lattice(seed, domain, east_cell, north_cell, octave);
    let southeast = signed_lattice(seed, domain, east_cell + 1, north_cell, octave);
    let northwest = signed_lattice(seed, domain, east_cell, north_cell + 1, octave);
    let northeast = signed_lattice(seed, domain, east_cell + 1, north_cell + 1, octave);
    let south = lerp_fixed(southwest, southeast, east_fraction);
    let north = lerp_fixed(northwest, northeast, east_fraction);
    lerp_fixed(south, north, north_fraction)
}

fn signed_lattice(seed: u64, domain: &str, east: u32, north: u32, octave: u32) -> i64 {
    let value = hash_u64(seed, domain, &[east, north, octave]);
    i64::from((value >> 32) as u32) - i64::from(u32::MAX / 2)
}

fn smooth_fraction(remainder: u32, period: u32) -> i64 {
    let fraction = i64::from(remainder) * FIXED_ONE / i64::from(period);
    fraction * fraction / FIXED_ONE * (3 * FIXED_ONE - 2 * fraction) / FIXED_ONE
}

fn lerp_fixed(start: i64, end: i64, fraction: i64) -> i64 {
    start + (end - start) * fraction / FIXED_ONE
}

fn apply_island_falloff(value: i64, east: u32, north: u32, width: u32, depth: u32) -> i64 {
    let east_distance =
        (i64::from(east) * 2 - i64::from(width - 1)).abs() * FIXED_ONE / i64::from(width - 1);
    let north_distance =
        (i64::from(north) * 2 - i64::from(depth - 1)).abs() * FIXED_ONE / i64::from(depth - 1);
    let edge = east_distance.max(north_distance);
    let threshold = FIXED_ONE * 3 / 5;
    if edge <= threshold {
        value
    } else {
        let falloff = (edge - threshold) * i64::from(i32::MAX) / (FIXED_ONE - threshold);
        value.saturating_sub(falloff)
    }
}

fn hash_u64(seed: u64, domain: &str, coordinates: &[u32]) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"REAL-DISUNITY-GENERATION-STREAM-V2");
    hasher.update(&GENERATION_STREAM_VERSION.to_le_bytes());
    hasher.update(&seed.to_le_bytes());
    hasher.update(domain.as_bytes());
    for coordinate in coordinates {
        hasher.update(&coordinate.to_le_bytes());
    }
    u64::from_le_bytes(
        hasher.finalize().as_bytes()[..8]
            .try_into()
            .expect("BLAKE3 output has eight bytes"),
    )
}

fn profile_biome_definitions(profile: &GenerationProfile) -> Vec<BiomeDefinition> {
    let biomes = &profile.biomes;
    [&biomes.coastal, &biomes.temperate, &biomes.highland]
        .into_iter()
        .map(|choice| BiomeDefinition {
            definition_id: choice.definition_id,
            biome_asset_id: logical_id(&choice.biome_asset_id),
        })
        .collect()
}

fn profile_material_palette(profile: &GenerationProfile) -> Vec<TerrainMaterialSlot> {
    profile
        .surfaces
        .materials
        .iter()
        .map(|material| {
            terrain_material(&material.material_asset_id, material.uv_scale_millimeters)
        })
        .collect()
}

fn terrain_material(value: &str, uv_scale_millimeters: u32) -> TerrainMaterialSlot {
    TerrainMaterialSlot {
        material_id: logical_id(value),
        uv_scale_millimeters,
    }
}

fn logical_id(value: &str) -> LogicalAssetId {
    LogicalAssetId::new(value).expect("built-in logical asset IDs are valid")
}

fn centered_millimeters(cell: u32, cells: u32, extent: u64) -> i64 {
    let numerator = u64::from(cell)
        .saturating_mul(2)
        .saturating_add(1)
        .saturating_mul(extent);
    let coordinate = numerator / (u64::from(cells) * 2);
    i64::try_from(coordinate).unwrap_or(i64::MAX) - i64::try_from(extent / 2).unwrap_or(i64::MAX)
}

fn millimeters_to_centered_meters(value: u64, extent: u64) -> f64 {
    let fallback = if value < extent / 2 {
        i32::MIN
    } else {
        i32::MAX
    };
    let centered = i32::try_from(
        i64::try_from(value).unwrap_or(i64::MAX) - i64::try_from(extent / 2).unwrap_or(i64::MAX),
    )
    .unwrap_or(fallback);
    f64::from(centered) / 1_000.0
}

fn index(width: u32, east: u32, north: u32) -> usize {
    usize::try_from(u64::from(north) * u64::from(width) + u64::from(east))
        .expect("validated terrain sample index fits in usize")
}

fn clamp_i32(value: i64) -> i32 {
    i32::try_from(value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)))
        .expect("value was clamped to i32")
}

fn low_u32(value: u64) -> u32 {
    u32::from_le_bytes(
        value.to_le_bytes()[..4]
            .try_into()
            .expect("u64 has four low bytes"),
    )
}

fn high_u32(value: u64) -> u32 {
    u32::from_le_bytes(
        value.to_le_bytes()[4..]
            .try_into()
            .expect("u64 has four high bytes"),
    )
}

#[derive(Clone, Copy)]
struct ClimateSample {
    moisture: u16,
    temperature: u16,
}

#[cfg(test)]
#[path = "generation_tests.rs"]
mod tests;

#[cfg(test)]
mod request_tests;
