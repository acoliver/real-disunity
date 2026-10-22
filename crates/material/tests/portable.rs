use worldrunner_asset_id::LogicalAssetId;
use worldrunner_material::{AlphaMode, MaterialArtifact, MaterialError};

fn id(value: &str) -> LogicalAssetId {
    LogicalAssetId::new(value).unwrap()
}

fn material() -> MaterialArtifact {
    MaterialArtifact {
        albedo_texture: id("fixture.texture.base"),
        normal_texture: Some(id("fixture.texture.normal")),
        roughness_metallic_texture: Some(id("fixture.texture.surface")),
        ambient_occlusion_texture: Some(id("fixture.texture.surface")),
        self_illumination_texture: None,
        base_color_factor: [0.25, 0.5, 0.75, 1.0],
        metallic_factor: 0.125,
        roughness_factor: 0.625,
        normal_scale: 1.0,
        alpha_mode: AlphaMode::Mask,
        alpha_cutoff: 0.375,
        double_sided: true,
        translucent_foliage: true,
        uv_tiling: [2.0, 3.0],
        terrain_distorted: true,
    }
}

#[test]
fn portable_version_two_round_trip_preserves_authored_material() {
    let material = material();
    let bytes = material.encode().unwrap();
    assert_eq!(&bytes[..8], b"RDMATL01");
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 2);
    assert_eq!(MaterialArtifact::decode(&bytes).unwrap(), material);
    assert_eq!(
        material.dependencies(),
        vec![
            id("fixture.texture.base"),
            id("fixture.texture.normal"),
            id("fixture.texture.surface")
        ]
    );
}

#[test]
fn invalid_material_factors_and_tiling_are_rejected() {
    let mut value = material();
    value.base_color_factor[0] = f32::NAN;
    assert!(matches!(
        value.encode(),
        Err(MaterialError::InvalidBaseColor)
    ));
    let mut value = material();
    value.metallic_factor = 1.5;
    assert!(matches!(
        value.encode(),
        Err(MaterialError::InvalidPbrFactor)
    ));
    let mut value = material();
    value.uv_tiling[1] = 0.0;
    assert!(matches!(
        value.encode(),
        Err(MaterialError::InvalidUvTiling)
    ));
    let mut value = material();
    value.alpha_cutoff = -0.1;
    assert!(matches!(
        value.encode(),
        Err(MaterialError::InvalidAlphaCutoff)
    ));
}

#[test]
fn decoder_rejects_corruption_versions_and_trailing_container_bytes() {
    let bytes = material().encode().unwrap();
    for end in 0..bytes.len() {
        assert!(MaterialArtifact::decode(&bytes[..end]).is_err());
    }
    let mut corrupt = bytes.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(matches!(
        MaterialArtifact::decode(&corrupt),
        Err(MaterialError::HashMismatch)
    ));
    let mut unknown = bytes.clone();
    unknown[8..12].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        MaterialArtifact::decode(&unknown),
        Err(MaterialError::UnsupportedVersion(99))
    ));
    let mut extended = bytes;
    extended.push(0);
    assert!(matches!(
        MaterialArtifact::decode(&extended),
        Err(MaterialError::InvalidExtent)
    ));
}

#[test]
fn removed_material_payload_version_is_rejected() {
    let mut bytes = material().encode().unwrap();
    bytes[8..12].copy_from_slice(&1_u32.to_le_bytes());
    assert!(matches!(
        MaterialArtifact::decode(&bytes),
        Err(MaterialError::UnsupportedVersion(1))
    ));
}
