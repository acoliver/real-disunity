use crate::{
    BiomeDefinition, BiomeRegion, CURRENT_GENERATOR_VERSION, CURRENT_WORLD_SCHEMA, CatalogBinding,
    EnvironmentInstance, EnvironmentOrientation, EnvironmentPlacement, GenerationParameters,
    TerrainDocument, TerrainMaterialSlot, WorldCoordinateFrame, WorldDocument,
};
use uuid::Uuid;
use worldrunner_asset_id::{LogicalAssetId, QualityTier};
use worldrunner_core_types::{
    AssetCatalogVersion, WorldDimensions, WorldId, WorldPosition, WorldSeed,
};

pub(super) fn id(value: &str) -> LogicalAssetId {
    LogicalAssetId::new(value).unwrap()
}

pub(super) fn world() -> WorldDocument {
    WorldDocument {
        schema_version: CURRENT_WORLD_SCHEMA,
        generator_version: CURRENT_GENERATOR_VERSION,
        coordinate_frame: WorldCoordinateFrame::RightHandedYUp,
        world_id: WorldId::from_uuid(Uuid::from_u128(1)),
        seed: WorldSeed(7),
        dimensions: WorldDimensions::new(64, 64),
        generation_parameters: GenerationParameters {
            sea_level_millimeters: 0,
            elevation_amplitude_millimeters: 80_000,
            terrain_frequency_microunits: 4_000,
            environment_density_per_square_kilometer: 600,
        },
        terrain: TerrainDocument {
            width_samples: 2,
            depth_samples: 2,
            sample_spacing_millimeters: 1_000,
            heights_millimeters: vec![0; 4],
            material_palette: vec![TerrainMaterialSlot {
                material_id: id("test.material.ground"),
                uv_scale_millimeters: 4_000,
            }],
            material_indices: vec![0; 4],
        },
        biome_definitions: vec![BiomeDefinition {
            definition_id: 1,
            biome_asset_id: id("test.biome.temperate"),
        }],
        biome_regions: vec![BiomeRegion {
            region_id: 1,
            biome_definition_id: 1,
            center_east_millimeters: 1_000,
            center_north_millimeters: 1_000,
            radius_millimeters: 500,
        }],
        environment_instances: vec![instance(1)],
        catalog: CatalogBinding {
            version: AssetCatalogVersion(3),
            preferred_art_sets: vec!["caller-art".to_owned()],
            preferred_platform_profiles: vec!["caller-platform".to_owned()],
            maximum_quality: QualityTier::High,
        },
        authored_overrides: Vec::new(),
    }
}

pub(super) fn instance(instance_id: u64) -> EnvironmentInstance {
    EnvironmentInstance {
        instance_id,
        asset_id: id("test.environment.prop"),
        position: WorldPosition::ORIGIN,
        orientation: EnvironmentOrientation::IDENTITY,
        placement: EnvironmentPlacement::Absolute,
        uniform_scale_millionths: 1_000_000,
        visible: true,
    }
}
