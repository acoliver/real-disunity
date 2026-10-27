use super::streaming::*;
use super::{TerrainMesh, build_terrain_mesh, chunks};
use glam::{Mat4, Vec3};
use worldrunner_render_plan::*;

fn terrain(budget: u32) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 65,
        rows: 65,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: (0..65 * 65)
            .map(|i| if i % 7 == 0 { 3000 } else { 0 })
            .collect(),
        surface_indices: vec![0; 65 * 65],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(
        TerrainStreamingPolicy::new(8, budget, 64, 12000).unwrap(),
    ))
}
fn root(terrain: &SceneTerrain) -> TerrainChunk {
    TerrainChunk::root(terrain, terrain.streaming_policy().unwrap())
}
fn ortho(width: f32, eye: [f32; 3]) -> Mat4 {
    glam::camera::rh::proj::directx::orthographic(
        -width / 2.0,
        width / 2.0,
        -width / 2.0,
        width / 2.0,
        0.1,
        1000.0,
    ) * glam::camera::rh::view::look_at_mat4(Vec3::from(eye), Vec3::from(eye) - Vec3::Z, Vec3::Y)
}
fn reference(node: TerrainChunk, pixels_per_meter: f64, leaves: &mut Vec<TerrainChunk>) {
    if f64::from(node.stride) * pixels_per_meter > 12.0
        && let Some(children) = node.children()
    {
        for child in children {
            reference(child, pixels_per_meter, leaves);
        }
        return;
    }
    leaves.push(node);
}
#[test]
fn lod_matches_independent_orthographic_reference_at_several_cameras() {
    let terrain = terrain(1000);
    for (width, eye) in [
        (64.0, [32.0, 32.0, 100.0]),
        (256.0, [20.0, 40.0, 200.0]),
        (1024.0, [-60.0, 10.0, 300.0]),
    ] {
        let mut leaves = vec![root(&terrain)];
        for _ in 0..8 {
            leaves = select(&terrain, &leaves, ortho(width, eye), [128; 2]).leaves;
        }
        let mut expected = Vec::new();
        reference(root(&terrain), 128.0 / f64::from(width), &mut expected);
        leaves.sort();
        expected.sort();
        assert_eq!(leaves, expected);
    }
}
#[test]
fn budgets_hold_and_coarsening_evicts_children_without_holes() {
    let terrain = terrain(9);
    let mut leaves = vec![root(&terrain)];
    for _ in 0..8 {
        let plan = select(&terrain, &leaves, ortho(8.0, [32.0, 32.0, 100.0]), [128; 2]);
        assert!(leaves.len() + plan.uploads.len() <= 9);
        assert!(plan.uploads.len() <= 64);
        assert_eq!(
            plan.leaves
                .iter()
                .map(|n| u64::from(n.maximum[0] - n.minimum[0])
                    * u64::from(n.maximum[1] - n.minimum[1]))
                .sum::<u64>(),
            4096
        );
        leaves = plan.leaves;
    }
    assert!(leaves.len() > 1);
    let plan = select(
        &terrain,
        &leaves,
        ortho(10000.0, [32.0, 32.0, 100.0]),
        [128; 2],
    );
    assert_eq!(plan.leaves, [root(&terrain)]);
    assert_eq!(plan.uploads, [root(&terrain)]);
}
#[test]
fn disabling_policy_preserves_default_vertex_and_index_bytes() {
    let plain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; 4],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    let mut expected = Vec::new();
    for [x, y] in [[0.0_f32, 0.0_f32], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
        for value in [x, y, 0.0, -0.0, -0.0, 1.0, x / 10.0, y / 10.0] {
            expected.extend_from_slice(&value.to_le_bytes());
        }
        expected.extend_from_slice(&0_u32.to_le_bytes());
        expected.extend_from_slice(&1.0_f32.to_le_bytes());
        expected.extend_from_slice(&[0; 16]);
    }
    let policy = terrain(64).streaming_policy();
    let mesh = build_terrain_mesh(
        &plain
            .with_streaming_policy(policy)
            .with_streaming_policy(None),
    )
    .unwrap();
    assert_eq!(mesh.vertices, expected);
    assert_eq!(
        mesh.indices,
        [0_u32, 1, 3, 0, 3, 2]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>()
    );
}
fn positions(mesh: &TerrainMesh) -> Vec<Vec3> {
    mesh.vertices
        .chunks_exact(crate::geometry::VERTEX_BYTES)
        .map(|v| {
            Vec3::from_array(std::array::from_fn(|i| {
                f32::from_le_bytes(v[i * 4..i * 4 + 4].try_into().unwrap())
            }))
        })
        .collect()
}
fn hit(mesh: &TerrainMesh, origin: Vec3, direction: Vec3) -> bool {
    let vertices = positions(mesh);
    mesh.indices.chunks_exact(12).any(|indices| {
        let points: [Vec3; 3] = std::array::from_fn(|i| {
            vertices[u32::from_le_bytes(indices[i * 4..i * 4 + 4].try_into().unwrap()) as usize]
        });
        let edge1 = points[1] - points[0];
        let edge2 = points[2] - points[0];
        let cross_direction = direction.cross(edge2);
        let determinant = edge1.dot(cross_direction);
        if determinant.abs() < 0.000_001 {
            return false;
        }
        let offset = origin - points[0];
        let barycentric_u = offset.dot(cross_direction) / determinant;
        let cross_offset = offset.cross(edge1);
        let barycentric_v = direction.dot(cross_offset) / determinant;
        (0.0..=1.0).contains(&barycentric_u)
            && barycentric_v >= 0.0
            && barycentric_u + barycentric_v <= 1.000_01
            && (0.0..=0.02).contains(&(edge2.dot(cross_offset) / determinant))
    })
}
#[test]
fn ray_oracle_hits_skirts_across_mixed_lod_border() {
    let terrain = terrain(64);
    let siblings = root(&terrain).children().unwrap();
    let coarse = chunks::build(&terrain, siblings[0]).unwrap();
    let fine: Vec<_> = siblings[1]
        .children()
        .unwrap()
        .into_iter()
        .map(|n| chunks::build(&terrain, n).unwrap())
        .collect();
    for y in 1_u16..32 {
        let coordinate = f32::from(y) + 0.125;
        let top = [2_u16, 4]
            .into_iter()
            .map(|stride| {
                let start = y / stride * stride;
                let end = (start + stride).min(32);
                let height = |row: u16| {
                    crate::geometry::millimeter_to_meter(i64::from(
                        terrain.heights_millimeters()[usize::from(row) * 65 + 32],
                    ))
                };
                height(start)
                    + (height(end) - height(start)) * (coordinate - f32::from(start))
                        / f32::from(end - start)
            })
            .fold(0.0_f32, f32::max);
        for z in [-0.5, top / 2.0, top - 0.01] {
            let origin = Vec3::new(31.99, coordinate, z);
            assert!(
                hit(&coarse, origin, Vec3::X) || fine.iter().any(|mesh| hit(mesh, origin, Vec3::X)),
                "uncovered border at {origin:?}"
            );
        }
    }
}

#[test]
fn upload_budget_and_hysteresis_preserve_complete_leaf_sets() {
    let terrain = terrain(1000).with_streaming_policy(Some(
        TerrainStreamingPolicy::new(8, 1000, 4, 12000).unwrap(),
    ));
    let first = select(
        &terrain,
        &[root(&terrain)],
        ortho(64.0, [32.0, 32.0, 100.0]),
        [128; 2],
    );
    assert_eq!(first.uploads.len(), 4);
    let cached = select(
        &terrain,
        &first.leaves,
        ortho(90.0, [32.0, 32.0, 100.0]),
        [128; 2],
    );
    assert_eq!(cached.leaves, first.leaves);
    assert!(cached.uploads.is_empty());
    let merged = select(
        &terrain,
        &cached.leaves,
        ortho(110.0, [32.0, 32.0, 100.0]),
        [128; 2],
    );
    assert_eq!(merged.leaves, [root(&terrain)]);
    let mut leaves = merged.leaves;
    for x in [16.0, 48.0] {
        for _ in 0..12 {
            let plan = select(&terrain, &leaves, ortho(16.0, [x, 32.0, 100.0]), [128; 2]);
            assert!(plan.uploads.len() <= 4);
            assert!(leaves.len() + plan.uploads.len() <= 1000);
            assert_eq!(
                plan.leaves
                    .iter()
                    .map(|node| u64::from(node.maximum[0] - node.minimum[0])
                        * u64::from(node.maximum[1] - node.minimum[1]))
                    .sum::<u64>(),
                4096
            );
            leaves = plan.leaves;
        }
    }
}
