use crate::{
    CURRENT_WORLD_SCHEMA, EnvironmentOrientation, EnvironmentPlacement, WorldCoordinateFrame,
    WorldValidationError,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use worldrunner_asset_id::{LogicalAssetId, QualityTier};
use worldrunner_core_types::{
    AssetCatalogVersion, GenerationVersion, SchemaVersion, WorldDimensions, WorldId, WorldPosition,
    WorldSeed,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldDocument {
    pub schema_version: SchemaVersion,
    pub generator_version: GenerationVersion,
    pub coordinate_frame: WorldCoordinateFrame,
    pub world_id: WorldId,
    pub seed: WorldSeed,
    pub dimensions: WorldDimensions,
    pub generation_parameters: GenerationParameters,
    pub terrain: TerrainDocument,
    pub biome_definitions: Vec<BiomeDefinition>,
    pub biome_regions: Vec<BiomeRegion>,
    pub environment_instances: Vec<EnvironmentInstance>,
    pub catalog: CatalogBinding,
    pub authored_overrides: Vec<AuthoredOverride>,
}

impl WorldDocument {
    pub fn validate(&self) -> Result<(), WorldValidationError> {
        if self.schema_version != CURRENT_WORLD_SCHEMA {
            return Err(WorldValidationError::UnsupportedSchema(
                self.schema_version.0,
            ));
        }
        if self.dimensions.is_empty() {
            return Err(WorldValidationError::EmptyDimensions);
        }
        validate_terrain(&self.terrain)?;
        validate_catalog(&self.catalog)?;
        self.validate_biomes()?;
        self.validate_instances()?;
        ensure_strictly_ordered(
            self.authored_overrides.iter().map(|item| item.override_id),
            WorldValidationError::OverrideOrder,
        )?;
        for authored in &self.authored_overrides {
            validate_override(authored, &self.terrain, &self.environment_instances)?;
        }
        Ok(())
    }

    fn validate_biomes(&self) -> Result<(), WorldValidationError> {
        let mut biome_ids = BTreeSet::new();
        for biome in &self.biome_definitions {
            if !biome_ids.insert(biome.definition_id) {
                return Err(WorldValidationError::DuplicateBiomeDefinition(
                    biome.definition_id,
                ));
            }
        }
        let mut region_ids = BTreeSet::new();
        for region in &self.biome_regions {
            if !region_ids.insert(region.region_id) {
                return Err(WorldValidationError::DuplicateBiomeRegion(region.region_id));
            }
            if !biome_ids.contains(&region.biome_definition_id) {
                return Err(WorldValidationError::UnknownBiomeDefinition(
                    region.biome_definition_id,
                ));
            }
            if region.radius_millimeters == 0 {
                return Err(WorldValidationError::EmptyBiomeRegion(region.region_id));
            }
        }
        Ok(())
    }

    fn validate_instances(&self) -> Result<(), WorldValidationError> {
        ensure_strictly_ordered(
            self.environment_instances
                .iter()
                .map(|instance| instance.instance_id),
            WorldValidationError::EnvironmentOrder,
        )?;
        for instance in &self.environment_instances {
            if !position_is_finite(instance.position)
                || instance.uniform_scale_millionths == 0
                || instance.orientation.validate().is_err()
            {
                return Err(WorldValidationError::InvalidEnvironmentInstance(
                    instance.instance_id,
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationParameters {
    pub sea_level_millimeters: i32,
    pub elevation_amplitude_millimeters: u32,
    pub terrain_frequency_microunits: u32,
    pub environment_density_per_square_kilometer: u32,
}

impl Default for GenerationParameters {
    fn default() -> Self {
        Self {
            sea_level_millimeters: 0,
            elevation_amplitude_millimeters: 80_000,
            terrain_frequency_microunits: 4_000,
            environment_density_per_square_kilometer: 600,
        }
    }
}

/// Embedded heightfield centred at the world origin. Samples are row-major:
/// columns increase east (+X), rows increase north (-Z), heights increase up (+Y).
/// Spacing and heights use explicit millimetres; material indices address the palette.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainDocument {
    pub width_samples: u32,
    pub depth_samples: u32,
    pub sample_spacing_millimeters: u32,
    pub heights_millimeters: Vec<i32>,
    pub material_palette: Vec<TerrainMaterialSlot>,
    pub material_indices: Vec<u16>,
}

impl TerrainDocument {
    pub fn sample_height_meters(&self, east_meters: f64, north_meters: f64) -> Option<f64> {
        if !east_meters.is_finite()
            || !north_meters.is_finite()
            || self.width_samples == 0
            || self.depth_samples == 0
            || self.sample_spacing_millimeters == 0
        {
            return None;
        }
        let width = usize::try_from(self.width_samples).ok()?;
        let depth = usize::try_from(self.depth_samples).ok()?;
        if self.heights_millimeters.len() != width.checked_mul(depth)? {
            return None;
        }

        let spacing = f64::from(self.sample_spacing_millimeters) / 1_000.0;
        let half_width = f64::from(self.width_samples - 1) * spacing * 0.5;
        let half_depth = f64::from(self.depth_samples - 1) * spacing * 0.5;
        let east_sample =
            ((east_meters + half_width) / spacing).clamp(0.0, f64::from(self.width_samples - 1));
        let north_sample =
            ((north_meters + half_depth) / spacing).clamp(0.0, f64::from(self.depth_samples - 1));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let east_low = east_sample.floor() as usize;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let north_low = north_sample.floor() as usize;
        let east_high = (east_low + 1).min(width - 1);
        let north_high = (north_low + 1).min(depth - 1);
        let east_factor = east_sample.fract();
        let north_factor = north_sample.fract();
        let height = |east: usize, north: usize| {
            f64::from(self.heights_millimeters[north * width + east]) / 1_000.0
        };
        let near = height(east_low, north_low)
            + (height(east_high, north_low) - height(east_low, north_low)) * east_factor;
        let far = height(east_low, north_high)
            + (height(east_high, north_high) - height(east_low, north_high)) * east_factor;
        Some(near + (far - near) * north_factor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainMaterialSlot {
    pub material_id: LogicalAssetId,
    pub uv_scale_millimeters: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeDefinition {
    pub definition_id: u32,
    pub biome_asset_id: LogicalAssetId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BiomeRegion {
    pub region_id: u32,
    pub biome_definition_id: u32,
    pub center_east_millimeters: i64,
    pub center_north_millimeters: i64,
    pub radius_millimeters: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentInstance {
    pub instance_id: u64,
    pub asset_id: LogicalAssetId,
    pub position: WorldPosition,
    pub orientation: EnvironmentOrientation,
    pub placement: EnvironmentPlacement,
    /// Dimensionless uniform scale: `1_000_000` is 1x. Must be non-zero.
    pub uniform_scale_millionths: u32,
    /// Explicit application visibility. Asset availability is a consumer concern;
    /// the schema neither admits lifecycle phases nor infers visibility from imported data.
    pub visible: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogBinding {
    pub version: AssetCatalogVersion,
    pub preferred_art_sets: Vec<String>,
    pub preferred_platform_profiles: Vec<String>,
    pub maximum_quality: QualityTier,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthoredOverride {
    pub override_id: u64,
    pub operation: WorldOverride,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum WorldOverride {
    TerrainHeight {
        sample_index: u64,
        height_millimeters: i32,
    },
    RemoveEnvironment {
        instance_id: u64,
    },
}

pub(crate) fn validate_terrain(terrain: &TerrainDocument) -> Result<(), WorldValidationError> {
    if terrain.width_samples < 2 || terrain.depth_samples < 2 {
        return Err(WorldValidationError::TerrainTooSmall);
    }
    if terrain.sample_spacing_millimeters == 0 {
        return Err(WorldValidationError::InvalidTerrainSpacing);
    }
    let expected = terrain_sample_count(terrain)?;
    if terrain.heights_millimeters.len() != expected {
        return Err(WorldValidationError::TerrainSampleCount {
            field: "height",
            expected,
            actual: terrain.heights_millimeters.len(),
        });
    }
    if terrain.material_indices.len() != expected {
        return Err(WorldValidationError::TerrainSampleCount {
            field: "material",
            expected,
            actual: terrain.material_indices.len(),
        });
    }
    if terrain.material_palette.is_empty() {
        return Err(WorldValidationError::EmptyMaterialPalette);
    }
    for slot in &terrain.material_palette {
        if slot.uv_scale_millimeters == 0 {
            return Err(WorldValidationError::InvalidMaterialScale(
                slot.material_id.clone(),
            ));
        }
    }
    for &index in &terrain.material_indices {
        if usize::from(index) >= terrain.material_palette.len() {
            return Err(WorldValidationError::UnknownMaterialIndex(index));
        }
    }
    Ok(())
}

pub(crate) fn terrain_sample_count(
    terrain: &TerrainDocument,
) -> Result<usize, WorldValidationError> {
    usize::try_from(terrain.width_samples)
        .ok()
        .and_then(|width| {
            usize::try_from(terrain.depth_samples)
                .ok()
                .and_then(|depth| width.checked_mul(depth))
        })
        .ok_or(WorldValidationError::TerrainSampleOverflow)
}

pub(crate) fn validate_catalog(catalog: &CatalogBinding) -> Result<(), WorldValidationError> {
    validate_preferences("art set", &catalog.preferred_art_sets)?;
    validate_preferences("platform profile", &catalog.preferred_platform_profiles)
}

pub(crate) fn validate_preferences(
    field: &'static str,
    values: &[String],
) -> Result<(), WorldValidationError> {
    if values.is_empty()
        || values
            .iter()
            .any(|value| value.trim().is_empty() || value.chars().any(char::is_control))
    {
        return Err(WorldValidationError::EmptyCatalogPreference(field));
    }
    let mut unique = BTreeSet::new();
    if values.iter().any(|value| !unique.insert(value)) {
        return Err(WorldValidationError::DuplicateCatalogPreference(field));
    }
    Ok(())
}

pub(crate) fn validate_override(
    authored: &AuthoredOverride,
    terrain: &TerrainDocument,
    environment: &[EnvironmentInstance],
) -> Result<(), WorldValidationError> {
    match authored.operation {
        WorldOverride::TerrainHeight { sample_index, .. } => {
            let sample_count = u64::try_from(terrain_sample_count(terrain)?)
                .map_err(|_| WorldValidationError::TerrainSampleOverflow)?;
            if sample_index >= sample_count {
                return Err(WorldValidationError::InvalidOverride(authored.override_id));
            }
        }
        WorldOverride::RemoveEnvironment { instance_id } => {
            if environment
                .binary_search_by_key(&instance_id, |instance| instance.instance_id)
                .is_err()
            {
                return Err(WorldValidationError::InvalidOverride(authored.override_id));
            }
        }
    }
    Ok(())
}

pub(crate) fn position_is_finite(position: WorldPosition) -> bool {
    position.east_meters.is_finite()
        && position.elevation_meters.is_finite()
        && position.north_meters.is_finite()
}

pub(crate) fn ensure_strictly_ordered<T: Ord>(
    values: impl Iterator<Item = T>,
    error: WorldValidationError,
) -> Result<(), WorldValidationError> {
    let mut previous = None;
    for value in values {
        if previous.as_ref().is_some_and(|previous| previous >= &value) {
            return Err(error);
        }
        previous = Some(value);
    }
    Ok(())
}
