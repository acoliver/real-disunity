use worldrunner_disunity_runtime::{MeshRun, PlayerMesh};

use crate::player_meshes::{MeshConversionError, SlotRun, player_scene_mesh, round_to_i64};

fn triangle() -> PlayerMesh {
    PlayerMesh {
        generation: 1,
        positions: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        normals: vec![],
        uvs: vec![],
        colors: vec![],
        indices: vec![0, 1, 2],
        submeshes: vec![MeshRun {
            slot: 0,
            first_index: 0,
            index_count: 3,
        }],
        left_out: 0,
    }
}

fn level(mesh: &PlayerMesh) -> worldrunner_render_plan::SceneMeshLod {
    player_scene_mesh(mesh).unwrap().mesh.lods()[0].clone()
}

#[test]
fn positions_are_whole_millimeters_with_the_meshes_own_axes() {
    let mut mesh = triangle();
    mesh.positions = vec![[1.5004, -2.0006, 0.25], [0., 0.0004, -0.0006], [3., 4., 5.]];
    let lod = level(&mesh);
    let positions: Vec<_> = lod
        .vertices()
        .iter()
        .map(|v| v.position_millimeters)
        .collect();
    assert_eq!(
        positions,
        vec![[1500, -2001, 250], [0, 0, -1], [3000, 4000, 5000]],
        "x stays x, y stays y and z stays z: the object's matrix carries the basis change"
    );
}

#[test]
fn triangle_winding_is_kept_because_the_camera_is_left_handed() {
    let mut mesh = triangle();
    mesh.indices = vec![0, 2, 1, 1, 2, 0];
    assert_eq!(
        level(&mesh).indices(),
        &[0, 2, 1, 1, 2, 0],
        "a left handed camera's front face is clockwise, as the mesh's own"
    );
}

#[test]
fn given_normals_and_uvs_become_micro_units() {
    let mut mesh = triangle();
    mesh.normals = vec![[0., 0., -1.], [0.6, 0.8, 0.], [0., 1., 0.]];
    mesh.uvs = vec![[0., 0.25], [1., 0.5], [-0.5, 2.]];
    let lod = level(&mesh);
    let vertices = lod.vertices();
    assert_eq!(vertices[0].normal_micro, [0, 0, -1_000_000]);
    assert_eq!(vertices[1].normal_micro, [600_000, 800_000, 0]);
    assert_eq!(vertices[0].texcoord_micro, [0, 250_000]);
    assert_eq!(vertices[2].texcoord_micro, [-500_000, 2_000_000]);
}

#[test]
fn missing_uvs_are_zero_and_missing_normals_follow_the_triangles() {
    let lod = level(&triangle());
    for vertex in lod.vertices() {
        assert_eq!(vertex.texcoord_micro, [0, 0]);
        assert_eq!(
            vertex.normal_micro,
            [0, 0, 1_000_000],
            "the normal is (b - a) x (c - a), the direction a clockwise face looks from"
        );
    }
}

#[test]
fn a_vertex_in_no_triangle_has_a_zero_normal() {
    let mut mesh = triangle();
    mesh.positions.push([5., 5., 5.]);
    let lod = level(&mesh);
    assert_eq!(lod.vertices()[3].normal_micro, [0, 0, 0]);
}

#[test]
fn shared_vertices_average_their_face_normals_by_area() {
    let mesh = PlayerMesh {
        positions: vec![[0., 0., 0.], [2., 0., 0.], [0., 2., 0.], [0., 0., 1.]],
        indices: vec![0, 1, 2, 0, 1, 3],
        submeshes: vec![],
        ..triangle()
    };
    let lod = level(&mesh);
    // Face one is (2,0,0) x (0,2,0) = (0,0,4); face two is (2,0,0) x (0,0,1) = (0,-2,0).
    // The shared origin averages their sum, (0, -2, 4), to (0, -1, 2) / sqrt(5).
    let normal = lod.vertices()[0].normal_micro;
    assert_eq!(normal[0], 0);
    assert!((normal[1] + 447_214).abs() <= 1, "{normal:?}");
    assert!((normal[2] - 894_427).abs() <= 1, "{normal:?}");
}

#[test]
fn sub_meshes_become_index_ranges_keeping_their_slot_numbers() {
    let mut mesh = triangle();
    mesh.indices = vec![0, 1, 2, 2, 1, 0, 0, 2, 1];
    mesh.submeshes = vec![
        MeshRun {
            slot: 0,
            first_index: 0,
            index_count: 6,
        },
        MeshRun {
            slot: 2,
            first_index: 6,
            index_count: 3,
        },
    ];
    let converted = player_scene_mesh(&mesh).unwrap();
    assert_eq!(
        converted.slots,
        vec![
            SlotRun {
                slot: 0,
                indices: 0..6
            },
            SlotRun {
                slot: 2,
                indices: 6..9
            },
        ]
    );
}

#[test]
fn a_mesh_with_no_triangles_cannot_be_drawn() {
    let mut mesh = triangle();
    mesh.indices.clear();
    mesh.submeshes.clear();
    assert!(matches!(
        player_scene_mesh(&mesh),
        Err(MeshConversionError::Scene(_))
    ));
}

#[test]
fn a_position_beyond_the_scene_range_is_named() {
    let mut mesh = triangle();
    mesh.positions[1] = [1e30, 0., 0.];
    let failed = player_scene_mesh(&mesh).unwrap_err();
    assert!(
        matches!(failed, MeshConversionError::OutOfRange(_)),
        "{failed:?}"
    );
}

#[test]
fn rounding_to_integers_is_exact_and_bounded() {
    assert_eq!(round_to_i64(0.4), Some(0));
    assert_eq!(round_to_i64(0.5), Some(1));
    assert_eq!(round_to_i64(-0.5), Some(-1));
    assert_eq!(round_to_i64(-1234.4), Some(-1234));
    assert_eq!(round_to_i64(4_503_599_627_370_496.0), Some(1 << 52));
    assert_eq!(round_to_i64(1e300), None);
    assert_eq!(round_to_i64(f64::NAN), None);
    assert_eq!(round_to_i64(f64::INFINITY), None);
}

#[test]
fn a_mesh_not_built_yet_is_held_back_without_a_report_and_read_once() {
    let mut meshes = crate::player_meshes::PlayerMeshes::default();
    let mut empty = triangle();
    empty.indices.clear();
    empty.submeshes.clear();
    meshes.take(7, &empty);
    assert!(meshes.get(7).is_none());
    assert!(meshes.reported.is_empty(), "an empty mesh is not an error");
    assert_eq!(
        meshes.seen.get(&7),
        Some(&1),
        "its generation is remembered"
    );
}

#[test]
fn a_changed_mesh_replaces_the_scene_mesh_and_an_unusable_one_is_reported_once() {
    let mut meshes = crate::player_meshes::PlayerMeshes::default();
    meshes.take(7, &triangle());
    assert_eq!(meshes.get(7).unwrap().slots.len(), 1);
    let mut broken = triangle();
    broken.generation = 2;
    broken.positions[0] = [1e30, 0., 0.];
    meshes.take(7, &broken);
    meshes.take(7, &broken);
    assert!(
        meshes.get(7).is_none(),
        "the failed update drops the old mesh"
    );
    assert_eq!(meshes.reported.len(), 1);
    assert_eq!(meshes.seen.get(&7), Some(&2));
}

#[test]
fn missing_normals_of_a_face_in_each_plane_follow_the_cross_product() {
    // (0,1,0) x (0,0,1) = (1,0,0), (0,0,1) x (1,0,0) = (0,1,0), (1,0,0) x (0,1,0) = (0,0,1).
    let cases = [
        (
            [[0., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            [1_000_000, 0, 0],
        ),
        (
            [[0., 0., 0.], [0., 0., 1.], [1., 0., 0.]],
            [0, 1_000_000, 0],
        ),
        (
            [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            [0, 0, 1_000_000],
        ),
    ];
    for (positions, expected) in cases {
        let mesh = PlayerMesh {
            positions: positions.to_vec(),
            ..triangle()
        };
        assert_eq!(level(&mesh).vertices()[0].normal_micro, expected);
    }
}
