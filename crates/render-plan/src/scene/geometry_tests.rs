use super::*;

fn vertex() -> SceneVertex {
    SceneVertex {
        tangent_micro: None,
        position_millimeters: [0, 0, 0],
        normal_micro: [0, 1_000_000, 0],
        texcoord_micro: [0, 0],
    }
}

#[test]
fn triangle_lists_are_validated_inclusively() {
    let lod = SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], None)
        .expect("whole triangles validate");
    assert_eq!(lod.level_index(), 0);
    assert!(matches!(
        SceneMeshLod::new(0, vec![vertex()], vec![0, 0], None),
        Err(SceneValidationError::NonTriangleList { observed: 2, .. })
    ));
    assert!(matches!(
        SceneMeshLod::new(0, vec![vertex()], vec![0, 1, 0], None),
        Err(SceneValidationError::IndexOutOfRange {
            index: 1,
            limit: 1,
            ..
        })
    ));
    assert!(matches!(
        SceneMeshLod::new(0, Vec::new(), vec![0, 0, 0], None),
        Err(SceneValidationError::EmptyCollection { .. })
    ));
}

#[test]
fn lod_chains_must_be_contiguous_from_zero() {
    let near =
        SceneMeshLod::new(0, vec![vertex()], vec![0, 0, 0], None).expect("near level validates");
    let far = SceneMeshLod::new(
        1,
        vec![vertex()],
        vec![0, 0, 0],
        Some(BillboardPolicy::CrossQuads),
    )
    .expect("far level validates");
    assert_eq!(far.billboard(), Some(BillboardPolicy::CrossQuads));
    SceneMesh::with_distances(vec![near, far], vec![10_000, 40_000])
        .expect("contiguous chain validates");
    let drifted =
        SceneMeshLod::new(2, vec![vertex()], vec![0, 0, 0], None).expect("level payload validates");
    assert!(matches!(
        SceneMesh::new(vec![drifted]),
        Err(SceneValidationError::LodOrdering {
            position: 0,
            expected: 0,
            observed: 2,
            ..
        })
    ));
}

#[test]
fn transforms_require_normalized_rotation_and_positive_scale() {
    assert!(
        SceneTransform {
            position_millimeters: [1, 2, 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        }
        .validate()
        .is_ok()
    );
    // A real quantized yaw of 45 degrees about +Z. Its squared norm misses
    // the target by 532_889 micro-units squared, which no exact integer
    // quaternion can avoid, so it must be accepted.
    assert!(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation {
                x: 0,
                y: 0,
                z: 382_683,
                w: 923_880,
            },
            scale_millionths: 1_000_000,
        }
        .validate()
        .is_ok()
    );
    assert!(matches!(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation {
                x: 0,
                y: 0,
                z: 0,
                w: 999_000,
            },
            scale_millionths: 1_000_000,
        }
        .validate(),
        Err(SceneValidationError::RotationNotNormalized { .. })
    ));
    assert!(matches!(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 0,
        }
        .validate(),
        Err(SceneValidationError::RangeViolation { observed: 0, .. })
    ));
}
