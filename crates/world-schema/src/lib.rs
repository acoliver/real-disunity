//! Generic engine world documents. This is a fresh v1 contract, not a source-data format.
//! Importers own source decoding, axis/unit conversion and application visibility policy.

use worldrunner_core_types::{GenerationVersion, SchemaVersion};

pub const CURRENT_WORLD_SCHEMA: SchemaVersion = SchemaVersion(1);
/// Revision 2 replaces platform-dependent generated yaw with integer Q56/Q40 math.
pub const CURRENT_GENERATOR_VERSION: GenerationVersion = GenerationVersion(2);

pub mod document;
pub mod frame;
pub mod hashing;
mod validation;

pub use document::{
    AuthoredOverride, BiomeDefinition, BiomeRegion, CatalogBinding, EnvironmentInstance,
    GenerationParameters, TerrainDocument, TerrainMaterialSlot, WorldDocument, WorldOverride,
};
pub use frame::{
    EnvironmentOrientation, EnvironmentPlacement, OrientationError, WorldCoordinateFrame,
};
pub use hashing::canonical_world_hash;
pub use validation::WorldValidationError;

#[cfg(test)]
mod tests;
