use super::residents::insert;
use crate::terrain::streaming::{TerrainChunk, select_with_retired};
use glam::{Mat4, Vec3};
use worldrunner_render_plan::*;

pub(in crate::draw) fn terrain() -> SceneTerrain {
    SceneTerrain::new(SceneTerrainParts {
        columns: 257,
        rows: 257,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 1000,
        heights_millimeters: (0..257 * 257)
            .map(|i| if i % 7 == 0 { 3000 } else { 0 })
            .collect(),
        surface_indices: vec![0; 257 * 257],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
    .with_streaming_policy(Some(TerrainStreamingPolicy::new(8, 41, 64, 12000).unwrap()))
}

pub(in crate::draw) fn matrix() -> Mat4 {
    glam::camera::rh::proj::directx::perspective(60.0_f32.to_radians(), 1.0, 0.1, 1000.0)
        * glam::camera::rh::view::look_at_mat4(
            Vec3::new(32.0, -8.0, 8.0),
            Vec3::new(32.0, 32.0, 0.0),
            Vec3::Z,
        )
}

struct Settled {
    chunks: Vec<TerrainChunk>,
    uploads: Vec<usize>,
    statistics: crate::draw::TerrainStreamingStatistics,
}

fn settle(delay: usize, append_input: bool) -> Settled {
    let terrain = terrain();
    let policy = terrain.streaming_policy().unwrap();
    let mut residents = vec![(TerrainChunk::root(&terrain, policy), ())];
    let mut history = vec![residents[0].0];
    let mut retired = Vec::new();
    let mut uploads = Vec::new();
    for frame in 0..256 {
        retired.retain(|&(released, _)| released > frame);
        let current: Vec<_> = if append_input {
            history.clone()
        } else {
            residents.iter().map(|(key, ())| *key).collect()
        };
        let retained = retired.iter().map(|&(_, count)| count).sum();
        let selection =
            select_with_retired(&terrain, &current, matrix(), [128; 2], retained, &|_| true);
        assert!(current.len() + retained + selection.uploads.len() <= 41);
        let removed = current
            .iter()
            .filter(|key| !selection.leaves.contains(key))
            .count();
        residents.retain(|(key, ())| selection.leaves.contains(key));
        for chunk in &selection.uploads {
            insert(&mut residents, *chunk, ());
        }
        history.retain(|key| selection.leaves.contains(key));
        history.extend(&selection.uploads);
        uploads.push(selection.uploads.len());
        if removed > 0 {
            retired.push((frame + delay + 1, removed));
        }
        if selection.uploads.is_empty() && retired.is_empty() {
            let chunks: Vec<_> = residents.into_iter().map(|(key, ())| key).collect();
            return Settled {
                statistics: crate::draw::geometry_statistics::measure(
                    &chunks,
                    selection.observation,
                ),
                chunks,
                uploads,
            };
        }
    }
    panic!("terrain did not settle");
}

#[test]
fn resident_draw_order_is_independent_of_retirement_schedule() {
    let immediate = settle(0, false);
    let delayed = settle(4, false);
    let mut first = immediate.chunks.clone();
    let mut second = delayed.chunks.clone();
    first.sort();
    second.sort();
    assert_eq!(first, second, "settled leaf sets differ");
    assert_eq!(first.len(), 40);
    assert!(immediate.statistics.complete);
    assert_eq!(immediate.statistics, delayed.statistics);
    assert_ne!(immediate.uploads, delayed.uploads);
    assert_eq!(immediate.chunks, delayed.chunks, "draw order differs");
    assert!(immediate.chunks.is_sorted());
    for delay in [0, 4] {
        let append_history = settle(delay, true);
        assert_eq!(append_history.chunks, immediate.chunks);
        assert_eq!(append_history.statistics, immediate.statistics);
        assert_eq!(append_history.uploads, settle(delay, false).uploads);
    }
}

fn constrained_terrain() -> SceneTerrain {
    terrain().with_streaming_policy(Some(TerrainStreamingPolicy::new(8, 12, 4, 12000).unwrap()))
}

fn orthographic_matrix() -> Mat4 {
    glam::camera::rh::proj::directx::orthographic(-64.0, 64.0, -64.0, 64.0, 0.1, 1000.0)
        * glam::camera::rh::view::look_at_mat4(
            Vec3::new(128.0, 128.0, 100.0),
            Vec3::new(128.0, 128.0, 0.0),
            Vec3::Y,
        )
}

#[test]
fn equal_error_admission_is_independent_of_resident_permutation() {
    let terrain = constrained_terrain();
    let root = TerrainChunk::root(&terrain, terrain.streaming_policy().unwrap());
    let mut current = root.children().unwrap();
    let matrix = orthographic_matrix();
    let errors: Vec<_> = current
        .iter()
        .map(|node| crate::terrain::streaming::error(&terrain, *node, matrix, [128; 2]))
        .collect();
    assert_eq!(errors, vec![16.0; 4]);
    for expected_count in [7, 10] {
        let expected = select_with_retired(&terrain, &current, matrix, [128; 2], 0, &|_| true);
        assert_eq!(expected.leaves.len(), expected_count);
        assert_eq!(expected.uploads.len(), 4);
        for _ in 0..current.len() {
            current.rotate_left(1);
            for reverse in [false, true] {
                let mut permuted = current.clone();
                if reverse {
                    permuted.reverse();
                }
                let actual =
                    select_with_retired(&terrain, &permuted, matrix, [128; 2], 0, &|_| true);
                assert_eq!(actual.uploads, expected.uploads, "upload admission differs");
                assert_eq!(actual.leaves, expected.leaves, "ordered leaves differ");
                assert_eq!(actual.observation, expected.observation);
            }
        }
        current = expected.leaves;
    }
}

fn eight_step_schedule(sorted: bool) -> Vec<(Vec<TerrainChunk>, Vec<TerrainChunk>)> {
    let terrain = constrained_terrain();
    let mut current = vec![TerrainChunk::root(
        &terrain,
        terrain.streaming_policy().unwrap(),
    )];
    let mut frames = Vec::new();
    for _ in 0..8 {
        let selection = select_with_retired(
            &terrain,
            &current,
            orthographic_matrix(),
            [128; 2],
            0,
            &|_| true,
        );
        assert!(current.len() + selection.uploads.len() <= 12);
        current.retain(|chunk| selection.leaves.contains(chunk));
        current.extend(selection.uploads);
        if sorted {
            current.sort();
        }
        let mut leaves = current.clone();
        leaves.sort();
        let mut draws = Vec::new();
        for chunk in &current {
            insert(&mut draws, *chunk, ());
        }
        frames.push((leaves, draws.into_iter().map(|(chunk, ())| chunk).collect()));
    }
    frames
}

#[test]
fn constrained_eight_step_leaf_sets_and_draws_ignore_storage_history() {
    let append_history = eight_step_schedule(false);
    let sorted_history = eight_step_schedule(true);
    assert_eq!(append_history.last().unwrap().0.len(), 10);
    for ((append_leaves, append_draws), (sorted_leaves, sorted_draws)) in
        append_history.iter().zip(&sorted_history)
    {
        assert_eq!(append_leaves, sorted_leaves, "admitted leaf sets differ");
        assert_eq!(append_draws, sorted_draws, "ordered draws differ");
        assert_eq!(append_leaves, append_draws);
    }
}
