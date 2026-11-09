#[test]
fn geometry_observations_retain_selected_fine_leaves_in_hysteresis_band() {
    let terrain = terrain(5);
    let children = root(&terrain).children().unwrap();
    let matrix = ortho(90.0, [32.0, 32.0, 100.0]);
    let plan = select(&terrain, &children, matrix, [128; 2]);
    let stats = crate::draw::geometry_statistics::measure(&plan.leaves, plan.observation);
    let mut expected = children;
    expected.sort();
    assert_eq!(plan.leaves, expected);
    assert!(plan.uploads.is_empty());
    assert_eq!(stats.selected_leaves, 4);
    assert_eq!(stats.drawn_at_detail, 4);
    assert_eq!(stats.unresolved_leaves, 0);
    assert!(stats.complete);
}

#[test]
fn geometry_observations_report_retirement_blocked_merge_until_admitted() {
    let terrain = terrain(5);
    let children = root(&terrain).children().unwrap();
    let matrix = ortho(110.0, [32.0, 32.0, 100.0]);
    let blocked = select_with_retired(&terrain, &children, matrix, [128; 2], 1, &|_| true);
    let before = crate::draw::geometry_statistics::measure(&blocked.leaves, blocked.observation);
    let mut expected = children;
    expected.sort();
    assert_eq!(blocked.leaves, expected);
    assert!(blocked.uploads.is_empty());
    assert_eq!(before.selected_leaves, 1);
    assert_eq!(before.unresolved_leaves, 1);
    assert!(!before.complete);
    let admitted = select_with_retired(&terrain, &blocked.leaves, matrix, [128; 2], 0, &|_| true);
    let after = crate::draw::geometry_statistics::measure(&admitted.leaves, admitted.observation);
    assert_eq!(admitted.leaves, [root(&terrain)]);
    assert_eq!(admitted.uploads, [root(&terrain)]);
    assert_eq!(after.unresolved_leaves, 0);
    assert!(after.complete);
    assert_ne!(before.drawn, after.drawn);
}

#[test]
fn geometry_observations_distinguish_budget_coverage_from_threshold_completion() {
    use crate::draw::geometry_statistics::measure;
    let terrain = terrain(9);
    let matrix = ortho(8.0, [32.0, 32.0, 100.0]);
    let plan = select(&terrain, &[root(&terrain)], matrix, [128; 2]);
    let stats = measure(&plan.leaves, plan.observation);
    assert_eq!(stats.selected_leaves, 28);
    assert_eq!(stats.drawn_at_detail, 0);
    assert_eq!(stats.drawn_coarse, 4);
    assert_eq!(stats.resident_leaves, 4);
    assert_eq!(stats.unresolved_leaves, 28);
    assert!(!stats.complete);
}

#[test]
fn geometry_observations_settle_and_identify_drawn_windows_independent_of_order() {
    use crate::draw::geometry_statistics::measure;
    let terrain = terrain(1000);
    let matrix = ortho(64.0, [32.0, 32.0, 100.0]);
    let mut leaves = vec![root(&terrain)];
    for _ in 0..8 {
        leaves = select(&terrain, &leaves, matrix, [128; 2]).leaves;
    }
    let observation = observe(&terrain, &leaves, matrix, [128; 2]);
    let stats = measure(&leaves, observation);
    assert!(stats.complete);
    assert_eq!(stats.selected_leaves, stats.drawn_at_detail);
    assert_eq!(stats.unresolved_leaves, 0);
    leaves.reverse();
    assert_eq!(measure(&leaves, observation), stats);
    let distant = ortho(10000.0, [32.0, 32.0, 100.0]);
    let far = select(&terrain, &leaves, distant, [128; 2]);
    assert_ne!(measure(&far.leaves, far.observation).drawn, stats.drawn);
}

#[test]
fn geometry_observations_respect_terminal_and_offscreen_leaves() {
    use crate::draw::geometry_statistics::measure;
    let terrain = terrain(1000);
    let matrix = ortho(8.0, [-100.0, -100.0, 100.0]);
    let plan = select(&terrain, &[root(&terrain)], matrix, [128; 2]);
    let stats = measure(&plan.leaves, plan.observation);
    assert!(stats.complete);
    assert_eq!(stats.selected_leaves, 1);
    assert_eq!(stats.drawn_at_detail, 1);
}

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

fn budgeted(uploads: u32) -> SceneTerrain {
    terrain(1000).with_streaming_policy(Some(
        TerrainStreamingPolicy::new(8, 1000, uploads, 12000).unwrap(),
    ))
}

#[test]
fn unbuilt_children_keep_the_parent_and_are_requested_in_admission_order() {
    let terrain = budgeted(4);
    let leaves = [root(&terrain)];
    let matrix = ortho(64.0, [32.0, 32.0, 100.0]);
    let built = select(&terrain, &leaves, matrix, [128; 2]);
    let waiting = select_with_retired(&terrain, &leaves, matrix, [128; 2], 0, &|_| false);
    assert_eq!(built.uploads.len(), 4);
    assert_eq!(waiting.leaves, leaves);
    assert!(waiting.uploads.is_empty());
    assert!(waiting.held.is_empty());
    assert_eq!(waiting.unready, built.uploads);
    assert_eq!(waiting.observation.drawn_coarse, 1);
    assert_eq!(waiting.observation.unresolved_leaves, 4);
}

#[test]
fn one_unbuilt_child_holds_back_the_whole_replacement() {
    let terrain = budgeted(4);
    let leaves = [root(&terrain)];
    let matrix = ortho(64.0, [32.0, 32.0, 100.0]);
    let children = select(&terrain, &leaves, matrix, [128; 2]).uploads;
    let missing = children[3];
    let ready = |chunk: TerrainChunk| chunk != missing;
    let plan = select_with_retired(&terrain, &leaves, matrix, [128; 2], 0, &ready);
    assert_eq!(plan.leaves, leaves);
    assert!(plan.uploads.is_empty());
    assert_eq!(plan.unready, [missing]);
    assert_eq!(plan.held, children[..3]);
}

#[test]
fn built_children_are_admitted_as_before() {
    let terrain = budgeted(4);
    let leaves = [root(&terrain)];
    let matrix = ortho(64.0, [32.0, 32.0, 100.0]);
    let expected = select(&terrain, &leaves, matrix, [128; 2]);
    let plan = select_with_retired(&terrain, &leaves, matrix, [128; 2], 0, &|_| true);
    assert_eq!(plan.leaves, expected.leaves);
    assert_eq!(plan.uploads, expected.uploads);
    assert!(plan.unready.is_empty() && plan.held.is_empty());
}

#[test]
fn unbuilt_merge_keeps_the_children_until_the_parent_is_built() {
    let terrain = terrain(5);
    let children = root(&terrain).children().unwrap();
    let matrix = ortho(110.0, [32.0, 32.0, 100.0]);
    let waiting = select_with_retired(&terrain, &children, matrix, [128; 2], 0, &|_| false);
    let mut expected = children.clone();
    expected.sort();
    assert_eq!(waiting.leaves, expected);
    assert!(waiting.uploads.is_empty());
    assert_eq!(waiting.unready, [root(&terrain)]);
    let built = select_with_retired(&terrain, &children, matrix, [128; 2], 0, &|_| true);
    assert_eq!(built.leaves, [root(&terrain)]);
    assert_eq!(built.uploads, [root(&terrain)]);
}

#[test]
fn requests_stay_within_the_budget_that_would_admit_the_uploads() {
    let matrix = ortho(16.0, [16.0, 32.0, 100.0]);
    let terrain = budgeted(4);
    let mut leaves = vec![root(&terrain)];
    for _ in 0..3 {
        leaves = select(&terrain, &leaves, matrix, [128; 2]).leaves;
    }
    for retired in [0, 995, 1000] {
        let built = select_with_retired(&terrain, &leaves, matrix, [128; 2], retired, &|_| true);
        let waiting = select_with_retired(&terrain, &leaves, matrix, [128; 2], retired, &|_| false);
        assert!(waiting.unready.len() <= 4, "{}", waiting.unready.len());
        assert_eq!(waiting.unready, built.uploads);
        assert_eq!(waiting.leaves, leaves);
    }
    let blocked = select_with_retired(&terrain, &leaves, matrix, [128; 2], 1000, &|_| false);
    assert!(blocked.unready.is_empty());
}

#[test]
fn replacements_past_the_upload_budget_are_prefetched_without_being_admitted() {
    let matrix = ortho(16.0, [16.0, 32.0, 100.0]);
    let terrain = budgeted(4);
    let mut leaves = vec![root(&terrain)];
    for _ in 0..3 {
        leaves = select(&terrain, &leaves, matrix, [128; 2]).leaves;
    }
    let waiting = select_with_retired(&terrain, &leaves, matrix, [128; 2], 0, &|_| false);
    assert_eq!(waiting.unready.len(), 4);
    assert_eq!(waiting.prefetch.len(), 4);
    assert!(
        waiting
            .prefetch
            .iter()
            .all(|chunk| !waiting.unready.contains(chunk))
    );
    assert_eq!(waiting.leaves, leaves);
    let prefetched = waiting.prefetch.clone();
    let ready = |chunk: TerrainChunk| prefetched.contains(&chunk);
    let deferred = select_with_retired(&terrain, &leaves, matrix, [128; 2], 0, &ready);
    assert_eq!(deferred.unready, waiting.unready);
    assert!(deferred.prefetch.is_empty());
    assert_eq!(deferred.held, prefetched);
    assert!(deferred.uploads.is_empty());
    let blocked = select_with_retired(&terrain, &leaves, matrix, [128; 2], 1000, &|_| false);
    assert!(blocked.prefetch.is_empty());
}

/// Every chunk of the quadtree down to `depth` levels below `node`.
fn nodes(node: TerrainChunk, depth: u32, out: &mut Vec<TerrainChunk>) {
    out.push(node);
    if depth > 0
        && let Some(children) = node.children()
    {
        for child in children {
            nodes(child, depth - 1, out);
        }
    }
}

#[test]
fn nested_lookup_agrees_with_scanning_every_chunk() {
    let terrain = terrain(1000);
    let mut queries = Vec::new();
    nodes(root(&terrain), 4, &mut queries);
    queries.extend((0..40u32).map(|i| TerrainChunk {
        minimum: [(i * 7) % 33, (i * 13) % 33],
        maximum: [(i * 7) % 33 + i % 9, (i * 13) % 33 + i % 5],
        stride: 1 << (i % 4),
    }));
    let mut sets = vec![queries.clone()];
    for (width, eye) in [(16.0, [16.0, 32.0, 100.0]), (64.0, [4.0, 60.0, 100.0])] {
        let mut leaves = vec![root(&terrain)];
        for _ in 0..4 {
            leaves = select(&terrain, &leaves, ortho(width, eye), [128; 2]).leaves;
            sets.push(leaves.clone());
        }
    }
    for set in &sets {
        let nested = Nested::new(set);
        for query in &queries {
            let scanned = set.iter().any(|chunk| {
                chunk != query
                    && (0..2).all(|axis| {
                        query.minimum[axis] <= chunk.minimum[axis]
                            && query.maximum[axis] >= chunk.maximum[axis]
                    })
            });
            assert_eq!(nested.inside(*query), scanned, "{query:?}");
            assert_eq!(nested.has(*query), set.contains(query), "{query:?}");
        }
    }
}

fn covered_samples(leaves: &[TerrainChunk]) -> std::collections::BTreeMap<[u32; 2], usize> {
    let mut cells = std::collections::BTreeMap::new();
    for leaf in leaves {
        for y in leaf.minimum[1]..leaf.maximum[1] {
            for x in leaf.minimum[0]..leaf.maximum[0] {
                *cells.entry([x, y]).or_insert(0) += 1;
            }
        }
    }
    cells
}

#[test]
fn caster_leaves_tile_the_grid_and_coarsen_with_a_smaller_map() {
    let terrain = terrain(1000);
    let whole = root(&terrain);
    let light = ortho(48.0, [16.0, 16.0, 100.0]);
    let finest = converged(&terrain, light, 1 << 16);
    let mut previous: Option<Vec<TerrainChunk>> = None;
    for size in [2048, 512, 128, 32] {
        let leaves = caster_leaves(&terrain, light, size, &finest);
        let cells = covered_samples(&leaves);
        let expected =
            (whole.maximum[0] - whole.minimum[0]) * (whole.maximum[1] - whole.minimum[1]);
        assert_eq!(cells.len(), expected as usize, "size {size} leaves a hole");
        assert!(
            cells.values().all(|count| *count == 1),
            "size {size} overlaps"
        );
        if let Some(finer) = &previous {
            assert!(leaves.len() <= finer.len(), "size {size}");
            assert!(
                leaves.iter().all(|leaf| finer.iter().any(|fine| {
                    (0..2).all(|axis| {
                        leaf.minimum[axis] <= fine.minimum[axis]
                            && leaf.maximum[axis] >= fine.maximum[axis]
                    })
                })),
                "size {size} refines where a larger map did not"
            );
        }
        previous = Some(leaves);
    }
    assert!(previous.unwrap().len() < caster_leaves(&terrain, light, 2048, &finest).len());
}

#[test]
fn caster_leaves_match_a_camera_selection_converged_at_the_same_size() {
    let terrain = terrain(1000);
    let light = ortho(48.0, [16.0, 16.0, 100.0]);
    let mut leaves = vec![root(&terrain)];
    for _ in 0..64 {
        let next = select(&terrain, &leaves, light, [512, 512]);
        if next.uploads.is_empty() {
            break;
        }
        leaves = next.leaves;
    }
    assert_eq!(
        caster_leaves(&terrain, light, 512, &converged(&terrain, light, 1 << 16)),
        leaves
    );
}

/// The camera leaves a selection converges to from the root at `size` pixels square.
fn converged(terrain: &SceneTerrain, matrix: Mat4, size: u32) -> Vec<TerrainChunk> {
    let mut leaves = vec![root(terrain)];
    for _ in 0..64 {
        let next = select(terrain, &leaves, matrix, [size, size]);
        if next.uploads.is_empty() {
            break;
        }
        leaves = next.leaves;
    }
    leaves
}

#[test]
fn caster_leaves_are_never_finer_than_the_camera_leaves() {
    let terrain = terrain(1000);
    let light = ortho(48.0, [16.0, 16.0, 100.0]);
    let camera = converged(&terrain, ortho(64.0, [32.0, 32.0, 100.0]), 128);
    let leaves = caster_leaves(&terrain, light, 2048, &camera);
    let unclamped = caster_leaves(&terrain, light, 2048, &converged(&terrain, light, 1 << 16));
    assert!(
        leaves.len() < unclamped.len(),
        "{} of {}",
        leaves.len(),
        unclamped.len()
    );
    for leaf in &leaves {
        assert!(
            !camera.iter().any(|coarse| coarse != leaf
                && (0..2).all(|axis| coarse.minimum[axis] <= leaf.minimum[axis]
                    && coarse.maximum[axis] >= leaf.maximum[axis])),
            "{leaf:?} is finer than a camera leaf"
        );
    }
    assert_eq!(covered_samples(&leaves), covered_samples(&camera));
}
