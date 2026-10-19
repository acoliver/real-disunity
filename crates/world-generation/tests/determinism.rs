use worldrunner_world_generation::{
    GenerationProfile, WorldGenerationRequest, generate_world_with_hash,
};

fn request() -> WorldGenerationRequest {
    let profile = GenerationProfile::from_authoring(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/generation-profile-v1.json"),
    )
    .unwrap();
    WorldGenerationRequest::development(42, profile)
}

#[test]
fn terrain_keeps_its_pre_portability_signature() {
    let generated = generate_world_with_hash(&request()).unwrap();
    let mut hasher = blake3::Hasher::new();
    for height in &generated.document.terrain.heights_millimeters {
        hasher.update(&height.to_le_bytes());
    }
    for material in &generated.document.terrain.material_indices {
        hasher.update(&material.to_le_bytes());
    }
    assert_eq!(
        hasher.finalize().to_hex().as_str(),
        "2af986c74c7abd461c0b0558decbb8468e462b9174a1eb730625e4c36f7437c3"
    );
}

#[test]
fn generated_rotation_bits_have_a_stable_signature() {
    let generated = generate_world_with_hash(&request()).unwrap();
    let mut hasher = blake3::Hasher::new();
    for instance in &generated.document.environment_instances {
        hasher.update(&instance.instance_id.to_le_bytes());
        for component in instance.orientation.quaternion_xyzw {
            hasher.update(&component.to_bits().to_le_bytes());
        }
    }
    assert_eq!(
        hasher.finalize().to_hex().as_str(),
        "34662a66d2047d461c11703694b806e8a3f630589ff450e91129ab6234b57bdb"
    );
}

#[test]
fn generated_rotations_are_exact_binary_fractions() {
    let generated = generate_world_with_hash(&request()).unwrap();
    assert!(!generated.document.environment_instances.is_empty());
    for instance in &generated.document.environment_instances {
        for component in instance.orientation.quaternion_xyzw {
            let scaled = component * 1_099_511_627_776.0;
            assert_eq!(
                scaled.to_bits(),
                scaled.trunc().to_bits(),
                "instance {}",
                instance.instance_id
            );
        }
        assert_eq!(instance.orientation.validate(), Ok(()));
    }
}
