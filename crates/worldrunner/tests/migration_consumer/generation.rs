use worldrunner::world::worldrunner_world_generation::{
    GeneratedWorld, GenerationProfile, WorldGenerationRequest, generate_world_with_hash,
};

pub fn profile() -> GenerationProfile {
    GenerationProfile::from_authoring(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/migration_consumer/profile.json"),
    )
    .unwrap()
}

pub fn generated() -> GeneratedWorld {
    let request = WorldGenerationRequest::from_profile(profile(), "consumer").unwrap();
    generate_world_with_hash(&request).unwrap()
}

#[test]
fn explicit_generation_preserves_caller_inputs_and_determinism() {
    let first = generated();
    let second = generated();
    assert_eq!(first, second);
    assert_eq!(first.document.seed.0, 42);
    assert_eq!(
        first.document.dimensions,
        worldrunner::WorldDimensions::new(12, 8)
    );
    assert_eq!(first.document.terrain.heights_millimeters.len(), 35);
    assert_eq!(first.document.terrain.sample_spacing_millimeters, 2000);
    let mut changed = profile();
    changed.requests[0].seed = 43;
    let changed = generate_world_with_hash(
        &WorldGenerationRequest::from_profile(changed, "consumer").unwrap(),
    )
    .unwrap();
    assert_ne!(changed.canonical_hash, first.canonical_hash);
    assert_ne!(
        changed.document.terrain.heights_millimeters,
        first.document.terrain.heights_millimeters
    );
}

#[test]
fn malformed_generation_is_rejected_without_fallback() {
    assert!(WorldGenerationRequest::from_profile(profile(), "absent").is_err());
    for (extent, samples) in [([0, 8], [7, 5]), ([12, 8], [1, 5])] {
        let mut invalid = profile();
        invalid.requests[0].dimensions_meters = extent;
        invalid.requests[0].terrain_samples = samples;
        assert!(WorldGenerationRequest::from_profile(invalid, "consumer").is_err());
    }
    let mut request = WorldGenerationRequest::from_profile(profile(), "consumer").unwrap();
    request.parameters.terrain_frequency_microunits = 0;
    assert!(generate_world_with_hash(&request).is_err());
}
