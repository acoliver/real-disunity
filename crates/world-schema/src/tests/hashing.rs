use super::support::{id, instance, world};
use crate::{
    AuthoredOverride, EnvironmentOrientation, EnvironmentPlacement, WorldDocument, WorldOverride,
    canonical_world_hash,
};
use worldrunner_asset_id::QualityTier;

#[test]
fn repeated_hashes_and_negative_zero_are_stable() {
    let mut world = world();
    let expected = canonical_world_hash(&world).unwrap();
    assert_eq!(canonical_world_hash(&world).unwrap(), expected);
    world.environment_instances[0].position.east_meters = -0.0;
    world.environment_instances[0].orientation.quaternion_xyzw[0] = -0.0;
    assert_eq!(canonical_world_hash(&world).unwrap(), expected);
}

fn assert_mutations_change_hash(mutations: &[fn(&mut WorldDocument)]) {
    let original = world();
    let expected = canonical_world_hash(&original).unwrap();
    for mutate in mutations {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert_ne!(canonical_world_hash(&changed).unwrap(), expected);
    }
}

#[test]
fn instance_identity_transform_placement_and_visibility_are_hashed() {
    assert_mutations_change_hash(&[
        |w| w.environment_instances[0].instance_id += 1,
        |w| w.environment_instances[0].asset_id = id("test.environment.other"),
        |w| w.environment_instances[0].position.east_meters = 1.0,
        |w| w.environment_instances[0].position.elevation_meters = 1.0,
        |w| w.environment_instances[0].position.north_meters = 1.0,
        |w| w.environment_instances[0].orientation = EnvironmentOrientation::from_yaw_radians(0.5),
        |w| w.environment_instances[0].placement = EnvironmentPlacement::BoundsGrounded,
        |w| w.environment_instances[0].uniform_scale_millionths += 1,
        |w| w.environment_instances[0].visible = false,
        |w| w.environment_instances.push(instance(2)),
    ]);
}

#[test]
fn world_terrain_biome_and_generation_fields_are_hashed() {
    assert_mutations_change_hash(&[
        |w| w.generator_version.0 += 1,
        |w| w.world_id = worldrunner_core_types::WorldId::from_uuid(uuid::Uuid::from_u128(2)),
        |w| w.seed.0 += 1,
        |w| w.dimensions.width_meters += 1,
        |w| w.dimensions.depth_meters += 1,
        |w| w.generation_parameters.sea_level_millimeters += 1,
        |w| w.generation_parameters.elevation_amplitude_millimeters += 1,
        |w| w.generation_parameters.terrain_frequency_microunits += 1,
        |w| {
            w.generation_parameters
                .environment_density_per_square_kilometer += 1;
        },
        |w| w.terrain.heights_millimeters[0] += 1,
        |w| w.terrain.sample_spacing_millimeters += 1,
        |w| w.terrain.material_palette[0].material_id = id("test.material.other"),
        |w| w.terrain.material_palette[0].uv_scale_millimeters += 1,
        |w| w.biome_definitions[0].biome_asset_id = id("test.biome.other"),
        |w| w.biome_regions[0].region_id += 1,
        |w| w.biome_regions[0].center_east_millimeters += 1,
        |w| w.biome_regions[0].center_north_millimeters += 1,
        |w| w.biome_regions[0].radius_millimeters += 1,
    ]);
}

#[test]
fn explicit_catalog_and_overrides_are_hashed() {
    assert_mutations_change_hash(&[
        |w| w.catalog.version.0 += 1,
        |w| w.catalog.preferred_art_sets[0] = "another-art".to_owned(),
        |w| w.catalog.preferred_platform_profiles[0] = "another-platform".to_owned(),
        |w| w.catalog.maximum_quality = QualityTier::Ultra,
        |w| {
            w.authored_overrides.push(AuthoredOverride {
                override_id: 1,
                operation: WorldOverride::RemoveEnvironment { instance_id: 1 },
            });
        },
        |w| {
            w.authored_overrides.push(AuthoredOverride {
                override_id: 1,
                operation: WorldOverride::TerrainHeight {
                    sample_index: 0,
                    height_millimeters: 1,
                },
            });
        },
    ]);
}

#[test]
fn invalid_documents_never_produce_a_hash() {
    let mut world = world();
    world.environment_instances[0].orientation.quaternion_xyzw = [0.0; 4];
    assert!(canonical_world_hash(&world).is_err());
}

#[test]
fn hash_strings_are_length_delimited_not_concatenated() {
    let mut a = world();
    let mut b = world();
    a.catalog.preferred_art_sets = vec!["ab".to_owned(), "c".to_owned()];
    b.catalog.preferred_art_sets = vec!["a".to_owned(), "bc".to_owned()];
    assert_ne!(
        canonical_world_hash(&a).unwrap(),
        canonical_world_hash(&b).unwrap()
    );
    b.catalog.preferred_art_sets = vec!["c".to_owned(), "ab".to_owned()];
    assert_ne!(
        canonical_world_hash(&a).unwrap(),
        canonical_world_hash(&b).unwrap()
    );
}
