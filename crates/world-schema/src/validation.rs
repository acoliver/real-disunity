use thiserror::Error;
use worldrunner_asset_id::LogicalAssetId;

#[derive(Debug, Error, PartialEq)]
pub enum WorldValidationError {
    #[error("unsupported world schema version {0}")]
    UnsupportedSchema(u32),
    #[error("world dimensions must be non-zero")]
    EmptyDimensions,
    #[error("terrain must contain at least two samples on each axis")]
    TerrainTooSmall,
    #[error("terrain sample spacing must be non-zero")]
    InvalidTerrainSpacing,
    #[error("terrain sample count overflowed host limits")]
    TerrainSampleOverflow,
    #[error("terrain expected {expected} {field} samples but contains {actual}")]
    TerrainSampleCount {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("terrain material palette must not be empty")]
    EmptyMaterialPalette,
    #[error("terrain material {0} has an invalid UV scale")]
    InvalidMaterialScale(LogicalAssetId),
    #[error("terrain references unknown material palette index {0}")]
    UnknownMaterialIndex(u16),
    #[error("biome definition ID {0} is duplicated")]
    DuplicateBiomeDefinition(u32),
    #[error("biome region ID {0} is duplicated")]
    DuplicateBiomeRegion(u32),
    #[error("biome definition ID {0} does not exist")]
    UnknownBiomeDefinition(u32),
    #[error("biome region {0} has no area")]
    EmptyBiomeRegion(u32),
    #[error("environment instances must be strictly ordered by ID")]
    EnvironmentOrder,
    #[error("environment instance {0} has an invalid transform")]
    InvalidEnvironmentInstance(u64),
    #[error("authored overrides must be strictly ordered by ID")]
    OverrideOrder,
    #[error("authored override {0} references data outside the world")]
    InvalidOverride(u64),
    #[error("catalog {0} preferences must not be blank or contain control characters")]
    EmptyCatalogPreference(&'static str),
    #[error("catalog {0} preferences must not contain duplicates")]
    DuplicateCatalogPreference(&'static str),
}
