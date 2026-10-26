//! Behavioural claims about packing governed geometry for the GPU.

use super::{
    BILLBOARD_FIXED, BILLBOARD_SPHERICAL, BILLBOARD_YAW_LOCKED, GeometryError, INSTANCE_STRIDE,
    VERTEX_STRIDE, instance_attributes, model_matrix, pack_indices, pack_instance, pack_vertices,
    vertex_attributes,
};
use glam::Vec3;
use worldrunner_render_plan::{
    BillboardPolicy, SceneMeshLod, SceneRotation, SceneTransform, SceneVertex,
};

/// Micro-units in one whole unit, matching the scene domain's scale.
const MICRO: i64 = 1_000_000;

/// Reads the little-endian `f32` at a byte offset.
fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[offset..offset + 4]);
    f32::from_le_bytes(word)
}

/// Builds a vertex with the given position, normal and texture coordinate.
fn vertex(position: [i64; 3], normal: [i64; 3], texcoord: [i64; 2]) -> SceneVertex {
    SceneVertex {
        tangent_micro: None,
        position_millimeters: position,
        normal_micro: normal,
        texcoord_micro: texcoord,
    }
}

/// Builds a single-triangle LOD around the supplied vertices.
fn lod(vertices: Vec<SceneVertex>) -> SceneMeshLod {
    SceneMeshLod::new(0, vertices, vec![0, 1, 2], None).expect("a triangle LOD is valid")
}

/// Builds a LOD whose three vertices share one normal and texture coordinate.
fn lod_with(normal: [i64; 3], texcoord: [i64; 2]) -> SceneMeshLod {
    lod(vec![
        vertex([0, 0, 0], normal, texcoord),
        vertex([1000, 0, 0], normal, texcoord),
        vertex([0, 1000, 0], normal, texcoord),
    ])
}

/// Builds a transform with the given position, rotation and scale.
fn transform(position: [i64; 3], rotation: SceneRotation, scale: u64) -> SceneTransform {
    SceneTransform {
        position_millimeters: position,
        rotation,
        scale_millionths: scale,
    }
}

#[test]
fn positions_are_converted_from_millimeters_to_meters() {
    // The whole pipeline downstream of this point works in metres; leaving
    // millimetres in would place everything a thousand times too far away.
    let packed = pack_vertices(&lod(vec![
        vertex([1_500, -2_000, 250], [0, 0, MICRO], [0, 0]),
        vertex([0, 0, 0], [0, 0, MICRO], [0, 0]),
        vertex([1000, 0, 0], [0, 0, MICRO], [0, 0]),
    ]))
    .expect("well-formed vertices pack");
    assert!((f32_at(&packed, 0) - 1.5).abs() < 1e-6);
    assert!((f32_at(&packed, 4) + 2.0).abs() < 1e-6);
    assert!((f32_at(&packed, 8) - 0.25).abs() < 1e-6);
}

#[test]
fn normals_are_normalized_rather_than_merely_rescaled() {
    // Authored normals are quantised to micro-units, so they arrive slightly
    // off unit length. Lighting divides by that length implicitly; an
    // unnormalised normal brightens or dims the surface by its own error.
    let packed = pack_vertices(&lod_with([MICRO, MICRO, 0], [0, 0])).expect("vertices pack");
    let normal = Vec3::new(
        f32_at(&packed, 12),
        f32_at(&packed, 16),
        f32_at(&packed, 20),
    );
    assert!((normal.length() - 1.0).abs() < 1e-6);
    assert!((normal.x - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
}

#[test]
fn a_zero_length_normal_is_reported_against_its_vertex() {
    // Normalising zero yields NaN, and NaN contaminates every lighting term it
    // reaches, so the surface turns black or garbage far from the cause.
    // Substituting an arbitrary direction would hide the fault behind
    // plausible-looking shading instead of surfacing it.
    let error = pack_vertices(&lod(vec![
        vertex([0, 0, 0], [0, 0, MICRO], [0, 0]),
        vertex([1000, 0, 0], [0, 0, 0], [0, 0]),
        vertex([0, 1000, 0], [0, 0, MICRO], [0, 0]),
    ]))
    .expect_err("a zero normal is reported");
    assert_eq!(error, GeometryError::DegenerateNormal { index: 1 });
}

#[test]
fn texture_coordinates_outside_the_unit_square_survive_packing() {
    // Tiled surfaces address themselves with coordinates well beyond one.
    // Clamping would collapse the repeat into a single stretched texel.
    let packed =
        pack_vertices(&lod_with([0, 0, MICRO], [8 * MICRO, -3 * MICRO])).expect("vertices pack");
    assert!((f32_at(&packed, 24) - 8.0).abs() < 1e-6);
    assert!((f32_at(&packed, 28) + 3.0).abs() < 1e-6);
}

#[test]
fn the_declared_stride_matches_the_bytes_actually_written() {
    // The stride and the attribute offsets are told to wgpu separately from
    // the packing loop. If they drift apart every vertex after the first is
    // read from the wrong place, which looks like corrupt geometry rather
    // than a layout mistake.
    let packed = pack_vertices(&lod_with([0, 0, MICRO], [0, 0])).expect("vertices pack");
    assert_eq!(packed.len() as u64, 3 * VERTEX_STRIDE);
    let attributes = vertex_attributes();
    assert_eq!(attributes[0].offset, 0);
    assert_eq!(attributes[1].offset, 12);
    assert_eq!(attributes[2].offset, 24);
    assert!(attributes[2].offset + 8 <= VERTEX_STRIDE);
}

#[test]
fn instance_attributes_tile_the_declared_instance_stride() {
    let attributes = instance_attributes();
    assert_eq!(attributes.len(), 5, "model matrix and instance tint");
    for (slot, attribute) in attributes.iter().take(4).enumerate() {
        assert_eq!(attribute.offset, slot as u64 * 16);
    }
    // A stride that outran its attributes would leave the tail unread, so the
    // last attribute must finish exactly where one instance ends.
    let last = attributes[4];
    assert_eq!(last.shader_location, 10);
    assert_eq!(last.offset, 64);
    assert_eq!(last.offset + last.format.size(), INSTANCE_STRIDE);
}

#[test]
fn indices_are_packed_as_little_endian_words() {
    let packed = pack_indices(&lod_with([0, 0, MICRO], [0, 0]));
    assert_eq!(packed.len(), 12);
    assert_eq!(&packed[0..4], &0u32.to_le_bytes());
    assert_eq!(&packed[4..8], &1u32.to_le_bytes());
    assert_eq!(&packed[8..12], &2u32.to_le_bytes());
}

#[test]
fn a_model_matrix_places_its_mesh_in_meters() {
    let matrix = model_matrix(&transform(
        [2_000, 3_000, -500],
        SceneRotation::IDENTITY,
        MICRO as u64,
    ));
    let placed = matrix.transform_point3(Vec3::ZERO);
    assert!((placed - Vec3::new(2.0, 3.0, -0.5)).length() < 1e-6);
}

#[test]
fn uniform_scale_is_applied_from_millionths() {
    let matrix = model_matrix(&transform([0, 0, 0], SceneRotation::IDENTITY, 2_500_000));
    let placed = matrix.transform_point3(Vec3::new(1.0, 0.0, 0.0));
    assert!((placed.x - 2.5).abs() < 1e-6);
}

#[test]
fn the_model_matrix_stays_in_the_scenes_z_up_frame() {
    // The basis change belongs at the view level and happens exactly once.
    // Applying it here too would apply it twice; applying it here instead
    // would leave vertices and placements in different frames, which is the
    // classic way to ship a world mirrored in a way nobody can localise.
    let matrix = model_matrix(&transform(
        [0, 0, 1_000],
        SceneRotation::IDENTITY,
        MICRO as u64,
    ));
    let placed = matrix.transform_point3(Vec3::ZERO);
    assert!(
        (placed.z - 1.0).abs() < 1e-6,
        "elevation must remain on z, not move to y"
    );
}

#[test]
fn the_model_matrix_transforms_normals_without_a_separate_normal_matrix() {
    // Uniform scale means the inverse transpose is the same rotation times a
    // positive constant, which renormalising discards. This pins that claim
    // so a future non-uniform scale cannot quietly invalidate it: under
    // uniform scale the transformed normal stays unit and stays perpendicular
    // to a transformed tangent.
    let rotation = SceneRotation {
        x: 0,
        y: 0,
        z: 707_107,
        w: 707_107,
    };
    let matrix = model_matrix(&transform([1_000, 0, 0], rotation, 4_000_000));
    let normal = matrix.transform_vector3(Vec3::X).normalize();
    let tangent = matrix.transform_vector3(Vec3::Y).normalize();
    assert!((normal.length() - 1.0).abs() < 1e-6);
    assert!(
        normal.dot(tangent).abs() < 1e-6,
        "uniform scale preserves perpendicularity, so no normal matrix is needed"
    );
}

#[test]
fn an_instance_is_packed_column_major_to_match_wgsl() {
    // WGSL reads a mat4x4 column by column. Packing rows would transpose every
    // placement, which reads on screen as objects rotated and displaced in a
    // way that still looks like geometry.
    let placement = transform([1_000, 2_000, 3_000], SceneRotation::IDENTITY, MICRO as u64);
    let packed = pack_instance(&placement);
    assert_eq!(packed.len() as u64, INSTANCE_STRIDE);
    let expected = model_matrix(&placement).to_cols_array();
    for (slot, value) in expected.iter().enumerate() {
        assert!((f32_at(&packed, slot * 4) - value).abs() < 1e-6);
    }
    // The translation occupies the final column, which is where a transposed
    // packing would not put it.
    assert!((f32_at(&packed, 48) - 1.0).abs() < 1e-6);
}

#[test]
fn the_billboard_mode_belongs_to_the_selected_mesh_level() {
    let attributes = vertex_attributes();
    assert_eq!(attributes[3].shader_location, 8);
    assert_eq!(attributes[3].offset, 32);
    assert_eq!(attributes[3].offset + attributes[3].format.size(), 36);
    assert_eq!(attributes[4].shader_location, 7);
    assert_eq!(attributes[4].offset, 36);
    assert_eq!(
        attributes[5].offset + attributes[5].format.size(),
        VERTEX_STRIDE
    );
    for (policy, expected) in [
        (None, BILLBOARD_FIXED),
        (Some(BillboardPolicy::CrossQuads), BILLBOARD_FIXED),
        (Some(BillboardPolicy::YawLocked), BILLBOARD_YAW_LOCKED),
        (Some(BillboardPolicy::Spherical), BILLBOARD_SPHERICAL),
    ] {
        let source = lod_with([0, 0, MICRO], [0, 0]);
        let level = SceneMeshLod::new(
            0,
            source.vertices().to_vec(),
            source.indices().to_vec(),
            policy,
        )
        .unwrap();
        let bytes = pack_vertices(&level).unwrap();
        for vertex in bytes.chunks_exact(crate::geometry::VERTEX_BYTES) {
            assert_eq!(
                u32::from_le_bytes(vertex[32..36].try_into().unwrap()),
                expected
            );
        }
    }
}

#[test]
fn a_rotation_within_quantization_tolerance_is_renormalized() {
    // The domain accepts rotations that are unit only to within a proven
    // quantisation bound. Feeding such a quaternion straight into a matrix
    // leaves a residual scale that compounds with the authored one.
    let rotation = SceneRotation {
        x: 0,
        y: 0,
        z: 500_000,
        w: 866_025,
    };
    let matrix = model_matrix(&transform([0, 0, 0], rotation, MICRO as u64));
    let length = matrix.transform_vector3(Vec3::X).length();
    assert!(
        (length - 1.0).abs() < 1e-5,
        "a renormalised rotation must not rescale the mesh, got {length}"
    );
    assert!(
        matrix.determinant() > 0.0,
        "a rotation must not mirror the mesh"
    );
}

#[test]
fn crossed_card_dimensions_preserve_both_authored_planes() {
    use worldrunner_render_plan::SceneCard;
    let positions = [
        [-1000, 0, 0],
        [1000, 0, 2000],
        [0, -1000, 0],
        [0, 1000, 2000],
    ];
    let vertices = positions.map(|position| SceneVertex {
        tangent_micro: None,
        position_millimeters: position,
        normal_micro: [0, -1_000_000, 0],
        texcoord_micro: [0; 2],
    });
    let lod = SceneMeshLod::new(
        0,
        vertices.to_vec(),
        vec![0, 1, 2, 1, 2, 3],
        Some(BillboardPolicy::CrossQuads),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::CrossQuads, [4000, 6000], 500_000).unwrap())
    .unwrap();
    let bytes = pack_vertices(&lod).unwrap();
    let actual: Vec<[f32; 3]> = bytes
        .chunks_exact(super::VERTEX_BYTES)
        .map(|vertex| {
            std::array::from_fn(|axis| {
                f32::from_le_bytes(vertex[axis * 4..axis * 4 + 4].try_into().unwrap())
            })
        })
        .collect();
    assert_eq!(
        actual,
        [
            [-2.0, 0.0, 0.0],
            [2.0, 0.0, 6.0],
            [0.0, -2.0, 0.0],
            [0.0, 2.0, 6.0]
        ]
    );
}

#[test]
fn card_fitting_inverse_transposes_nonuniform_authored_normals() {
    use worldrunner_render_plan::SceneCard;
    let vertices = [[-1000, 0, 0], [1000, 0, 0], [1000, 0, 2000]].map(|position| SceneVertex {
        tangent_micro: None,
        position_millimeters: position,
        normal_micro: [0, -1_000_000, 1_000_000],
        texcoord_micro: [0; 2],
    });
    let lod = SceneMeshLod::new(
        0,
        vertices.to_vec(),
        vec![0, 1, 2],
        Some(BillboardPolicy::CrossQuads),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::CrossQuads, [2000, 6000], 1_000_000).unwrap())
    .unwrap();
    let bytes = pack_vertices(&lod).unwrap();
    let normal = Vec3::from_array(std::array::from_fn(|axis| {
        f32::from_le_bytes(bytes[12 + axis * 4..16 + axis * 4].try_into().unwrap())
    }));
    assert!(
        (normal - Vec3::new(0.0, -1.0, 1.0 / 3.0).normalize()).length() < 1e-7,
        "{normal:?}"
    );
}

#[test]
fn stored_tangents_pack_signed_frames_and_reject_degeneracy() {
    for handedness in [-MICRO, MICRO] {
        let mut vertex = vertex([0; 3], [0, 0, MICRO], [0; 2]);
        vertex.tangent_micro = Some([2 * MICRO, 0, MICRO, handedness]);
        let packed = pack_vertices(&lod(vec![vertex; 3])).unwrap();
        assert_eq!(&packed[40..44], &1.0_f32.to_le_bytes());
        assert_eq!(&packed[44..48], &0.0_f32.to_le_bytes());
        assert_eq!(&packed[48..52], &0.0_f32.to_le_bytes());
        assert_eq!(
            &packed[52..56],
            &if handedness < 0 { -1.0_f32 } else { 1.0_f32 }.to_le_bytes()
        );
    }
    for tangent in [[0, 0, MICRO, MICRO], [0; 4], [MICRO, 0, 0, 0]] {
        let mut vertex = vertex([0; 3], [0, 0, MICRO], [0; 2]);
        vertex.tangent_micro = Some(tangent);
        assert_eq!(
            pack_vertices(&lod(vec![vertex; 3])),
            Err(GeometryError::InvalidTangent { index: 0 })
        );
    }
}

#[test]
fn non_axis_aligned_parallel_tangents_are_rejected_before_orthogonalization() {
    for normal in [[MICRO; 3], [MICRO, 2 * MICRO, 3 * MICRO]] {
        let mut vertex = vertex([0; 3], normal, [0; 2]);
        vertex.tangent_micro = Some([normal[0], normal[1], normal[2], MICRO]);
        assert_eq!(
            pack_vertices(&lod(vec![vertex; 3])),
            Err(GeometryError::InvalidTangent { index: 0 })
        );
    }
}

#[test]
fn nonuniform_card_fit_cannot_admit_a_parallel_authored_tangent() {
    use worldrunner_render_plan::SceneCard;
    let vertices = [[-1000, 0, 0], [1000, 0, 0], [1000, 0, 2000]].map(|position| {
        let mut vertex = vertex(position, [MICRO; 3], [0; 2]);
        vertex.tangent_micro = Some([MICRO; 4]);
        vertex
    });
    let lod = SceneMeshLod::new(
        0,
        vertices.to_vec(),
        vec![0, 1, 2],
        Some(BillboardPolicy::CrossQuads),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::CrossQuads, [2000, 6000], 1_000_000).unwrap())
    .unwrap();
    assert_eq!(
        pack_vertices(&lod),
        Err(GeometryError::InvalidTangent { index: 0 })
    );
}

#[test]
fn absent_tangents_preserve_the_stock_vertex_prefix_and_zero_the_new_attribute() {
    let packed = pack_vertices(&lod_with([0, 0, MICRO], [0; 2])).unwrap();
    assert_eq!(vertex_attributes()[5].offset, 40);
    assert_eq!(vertex_attributes()[5].shader_location, 11);
    for vertex in packed.chunks_exact(super::VERTEX_BYTES) {
        assert_eq!(&vertex[40..56], &[0; 16]);
    }
}
