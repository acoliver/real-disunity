use super::streaming::{self, TerrainChunk, TerrainHeightBounds};
use glam::{Mat4, Vec3};
use worldrunner_render_plan::*;

fn terrain(columns: u32, rows: u32, heights: Vec<i32>) -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns,
        rows,
        origin_east_millimeters: -1_013_727,
        origin_north_millimeters: 5_001,
        sample_spacing_millimeters: 603,
        heights_millimeters: heights,
        surface_indices: vec![0; (columns * rows) as usize],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(
        TerrainStreamingPolicy::new(4, 1024, 64, 4000).unwrap(),
    ))
}

fn root(terrain: &SceneTerrain) -> TerrainChunk {
    TerrainChunk::root(terrain, terrain.streaming_policy().unwrap())
}

fn nodes(terrain: &SceneTerrain) -> Vec<TerrainChunk> {
    let mut pending = vec![root(terrain)];
    let mut result = Vec::new();
    while let Some(node) = pending.pop() {
        result.push(node);
        if let Some(children) =
            node.children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
        {
            pending.extend(children);
        }
    }
    result
}

#[test]
fn a_node_away_from_a_spike_has_a_tight_surface_range() {
    let mut heights = vec![0; 17 * 17];
    heights[0] = 100_000;
    let terrain = terrain(17, 17, heights);
    let cache = TerrainHeightBounds::new(&terrain);
    let away = root(&terrain)
        .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
        .unwrap()[3];
    assert_eq!(cache.height_range_millimeters(away), [0, 0]);
    let bounds = cache.chunk_bounds(&terrain, away);
    assert_eq!(bounds[1].z.to_bits(), 0.0_f32.to_bits());
    assert_eq!(bounds[0].z.to_bits(), (-1.0_f32).to_bits());
}

#[test]
fn a_high_plateau_still_contains_the_terrain_wide_skirt_bottom() {
    let mut heights = vec![50_000; 17 * 17];
    heights[0] = -100_000;
    let terrain = terrain(17, 17, heights);
    let cache = TerrainHeightBounds::new(&terrain);
    let away = root(&terrain)
        .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
        .unwrap()[3];
    assert_eq!(cache.height_range_millimeters(away), [50_000; 2]);
    let [low, high] = cache.chunk_bounds(&terrain, away);
    assert_eq!(low.z.to_bits(), (-101.0_f32).to_bits());
    assert_eq!(high.z.to_bits(), 50.0_f32.to_bits());
    let mesh = super::chunks::build(&terrain, away).unwrap();
    for vertex in mesh.vertices.chunks_exact(crate::geometry::VERTEX_BYTES) {
        let z = f32::from_le_bytes(vertex[8..12].try_into().unwrap());
        assert!((low.z..=high.z).contains(&z));
    }
}

#[test]
fn every_nodes_bounds_contain_all_mesh_vertices_including_skirts() {
    for (columns, rows) in [(17, 17), (18, 11), (2, 34)] {
        let heights = (0..columns * rows)
            .map(|i| match i % 5 {
                0 => -123_457,
                1 => 987_653,
                _ => i * 17 - 30_001,
            })
            .collect();
        let terrain = terrain(
            u32::try_from(columns).unwrap(),
            u32::try_from(rows).unwrap(),
            heights,
        );
        let cache = TerrainHeightBounds::new(&terrain);
        let skirt = streaming::elevation(&terrain)[0] - 1.0;
        for node in nodes(&terrain) {
            let [low, high] = cache.chunk_bounds(&terrain, node);
            let mesh = super::chunks::build(&terrain, node).unwrap();
            let mut saw_skirt = false;
            for vertex in mesh.vertices.chunks_exact(crate::geometry::VERTEX_BYTES) {
                let point = Vec3::from_array(std::array::from_fn(|axis| {
                    f32::from_le_bytes(vertex[axis * 4..axis * 4 + 4].try_into().unwrap())
                }));
                assert!(
                    point.cmpge(low).all() && point.cmple(high).all(),
                    "{node:?}: {point}"
                );
                saw_skirt |= point.z.to_bits() == skirt.to_bits();
            }
            assert!(saw_skirt);
        }
    }
}

#[test]
fn ranges_are_exact_and_every_parent_is_the_union_of_its_children() {
    for (columns, rows) in [(17, 17), (18, 11), (2, 34)] {
        let heights = (0..columns * rows)
            .map(|i| if i % 7 == 0 { -i * 103 } else { i * 211 })
            .collect();
        let terrain = terrain(
            u32::try_from(columns).unwrap(),
            u32::try_from(rows).unwrap(),
            heights,
        );
        let cache = TerrainHeightBounds::new(&terrain);
        for node in nodes(&terrain) {
            let expected = (node.minimum[1]..=node.maximum[1])
                .flat_map(|y| (node.minimum[0]..=node.maximum[0]).map(move |x| (x, y)))
                .map(|(x, y)| terrain.heights_millimeters()[(y * terrain.columns() + x) as usize])
                .fold([i32::MAX, i32::MIN], |[low, high], z| {
                    [low.min(z), high.max(z)]
                });
            assert_eq!(cache.height_range_millimeters(node), expected, "{node:?}");
            if let Some(children) =
                node.children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
            {
                let union = children
                    .into_iter()
                    .map(|child| cache.height_range_millimeters(child))
                    .fold([i32::MAX, i32::MIN], |a, b| {
                        [a[0].min(b[0]), a[1].max(b[1])]
                    });
                assert_eq!(cache.height_range_millimeters(node), union);
            }
        }
    }
}

fn global_leaves(terrain: &SceneTerrain, node: TerrainChunk, matrix: Mat4) -> usize {
    if streaming::footprint(
        terrain,
        node,
        matrix,
        [128; 2],
        streaming::elevation(terrain),
    ) > 4.0
        && let Some(children) =
            node.children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
    {
        children
            .into_iter()
            .map(|child| global_leaves(terrain, child, matrix))
            .sum()
    } else {
        1
    }
}

#[test]
fn tight_ranges_avoid_splits_caused_by_a_distant_spike() {
    let mut heights = vec![0; 65 * 65];
    heights[0] = 100_000;
    let terrain = terrain(65, 65, heights);
    let cache = TerrainHeightBounds::new(&terrain);
    let away = root(&terrain)
        .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
        .unwrap()[3];
    let [low, high] = cache.chunk_bounds(&terrain, away);
    let target = Vec3::new((low.x + high.x) * 0.5, (low.y + high.y) * 0.5, 0.0);
    let matrix = glam::camera::rh::proj::directx::perspective(1.2, 1.0, 0.1, 1000.0)
        * glam::camera::rh::view::look_at_mat4(target + Vec3::Z * 40.0, target, Vec3::Y);
    let global = streaming::footprint(
        &terrain,
        away,
        matrix,
        [128; 2],
        streaming::elevation(&terrain),
    );
    let local = streaming::error(&terrain, &cache, away, matrix, [128; 2]);
    assert!(
        local.is_finite() && local < global,
        "local={local}, global={global}"
    );
    let mut leaves = vec![root(&terrain)];
    loop {
        let next = streaming::select(&terrain, &cache, &leaves, matrix, [128; 2]);
        if next.leaves == leaves {
            assert_eq!(next.observation.unresolved_leaves, 0);
            break;
        }
        leaves = next.leaves;
    }
    let local_count = leaves
        .iter()
        .filter(|leaf| {
            (0..2).all(|axis| {
                leaf.minimum[axis] >= away.minimum[axis] && leaf.maximum[axis] <= away.maximum[axis]
            })
        })
        .count();
    let global_count = global_leaves(&terrain, away, matrix);
    assert!(
        local_count > 0 && local_count < global_count,
        "local={local_count}, global={global_count}"
    );
    let finest: Vec<_> = nodes(&terrain)
        .into_iter()
        .filter(|node| {
            node.children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
                .is_none()
        })
        .collect();
    let casters = streaming::caster_leaves(&terrain, matrix, 128, &finest);
    assert_eq!(
        casters.len(),
        global_leaves(&terrain, root(&terrain), matrix)
    );
}
