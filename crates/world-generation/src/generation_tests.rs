fn request(seed: u64, profile: GenerationProfile) -> WorldGenerationRequest {
    let mut request = WorldGenerationRequest::from_profile(profile, "smoke").unwrap();
    request.seed = WorldSeed(seed);
    request
}

fn authored_profile() -> GenerationProfile {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/generation-profile-v1.json");
    GenerationProfile::from_authoring(&path).expect("authored generation profile parses")
}

use super::*;
use std::collections::BTreeSet;
use worldrunner_asset_id::QualityTier;

/// An authored profile that differs from legacy in biome assets, thresholds,
/// terrain material, placement probabilities, prop assets and scales, and
/// catalog art preferences.
fn verdant_highlands_profile() -> GenerationProfile {
    let mut profile = authored_profile();
    profile.name = "verdant-highlands".to_owned();
    profile.biomes.coastal = BiomeChoice {
        definition_id: 1,
        biome_asset_id: "world.biome.littoral".to_owned(),
    };
    profile.biomes.temperate = BiomeChoice {
        definition_id: 2,
        biome_asset_id: "world.biome.woodland".to_owned(),
    };
    profile.biomes.highland = BiomeChoice {
        definition_id: 3,
        biome_asset_id: "world.biome.alpine".to_owned(),
    };
    profile.biomes.highland_max_temperature = 20_000;
    profile.biomes.highland_min_height_millimeters = 28_000;
    profile.surfaces.materials[1] = MaterialChoice {
        material_asset_id: "terrain.material.meadow".to_owned(),
        uv_scale_millimeters: 5_000,
    };
    profile.surfaces.dry_moisture_max = 30_000;
    profile.placement.rock_denominator_rocky_terrain = 6;
    profile.placement.rock_denominator_other_terrain = 12;
    profile.placement.tree_denominator = 11;
    profile.placement.tree = PropFamily {
        asset_id: "environment.vegetation.tree.pine_01".to_owned(),
        base_scale_millionths: 900_000,
        scale_jitter_modulus: 500_001,
    };
    profile.placement.grass = PropFamily {
        asset_id: "environment.vegetation.fern_01".to_owned(),
        base_scale_millionths: 1_000_000,
        scale_jitter_modulus: 600_001,
    };
    profile.catalog.preferred_art_sets = vec!["verdant-restyle".to_owned()];
    profile.catalog.maximum_quality = QualityTier::High;
    profile
}

fn hash_hex(hash: WorldStateHash) -> String {
    use std::fmt::Write as _;

    hash.0
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}

#[test]
fn identical_requests_generate_identical_worlds_and_hashes() {
    let request = request(42, authored_profile());
    let a = generate_world_with_hash(&request).unwrap();
    let b = generate_world_with_hash(&request).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.document.validate(), Ok(()));
    assert!(!a.document.biome_regions.is_empty());
    assert!(!a.document.environment_instances.is_empty());
}

#[test]
fn different_seeds_change_canonical_hash() {
    let a = generate_world_with_hash(&request(1, authored_profile())).unwrap();
    let b = generate_world_with_hash(&request(2, authored_profile())).unwrap();
    assert_ne!(a.canonical_hash, b.canonical_hash);
    assert_ne!(
        a.document.terrain.heights_millimeters,
        b.document.terrain.heights_millimeters
    );
}

#[test]
fn generation_streams_are_domain_separated() {
    let elevation = hash_u64(42, "elevation", &[1, 2, 3]);
    let moisture = hash_u64(42, "climate-moisture", &[1, 2, 3]);
    assert_ne!(elevation, moisture);
    assert_eq!(elevation, hash_u64(42, "elevation", &[1, 2, 3]));
}

#[test]
fn canonical_hash_has_stable_golden_vector() {
    let generated = generate_world_with_hash(&request(42, authored_profile())).unwrap();
    // Generator revision 2: portable Q56/Q40 yaw, unchanged revision-1 streams
    // and synthetic profile under the fresh-v1 document/hash contract.
    assert_eq!(
        hash_hex(generated.canonical_hash),
        "dea43c1c90bf9095183ed85bf6669c677ccedb83d76a53d1285f0ecd612ea104"
    );
}

#[test]
fn named_request_carries_the_supplied_profile() {
    let request = request(42, authored_profile());
    assert_eq!(request.profile, authored_profile());
    assert_eq!(request.profile.validate(), Ok(()));
}

#[test]
fn invalid_profile_fails_before_publication() {
    let mut profile = authored_profile();
    profile.placement.grass.asset_id = "not-a-logical-id".to_owned();
    let error = WorldGenerationRequest::from_profile(profile, "smoke").unwrap_err();
    assert!(
        matches!(error, GenerationError::InvalidProfile(_)),
        "{error}"
    );
}

#[test]
fn publication_records_profile_name_and_digest() {
    let request = request(42, authored_profile());
    let generated = generate_world_with_hash(&request).unwrap();
    assert_eq!(generated.profile_name, "synthetic-grid");
    assert_eq!(generated.profile_digest, request.profile.digest());
}

#[test]
fn authored_profile_changes_biomes_materials_art_and_placements() {
    let legacy = generate_world_with_hash(&request(42, authored_profile())).unwrap();
    let authored = generate_world_with_hash(&request(42, verdant_highlands_profile())).unwrap();
    assert_eq!(
        legacy.document.validate(),
        Ok(()),
        "authored profile output must validate"
    );
    assert_ne!(legacy.canonical_hash, authored.canonical_hash);
    assert_ne!(
        legacy.document.biome_definitions,
        authored.document.biome_definitions
    );
    assert_ne!(
        legacy.document.terrain.material_palette,
        authored.document.terrain.material_palette
    );
    assert_ne!(
        legacy.document.catalog.preferred_art_sets,
        authored.document.catalog.preferred_art_sets
    );
    assert_ne!(
        legacy.document.environment_instances,
        authored.document.environment_instances
    );
    let authored_assets: BTreeSet<&str> = authored
        .document
        .environment_instances
        .iter()
        .map(|instance| instance.asset_id.as_str())
        .collect();
    assert!(
        authored_assets.contains("environment.vegetation.tree.pine_01")
            && authored_assets.contains("environment.vegetation.fern_01"),
        "authored prop assets must appear: {authored_assets:?}"
    );
}

#[test]
fn same_seed_and_profile_reproduce_identical_publication() {
    let request = request(7, verdant_highlands_profile());
    let first = generate_world_with_hash(&request).unwrap();
    let second = generate_world_with_hash(&request).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.document.validate(), Ok(()));
    assert_eq!(first.profile_digest, request.profile.digest());
}

#[test]
fn profile_identity_changes_while_seed_stays_the_user_input() {
    let legacy_request = request(7, authored_profile());
    let authored_request = request(7, verdant_highlands_profile());
    assert_eq!(legacy_request.seed, authored_request.seed);
    assert_ne!(legacy_request.profile.name, authored_request.profile.name);
    assert_ne!(
        legacy_request.profile.digest(),
        authored_request.profile.digest()
    );
    assert_ne!(
        generate_world_with_hash(&legacy_request)
            .unwrap()
            .profile_digest,
        generate_world_with_hash(&authored_request)
            .unwrap()
            .profile_digest
    );
}
