use super::support::{instance, world};
use crate::{AuthoredOverride, EnvironmentOrientation, WorldOverride, WorldValidationError};
use worldrunner_core_types::SchemaVersion;

#[test]
fn validates_consistent_world_with_explicit_hidden_or_visible_instances() {
    let mut world = world();
    assert_eq!(world.validate(), Ok(()));
    world.environment_instances[0].visible = false;
    assert_eq!(world.validate(), Ok(()));
}

#[test]
fn rejects_other_schema_versions_and_empty_dimensions() {
    for version in [0, 2, 3, 4, u32::MAX] {
        let mut world = world();
        world.schema_version = SchemaVersion(version);
        assert_eq!(
            world.validate(),
            Err(WorldValidationError::UnsupportedSchema(version))
        );
    }
    let mut world = world();
    world.dimensions.width_meters = 0;
    assert_eq!(world.validate(), Err(WorldValidationError::EmptyDimensions));
}

#[test]
fn rejects_inconsistent_sample_counts() {
    let mut world = world();
    world.terrain.material_indices.pop();
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::TerrainSampleCount {
            field: "material",
            expected: 4,
            actual: 3,
        })
    );
    world.terrain.material_indices.push(0);
    world.terrain.heights_millimeters.pop();
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::TerrainSampleCount {
            field: "height",
            expected: 4,
            actual: 3,
        })
    );
}

#[test]
fn rejects_degenerate_terrain_and_invalid_material_indices() {
    let mut world = world();
    world.terrain.width_samples = 1;
    assert_eq!(world.validate(), Err(WorldValidationError::TerrainTooSmall));
    world.terrain.width_samples = 2;
    world.terrain.sample_spacing_millimeters = 0;
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::InvalidTerrainSpacing)
    );
    world.terrain.sample_spacing_millimeters = 1_000;
    world.terrain.material_indices[0] = 1;
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::UnknownMaterialIndex(1))
    );
}

#[test]
fn rejects_empty_material_palettes_and_zero_uv_scale() {
    let mut world = world();
    world.terrain.material_palette[0].uv_scale_millimeters = 0;
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::InvalidMaterialScale(
            world.terrain.material_palette[0].material_id.clone(),
        ))
    );
    world.terrain.material_palette.clear();
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::EmptyMaterialPalette)
    );
}

#[test]
fn rejects_duplicate_or_unordered_environment_ids() {
    for ids in [[1, 1], [2, 1]] {
        let mut world = world();
        world.environment_instances = ids.into_iter().map(instance).collect();
        assert_eq!(
            world.validate(),
            Err(WorldValidationError::EnvironmentOrder)
        );
    }
}

#[test]
fn hidden_instances_still_require_valid_transforms() {
    let mut world = world();
    let item = &mut world.environment_instances[0];
    item.visible = false;
    item.uniform_scale_millionths = 0;
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::InvalidEnvironmentInstance(1))
    );
    world.environment_instances[0].uniform_scale_millionths = 1;
    world.environment_instances[0].orientation = EnvironmentOrientation {
        quaternion_xyzw: [0.0; 4],
    };
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::InvalidEnvironmentInstance(1))
    );
}

#[test]
fn rejects_nonfinite_positions_on_every_axis() {
    for axis in 0..3 {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut world = world();
            let position = &mut world.environment_instances[0].position;
            match axis {
                0 => position.east_meters = invalid,
                1 => position.elevation_meters = invalid,
                _ => position.north_meters = invalid,
            }
            assert_eq!(
                world.validate(),
                Err(WorldValidationError::InvalidEnvironmentInstance(1))
            );
        }
    }
}

#[test]
fn rejects_invalid_biome_references_and_duplicates() {
    let mut world = world();
    world
        .biome_definitions
        .push(world.biome_definitions[0].clone());
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::DuplicateBiomeDefinition(1))
    );
    world.biome_definitions.pop();
    world.biome_regions.push(world.biome_regions[0].clone());
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::DuplicateBiomeRegion(1))
    );
    world.biome_regions.pop();
    world.biome_regions[0].biome_definition_id = 7;
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::UnknownBiomeDefinition(7))
    );
    world.biome_regions[0].biome_definition_id = 1;
    world.biome_regions[0].radius_millimeters = 0;
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::EmptyBiomeRegion(1))
    );
}

#[test]
fn catalog_preferences_are_explicit_nonempty_and_unique() {
    for invalid in [
        vec![],
        vec![String::new()],
        vec![" ".to_owned()],
        vec!["bad\nname".to_owned()],
    ] {
        let mut world = world();
        world.catalog.preferred_art_sets = invalid.clone();
        assert_eq!(
            world.validate(),
            Err(WorldValidationError::EmptyCatalogPreference("art set"))
        );
        world.catalog.preferred_art_sets = vec!["caller-art".to_owned()];
        world.catalog.preferred_platform_profiles = invalid;
        assert_eq!(
            world.validate(),
            Err(WorldValidationError::EmptyCatalogPreference(
                "platform profile"
            ))
        );
    }
    let mut world = world();
    world
        .catalog
        .preferred_art_sets
        .push(world.catalog.preferred_art_sets[0].clone());
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::DuplicateCatalogPreference("art set"))
    );
}

#[test]
fn validates_override_order_and_references() {
    let mut world = world();
    world.authored_overrides = vec![
        AuthoredOverride {
            override_id: 1,
            operation: WorldOverride::TerrainHeight {
                sample_index: 3,
                height_millimeters: -500,
            },
        },
        AuthoredOverride {
            override_id: 2,
            operation: WorldOverride::RemoveEnvironment { instance_id: 1 },
        },
    ];
    assert_eq!(world.validate(), Ok(()));
    world.authored_overrides.swap(0, 1);
    assert_eq!(world.validate(), Err(WorldValidationError::OverrideOrder));
    world.authored_overrides.swap(0, 1);
    world.authored_overrides[1].operation = WorldOverride::RemoveEnvironment { instance_id: 2 };
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::InvalidOverride(2))
    );
    world.authored_overrides.pop();
    world.authored_overrides[0].operation = WorldOverride::TerrainHeight {
        sample_index: 4,
        height_millimeters: 0,
    };
    assert_eq!(
        world.validate(),
        Err(WorldValidationError::InvalidOverride(1))
    );
}
