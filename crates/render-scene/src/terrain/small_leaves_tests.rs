use super::streaming::{self, TerrainChunk, TerrainHeightBounds};
use crate::geometry::{VERTEX_BYTES, millimeters_to_meters};
use glam::{Mat4, Vec3};
use worldrunner_render_plan::*;

fn terrain(minimum: u32) -> SceneTerrain {
    terrain_with_side(minimum, 129)
}

fn terrain_with_side(minimum: u32, side: u32) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: side,
        rows: side,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: (0..side * side)
            .map(|i| if i % 7 == 0 { 3000 } else { -1000 })
            .collect(),
        surface_indices: vec![0; usize::try_from(side * side).unwrap()],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(
        TerrainStreamingPolicy::new(64, 1024, 64, 4000)
            .unwrap()
            .with_minimum_leaf_texels(minimum)
            .unwrap(),
    ))
}

fn root(terrain: &SceneTerrain) -> TerrainChunk {
    TerrainChunk::root(terrain, terrain.streaming_policy().unwrap())
}

fn span(node: TerrainChunk) -> u32 {
    node.maximum[0] - node.minimum[0]
}

fn nodes(terrain: &SceneTerrain) -> Vec<TerrainChunk> {
    let mut pending = vec![root(terrain)];
    let mut nodes = Vec::new();
    while let Some(node) = pending.pop() {
        nodes.push(node);
        if let Some(children) =
            node.children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
        {
            pending.extend(children);
        }
    }
    nodes
}

fn ortho() -> Mat4 {
    glam::camera::rh::proj::directx::orthographic(-64.0, 64.0, -64.0, 64.0, 0.1, 1000.0)
        * glam::camera::rh::view::look_at_mat4(
            Vec3::new(64.0, 64.0, 100.0),
            Vec3::new(64.0, 64.0, 0.0),
            Vec3::Y,
        )
}

fn settled(terrain: &SceneTerrain, matrix: Mat4, pixels: u32) -> Vec<TerrainChunk> {
    let bounds = TerrainHeightBounds::new(terrain);
    let mut leaves = vec![root(terrain)];
    for _ in 0..128 {
        let plan = streaming::select(terrain, &bounds, &leaves, matrix, [pixels; 2]);
        assert!(plan.uploads.len() <= 64);
        assert!(leaves.len() + plan.uploads.len() <= 1024);
        assert_eq!(
            leaves.iter().map(|node| span(*node).pow(2)).sum::<u32>(),
            128 * 128
        );
        if plan.leaves == leaves {
            assert_eq!(plan.observation.unresolved_leaves, 0);
            return leaves;
        }
        leaves = plan.leaves;
    }
    panic!("selection did not settle");
}

fn positions(mesh: &super::TerrainMesh) -> Vec<Vec3> {
    mesh.vertices
        .chunks_exact(VERTEX_BYTES)
        .map(|v| {
            Vec3::from_array(std::array::from_fn(|axis| {
                f32::from_le_bytes(v[axis * 4..axis * 4 + 4].try_into().unwrap())
            }))
        })
        .collect()
}

#[test]
fn selector_reaches_32_and_16_texels_without_crossing_the_minimum() {
    let terrain = terrain(16);
    for (pixels, expected) in [(1024, 32), (2048, 16), (4096, 16)] {
        let leaves = settled(&terrain, ortho(), pixels);
        assert!(leaves.iter().all(|node| span(*node) == expected));
        assert!(leaves.iter().all(|node| node.stride == 1));
    }
    let default = terrain.clone().with_streaming_policy(Some(
        TerrainStreamingPolicy::new(64, 1024, 64, 4000).unwrap(),
    ));
    assert!(
        settled(&default, ortho(), 4096)
            .iter()
            .all(|node| span(*node) == 64)
    );
}

#[test]
fn meshes_cap_cells_at_the_leaf_span_and_use_the_source_samples() {
    let terrain = terrain(16);
    let nodes = nodes(&terrain);
    for size in [16, 32, 64] {
        let node = *nodes
            .iter()
            .find(|node| span(**node) == size)
            .expect("leaf size exists");
        let mesh = super::chunks::build(&terrain, node).unwrap();
        assert_eq!(mesh.index_count / 3, 2 * size * size + 16 * size);
        let vertices = positions(&mesh);
        let side = usize::try_from(size + 1).unwrap();
        for y in 0..=size {
            for x in 0..=size {
                let column = node.minimum[0] + x;
                let row = node.minimum[1] + y;
                let z = terrain.heights_millimeters()[usize::try_from(row * 129 + column).unwrap()];
                let expected = millimeters_to_meters([
                    i64::from(column) * 1000,
                    i64::from(row) * 1000,
                    i64::from(z),
                ]);
                assert_eq!(
                    vertices[usize::try_from(y).unwrap() * side + usize::try_from(x).unwrap()],
                    expected
                );
            }
        }
        assert!(mesh.indices.chunks_exact(4).all(|index| {
            usize::try_from(u32::from_le_bytes(index.try_into().unwrap())).unwrap() < vertices.len()
        }));
    }
}

#[test]
fn continuous_footprint_is_finite_at_the_minimum_and_decreases_with_distance() {
    let terrain = terrain(16);
    let bounds = TerrainHeightBounds::new(&terrain);
    let nodes = nodes(&terrain);
    let small = *nodes
        .iter()
        .find(|node| span(**node) == 16)
        .expect("16 texels");
    let parent = *nodes
        .iter()
        .find(|node| span(**node) == 32)
        .expect("32 texels");
    let fine = streaming::error(&terrain, &bounds, small, ortho(), [1024; 2]);
    let coarse = streaming::error(&terrain, &bounds, parent, ortho(), [1024; 2]);
    assert!(fine > 0.0 && (coarse / fine - 2.0).abs() < 0.0001);
    let target = Vec3::new(
        f32::from(u16::try_from(small.minimum[0] + 8).unwrap()),
        f32::from(u16::try_from(small.minimum[1] + 8).unwrap()),
        0.0,
    );
    let mut previous = f64::INFINITY;
    for distance in [20.0, 40.0, 80.0, 160.0] {
        let matrix = glam::camera::rh::proj::directx::perspective(1.2, 1.0, 0.1, 1000.0)
            * glam::camera::rh::view::look_at_mat4(target + Vec3::Z * distance, target, Vec3::Y);
        let footprint = streaming::error(&terrain, &bounds, small, matrix, [1024; 2]);
        assert!(footprint.is_finite() && footprint > 0.0 && footprint < previous);
        previous = footprint;
    }
}

#[test]
fn cached_bounds_contain_every_small_mesh_vertex_and_skirt() {
    let terrain = terrain(16);
    let bounds = TerrainHeightBounds::new(&terrain);
    for node in nodes(&terrain) {
        let [low, high] = bounds.chunk_bounds(&terrain, node);
        let mesh = super::chunks::build(&terrain, node).unwrap();
        assert!(
            positions(&mesh)
                .iter()
                .all(|p| p.cmpge(low).all() && p.cmple(high).all())
        );
    }
}

#[test]
fn small_siblings_remain_atomic_under_readiness_and_upload_budgets() {
    let terrain = terrain(16).with_streaming_policy(Some(
        TerrainStreamingPolicy::new(64, 1024, 4, 4000)
            .unwrap()
            .with_minimum_leaf_texels(16)
            .unwrap(),
    ));
    let bounds = TerrainHeightBounds::new(&terrain);
    let parent = root(&terrain)
        .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
        .unwrap();
    let children = parent[0]
        .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
        .expect("32-texel children");
    let missing = children[0];
    let plan = streaming::select_with_retired(
        &terrain,
        &bounds,
        &parent,
        ortho(),
        [2048; 2],
        0,
        &|node| node != missing,
    );
    assert!(plan.leaves.contains(&parent[0]));
    assert!(children.iter().all(|node| !plan.leaves.contains(node)));
    assert!(plan.unready.contains(&missing));
    let ready = streaming::select(&terrain, &bounds, &parent, ortho(), [2048; 2]);
    assert_eq!(ready.uploads.len(), 4);
    assert_eq!(ready.leaves.len(), parent.len() + 3);
    assert_eq!(
        ready
            .leaves
            .iter()
            .map(|node| span(*node).pow(2))
            .sum::<u32>(),
        128 * 128
    );
}

#[test]
fn shadow_selection_handles_small_leaves_without_refining_past_the_camera() {
    let terrain = terrain(16);
    let camera = settled(&terrain, ortho(), 2048);
    let coarse = streaming::caster_leaves(&terrain, ortho(), 128, &camera);
    let fine = streaming::caster_leaves(&terrain, ortho(), 4096, &camera);
    assert_eq!(fine, camera);
    assert!(coarse.len() < fine.len());
    assert_eq!(
        coarse.iter().map(|node| span(*node).pow(2)).sum::<u32>(),
        128 * 128
    );
    let planes = super::planes::chunk_indices(&terrain);
    assert!(nodes(&terrain).iter().all(|node| planes.contains_key(node)));
}

#[test]
fn skirts_cover_a_128_texel_coarse_edge_beside_16_texel_leaves() {
    let terrain = terrain_with_side(16, 257);
    let minimum = terrain.streaming_policy().unwrap().minimum_leaf_texels();
    let coarse = root(&terrain).children(minimum).unwrap()[0];
    assert_eq!(coarse.stride, 2);
    let coarse = super::chunks::build(&terrain, coarse).unwrap();
    let fine: Vec<_> = nodes(&terrain)
        .into_iter()
        .filter(|node| span(*node) == 16 && node.minimum[0] == 128 && node.maximum[1] <= 128)
        .map(|node| super::chunks::build(&terrain, node).unwrap())
        .collect();
    assert_eq!(fine.len(), 8);
    for y in 1_u16..128 {
        let coordinate = f32::from(y) + 0.125;
        let top = [1_u16, 2]
            .into_iter()
            .map(|stride| {
                let start = y / stride * stride;
                let end = start + stride;
                let height = |row: u16| {
                    crate::geometry::millimeter_to_meter(i64::from(
                        terrain.heights_millimeters()[usize::from(row) * 257 + 128],
                    ))
                };
                height(start)
                    + (height(end) - height(start)) * (coordinate - f32::from(start))
                        / f32::from(stride)
            })
            .fold(f32::NEG_INFINITY, f32::max);
        for z in [-1.5, (top - 1.0) / 2.0, top - 0.01] {
            let origin = Vec3::new(127.99, coordinate, z);
            assert!(
                super::streaming_tests::hit(&coarse, origin, Vec3::X)
                    || fine
                        .iter()
                        .any(|mesh| super::streaming_tests::hit(mesh, origin, Vec3::X)),
                "uncovered mixed-size edge at {origin:?}"
            );
        }
    }
}

fn default_flat_terrain(side: u32) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: side,
        rows: side,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; usize::try_from(side * side).unwrap()],
        surface_indices: vec![0; usize::try_from(side * side).unwrap()],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(
        TerrainStreamingPolicy::new(64, 1024, 4, 4000).unwrap(),
    ))
}

fn default_policy_ortho() -> Mat4 {
    glam::camera::rh::proj::directx::orthographic(-256.0, 256.0, -256.0, 256.0, 0.1, 1000.0)
        * glam::camera::rh::view::look_at_mat4(
            Vec3::new(128.0, 128.0, 100.0),
            Vec3::new(128.0, 128.0, 0.0),
            Vec3::Y,
        )
}

#[test]
fn default_policy_admits_coordinate_first_refinement_on_odd_interval_grid() {
    let terrain = default_flat_terrain(258);
    let bounds = TerrainHeightBounds::new(&terrain);
    let current = root(&terrain).children(64).unwrap();
    for node in &current {
        assert_eq!(node.stride, 4);
        assert_eq!(
            streaming::error(&terrain, &bounds, *node, default_policy_ortho(), [1024; 2]).to_bits(),
            8.0_f64.to_bits()
        );
    }
    let plan = streaming::select(
        &terrain,
        &bounds,
        &current,
        default_policy_ortho(),
        [1024; 2],
    );
    let expected = [
        ([0, 0], [64, 64]),
        ([64, 0], [128, 64]),
        ([0, 64], [64, 128]),
        ([64, 64], [128, 128]),
    ]
    .map(|(minimum, maximum)| TerrainChunk {
        minimum,
        maximum,
        stride: 2,
    });
    assert_eq!(plan.uploads, expected);
    assert!(!plan.leaves.contains(&current[0]));
    assert!(current[1..].iter().all(|node| plan.leaves.contains(node)));
}

#[test]
fn default_policy_below_cap_root_keeps_stride_footprint() {
    let terrain = default_flat_terrain(33);
    let bounds = TerrainHeightBounds::new(&terrain);
    let node = root(&terrain);
    assert_eq!(span(node), 32);
    assert_eq!(node.stride, 1);
    assert_eq!(
        streaming::error(&terrain, &bounds, node, default_policy_ortho(), [1024; 2]).to_bits(),
        2.0_f64.to_bits()
    );
}

#[test]
fn default_policy_partial_terminal_window_keeps_stride_footprint() {
    let terrain = default_flat_terrain(130);
    let bounds = TerrainHeightBounds::new(&terrain);
    let parent = root(&terrain).children(64).unwrap()[3];
    let node = parent.children(64).unwrap()[3];
    assert_eq!(node.minimum, [96, 96]);
    assert_eq!(node.maximum, [129, 129]);
    assert_eq!(node.stride, 1);
    assert!(node.children(64).is_none());
    assert_eq!(
        streaming::error(&terrain, &bounds, node, default_policy_ortho(), [1024; 2]).to_bits(),
        2.0_f64.to_bits()
    );
}
