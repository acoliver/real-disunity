use worldrunner_asset_id::LogicalAssetId;
use worldrunner_mesh::{MeshError, MeshPrimitive, MeshVertex, StaticMeshArtifact};

fn triangle() -> StaticMeshArtifact {
    StaticMeshArtifact {
        vertices: [[-2.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 0.0, 3.0]]
            .map(|position| MeshVertex {
                position,
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            })
            .to_vec(),
        indices: vec![0, 1, 2],
        primitives: vec![MeshPrimitive {
            first_index: 0,
            index_count: 3,
            material_slot: 0,
        }],
        material_slots: vec![LogicalAssetId::new("fixture.material.solid").unwrap()],
        bounds_min: [-2.0, 0.0, 0.0],
        bounds_max: [2.0, 0.0, 3.0],
        colors_rg: Some(vec![[0, 255], [64, 192], [128, 128]]),
    }
}

#[test]
fn portable_version_one_round_trip_preserves_geometry_colors_and_dependencies() {
    let mesh = triangle();
    let bytes = mesh.encode().unwrap();
    assert_eq!(&bytes[..8], b"RDMESH01");
    assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 1);
    assert_eq!(StaticMeshArtifact::decode(&bytes).unwrap(), mesh);
    assert_eq!(mesh.dependencies(), mesh.material_slots);
}

#[test]
fn decoder_rejects_corruption_unknown_versions_and_extent_changes() {
    let bytes = triangle().encode().unwrap();
    for end in 0..bytes.len() {
        assert!(StaticMeshArtifact::decode(&bytes[..end]).is_err());
    }
    let mut corrupted = bytes.clone();
    *corrupted.last_mut().unwrap() ^= 1;
    assert!(matches!(
        StaticMeshArtifact::decode(&corrupted),
        Err(MeshError::HashMismatch)
    ));
    let mut extended = bytes.clone();
    extended.push(0);
    assert!(matches!(
        StaticMeshArtifact::decode(&extended),
        Err(MeshError::InvalidArtifactExtent)
    ));
    let mut unknown = bytes;
    unknown[8..12].copy_from_slice(&99_u32.to_le_bytes());
    assert!(matches!(
        StaticMeshArtifact::decode(&unknown),
        Err(MeshError::UnsupportedArtifactVersion(99))
    ));
}

#[test]
fn invalid_geometry_never_encodes() {
    let mut mesh = triangle();
    mesh.indices[2] = 3;
    assert!(matches!(mesh.encode(), Err(MeshError::IndexOutOfBounds)));
    let mut mesh = triangle();
    mesh.vertices[0].position[0] = f32::NAN;
    assert!(matches!(mesh.encode(), Err(MeshError::NonFiniteData)));
    let mut mesh = triangle();
    mesh.colors_rg.as_mut().unwrap().pop();
    assert!(matches!(
        mesh.encode(),
        Err(MeshError::ColorChannelLengthMismatch)
    ));
    let mut mesh = triangle();
    mesh.primitives[0].first_index = 1;
    assert!(matches!(
        mesh.encode(),
        Err(MeshError::InvalidPrimitiveRange)
    ));
    let mut mesh = triangle();
    mesh.primitives[0].material_slot = 1;
    assert!(matches!(mesh.encode(), Err(MeshError::InvalidMaterialSlot)));
    let mut mesh = triangle();
    mesh.bounds_min[0] = 5.0;
    assert!(matches!(mesh.encode(), Err(MeshError::InvalidBounds)));
}
