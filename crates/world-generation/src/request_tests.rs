use super::*;

fn profile() -> GenerationProfile {
    GenerationProfile::from_authoring(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/generation-profile-v1.json"),
    )
    .unwrap()
}

#[test]
fn named_request_consumes_every_authored_request_field() {
    let mut profile = profile();
    let mut preset = profile.requests[0].clone();
    preset.name = "rectangular".to_owned();
    preset.seed = 91;
    preset.dimensions_meters = [240, 480];
    preset.terrain_samples = [9, 17];
    preset.parameters = GenerationParameters {
        sea_level_millimeters: -700,
        elevation_amplitude_millimeters: 91_000,
        terrain_frequency_microunits: 5_000,
        environment_density_per_square_kilometer: 900,
    };
    preset.asset_catalog_version = 7;
    profile.requests.push(preset.clone());
    let request = WorldGenerationRequest::from_profile(profile, "rectangular").unwrap();
    let generated = generate_world_with_hash(&request).unwrap();
    assert_eq!(request.seed.0, preset.seed);
    assert_eq!(generated.document.seed, request.seed);
    assert_eq!(
        generated.document.dimensions,
        WorldDimensions::new(240, 480)
    );
    assert_eq!(
        [
            generated.document.terrain.width_samples,
            generated.document.terrain.depth_samples
        ],
        preset.terrain_samples
    );
    assert_eq!(generated.document.generation_parameters, preset.parameters);
    assert_eq!(
        generated.document.catalog.version.0,
        preset.asset_catalog_version
    );
    assert_eq!(
        generated.document.terrain.sample_spacing_millimeters,
        30_000
    );
}

#[test]
fn requests_reject_unknown_missing_and_duplicate_names() {
    assert!(WorldGenerationRequest::from_profile(profile(), "missing").is_err());
    let mut empty = profile();
    empty.requests.clear();
    assert!(empty.validate().is_err());
    let mut duplicate = profile();
    duplicate.requests.push(duplicate.requests[0].clone());
    assert!(duplicate.validate().unwrap_err().contains("duplicate"));
    let mut unnamed = profile();
    unnamed.requests[0].name = " ".to_owned();
    assert!(unnamed.validate().is_err());
}

#[test]
fn request_json_requires_every_field_and_rejects_unknown_fields() {
    let preset = serde_json::to_value(&profile().requests[0]).unwrap();
    for field in preset.as_object().unwrap().keys() {
        let mut incomplete = preset.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<GenerationPreset>(incomplete).is_err(),
            "{field}"
        );
    }
    for field in preset["parameters"].as_object().unwrap().keys() {
        let mut incomplete = preset.clone();
        incomplete["parameters"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<GenerationPreset>(incomplete).is_err(),
            "{field}"
        );
    }
    let mut unknown = preset;
    unknown["typo"] = serde_json::json!(1);
    assert!(serde_json::from_value::<GenerationPreset>(unknown).is_err());
}

#[test]
fn invalid_request_shapes_fail_before_generation() {
    for (dimensions, samples) in [
        ([0, 12], [3, 3]),
        ([12, 12], [1, 3]),
        ([12, 12], [u32::MAX, u32::MAX]),
    ] {
        let mut profile = profile();
        profile.requests[0].dimensions_meters = dimensions;
        profile.requests[0].terrain_samples = samples;
        assert!(WorldGenerationRequest::from_profile(profile, "smoke").is_err());
    }
    for amplitude in [true, false] {
        let mut profile = profile();
        if amplitude {
            profile.requests[0]
                .parameters
                .elevation_amplitude_millimeters = 0;
        } else {
            profile.requests[0].parameters.terrain_frequency_microunits = 0;
        }
        assert!(WorldGenerationRequest::from_profile(profile, "smoke").is_err());
    }
}

#[test]
fn generation_revalidates_mutated_requests() {
    let mut request = WorldGenerationRequest::from_profile(profile(), "smoke").unwrap();
    request.terrain_samples = [0, 0];
    assert!(matches!(
        generate_world(&request),
        Err(GenerationError::TerrainTooSmall)
    ));
    request.terrain_samples = [2, 2];
    request.profile.placement.tree_denominator = 0;
    assert!(matches!(
        generate_world(&request),
        Err(GenerationError::InvalidProfile(_))
    ));
}

#[test]
fn request_parameters_reach_terrain_and_placement_consumers() {
    let mut request = WorldGenerationRequest::from_profile(profile(), "smoke").unwrap();
    request.seed = WorldSeed(42);
    request.dimensions = WorldDimensions::new(1024, 1024);
    request.terrain_samples = [65, 65];
    request.parameters.sea_level_millimeters = i32::MIN;
    let baseline = generate_world(&request).unwrap();
    assert!(!baseline.environment_instances.is_empty());
    let mut amplitude = request.clone();
    amplitude.parameters.elevation_amplitude_millimeters /= 2;
    assert_ne!(
        generate_world(&amplitude)
            .unwrap()
            .terrain
            .heights_millimeters,
        baseline.terrain.heights_millimeters
    );
    let mut frequency = request.clone();
    frequency.parameters.terrain_frequency_microunits *= 2;
    assert_ne!(
        generate_world(&frequency)
            .unwrap()
            .terrain
            .heights_millimeters,
        baseline.terrain.heights_millimeters
    );
    let mut density = request.clone();
    density.parameters.environment_density_per_square_kilometer *= 2;
    assert!(
        generate_world(&density)
            .unwrap()
            .environment_instances
            .len()
            > baseline.environment_instances.len()
    );
    request.parameters.sea_level_millimeters = i32::MAX;
    let submerged = generate_world(&request).unwrap();
    assert!(
        submerged
            .terrain
            .material_indices
            .iter()
            .all(|index| *index == 0)
    );
    assert!(submerged.environment_instances.is_empty());
}

#[test]
fn profiles_require_requests_and_requests_do_not_change_the_content_digest() {
    let profile = profile();
    let mut missing = serde_json::to_value(&profile).unwrap();
    missing.as_object_mut().unwrap().remove("requests");
    assert!(serde_json::from_value::<GenerationProfile>(missing).is_err());
    let mut changed = profile.clone();
    changed.requests[0].seed += 1;
    assert_eq!(profile.digest(), changed.digest());
    let mut unknown = serde_json::to_value(&profile.requests[0]).unwrap();
    unknown["parameters"]["typo"] = serde_json::json!(1);
    assert!(serde_json::from_value::<GenerationPreset>(unknown).is_err());
}

fn boundary_profile() -> GenerationProfile {
    let mut profile = profile();
    profile.requests[0].terrain_samples = [3, 3];
    profile.requests[0]
        .parameters
        .environment_density_per_square_kilometer = 0;
    profile
}

#[test]
fn amplitude_boundaries_are_checked_by_constructor_and_public_generation() {
    let mut profile = boundary_profile();
    profile.requests[0].seed = 7;
    profile.requests[0].parameters.terrain_frequency_microunits = 1;
    profile.requests[0]
        .parameters
        .elevation_amplitude_millimeters = 1_171_354_717;
    let mut request = WorldGenerationRequest::from_profile(profile.clone(), "smoke").unwrap();
    assert!(generate_world(&request).is_ok());
    for amplitude in [1_171_354_718, u32::MAX] {
        profile.requests[0]
            .parameters
            .elevation_amplitude_millimeters = amplitude;
        assert!(WorldGenerationRequest::from_profile(profile.clone(), "smoke").is_err());
        request.parameters.elevation_amplitude_millimeters = amplitude;
        assert!(matches!(
            generate_world(&request),
            Err(GenerationError::ElevationAmplitudeTooLarge)
        ));
    }
}

#[test]
fn extent_boundaries_are_checked_on_each_axis_before_generation() {
    for axis in 0..2 {
        let mut profile = boundary_profile();
        profile.requests[0].dimensions_meters[axis] = 4_294_967;
        let mut request = WorldGenerationRequest::from_profile(profile.clone(), "smoke").unwrap();
        assert!(generate_world(&request).is_ok());
        let extent = 4_294_967_000;
        assert_eq!(
            millimeters_to_centered_meters(0, extent).to_bits(),
            (-2_147_483.5_f64).to_bits()
        );
        assert_eq!(
            millimeters_to_centered_meters(extent - 1, extent).to_bits(),
            2_147_483.499_f64.to_bits()
        );
        for meters in [4_294_968, 50_000_000] {
            profile.requests[0].dimensions_meters[axis] = meters;
            assert!(WorldGenerationRequest::from_profile(profile.clone(), "smoke").is_err());
            request.dimensions = WorldDimensions::new(
                profile.requests[0].dimensions_meters[0],
                profile.requests[0].dimensions_meters[1],
            );
            assert!(matches!(
                validate_request(&request),
                Err(GenerationError::DimensionsTooLarge)
            ));
            assert!(matches!(
                generate_world(&request),
                Err(GenerationError::DimensionsTooLarge)
            ));
        }
    }
}

#[test]
fn candidate_overflow_is_rejected_in_selected_and_unselected_presets() {
    let mut profile = boundary_profile();
    profile.requests[0].dimensions_meters = [100_000, 100_000];
    profile.requests[0]
        .parameters
        .environment_density_per_square_kilometer = u32::MAX;
    assert!(WorldGenerationRequest::from_profile(profile.clone(), "smoke").is_err());
    let mut unselected = boundary_profile();
    profile.requests[0].name = "overflow".to_owned();
    unselected.requests.push(profile.requests.remove(0));
    assert!(WorldGenerationRequest::from_profile(unselected, "smoke").is_err());
}

#[test]
fn candidate_product_boundary_is_checked_by_runtime_preflight() {
    let mut profile = boundary_profile();
    profile.requests[0].dimensions_meters = [100_000, 100_000];
    profile.requests[0]
        .parameters
        .environment_density_per_square_kilometer = 1_844_674_407;
    let mut request = WorldGenerationRequest::from_profile(profile.clone(), "smoke").unwrap();
    assert!(validate_request(&request).is_ok());
    assert_eq!(
        environment_candidate_count(&request).unwrap(),
        ENVIRONMENT_LIMIT
    );
    for density in [1_844_674_408, u32::MAX] {
        profile.requests[0]
            .parameters
            .environment_density_per_square_kilometer = density;
        assert!(WorldGenerationRequest::from_profile(profile.clone(), "smoke").is_err());
        request.parameters.environment_density_per_square_kilometer = density;
        assert!(matches!(
            validate_request(&request),
            Err(GenerationError::EnvironmentCandidateCountOverflow)
        ));
        assert!(matches!(
            generate_world(&request),
            Err(GenerationError::EnvironmentCandidateCountOverflow)
        ));
    }
}

#[test]
fn amplitude_limit_fits_both_shaping_extremes_without_height_clipping() {
    let divisor = i64::from(i32::MAX);
    let lattice_max = i64::from(u32::MAX) - i64::from(u32::MAX / 2);
    let ridge_max = lattice_max * lattice_max / divisor * (divisor / 2) / divisor;
    let shaped_max = apply_island_falloff(lattice_max + divisor / 3 + ridge_max, 1, 1, 3, 3);
    let shaped_min = apply_island_falloff(-divisor + divisor / 3, 0, 0, 3, 3);
    assert_eq!(shaped_max, 3_937_053_353);
    assert_eq!(shaped_min, -3_579_139_412);
    let amplitude = 1_171_354_717;
    assert_eq!(shaped_max * amplitude / divisor, i64::from(i32::MAX));
    assert!(i32::try_from(shaped_min * amplitude / divisor).is_ok());
    assert!(i32::try_from(shaped_max * (amplitude + 1) / divisor).is_err());
}
