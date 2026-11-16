use crate::draw::visibility::{Bounds, Frustum};
use crate::terrain::streaming::{TerrainChunk, TerrainHeightBounds};
use glam::Mat4;
use worldrunner_render_plan::SceneTerrain;

pub(super) fn keep<'a>(
    terrain: &'a SceneTerrain,
    bounds: &'a TerrainHeightBounds,
    matrix: Mat4,
) -> impl Fn(&TerrainChunk) -> bool + 'a {
    let frustum = Frustum::new(matrix);
    move |chunk| {
        let [min, max] = bounds.chunk_bounds(terrain, *chunk);
        frustum.intersects(Bounds { min, max }.outward_rounded())
    }
}

/// Borrows only submitted leaves; residency and streaming work stay untouched.
pub(super) fn visible<'a, T>(
    terrain: &SceneTerrain,
    bounds: &TerrainHeightBounds,
    matrix: Mat4,
    residents: &'a [(TerrainChunk, T)],
) -> Vec<&'a (TerrainChunk, T)> {
    let keep = keep(terrain, bounds, matrix);
    residents.iter().filter(|(chunk, _)| keep(chunk)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::streaming::schedule_tests::terrain;
    use glam::Vec3;

    fn root(terrain: &SceneTerrain) -> TerrainChunk {
        TerrainChunk::root(terrain, terrain.streaming_policy().unwrap())
    }

    #[test]
    fn leaves_outside_each_of_the_six_planes_are_not_submitted() {
        let terrain = terrain();
        let bounds = TerrainHeightBounds::new(&terrain);
        let residents = [(root(&terrain), ())];
        let fit = Mat4::from_scale(Vec3::new(0.001, 0.001, 0.01));
        assert_eq!(visible(&terrain, &bounds, fit, &residents).len(), 1);
        for offset in [
            Vec3::X * -2.0,
            Vec3::X * 2.0,
            Vec3::Y * -2.0,
            Vec3::Y * 2.0,
            Vec3::Z * -2.0,
            Vec3::Z * 2.0,
        ] {
            let matrix = Mat4::from_translation(offset) * fit;
            assert!(
                visible(&terrain, &bounds, matrix, &residents).is_empty(),
                "{offset}"
            );
        }
    }

    #[test]
    fn skirt_and_maximum_height_touching_the_view_are_kept() {
        let terrain = terrain();
        let bounds = TerrainHeightBounds::new(&terrain);
        let residents = [(root(&terrain), ())];
        let fit = Mat4::from_scale(Vec3::new(0.001, 0.001, 1.0));
        // Only the skirt bottom touches the far plane; every surface sample is beyond it.
        let skirt = Mat4::from_translation(Vec3::Z * 2.0) * fit;
        assert_eq!(visible(&terrain, &bounds, skirt, &residents).len(), 1);
        assert!(
            visible(
                &terrain,
                &bounds,
                Mat4::from_translation(Vec3::Z * 0.01) * skirt,
                &residents
            )
            .is_empty()
        );
        // Only the terrain-wide maximum touches the near plane.
        let peak = Mat4::from_translation(Vec3::Z * -3.0) * fit;
        assert_eq!(visible(&terrain, &bounds, peak, &residents).len(), 1);
        assert!(
            visible(
                &terrain,
                &bounds,
                Mat4::from_translation(Vec3::Z * -0.01) * peak,
                &residents
            )
            .is_empty()
        );
    }

    #[test]
    fn translated_rotated_leaf_with_raster_visible_vertex_is_kept() {
        let source = terrain();
        let terrain = SceneTerrain::new(worldrunner_render_plan::SceneTerrainParts {
            columns: 129,
            rows: 129,
            origin_east_millimeters: 1_013_727,
            origin_north_millimeters: 0,
            sample_spacing_millimeters: 1000,
            heights_millimeters: (0..129 * 129)
                .map(|i| if i == 0 { 0 } else { 3000 })
                .collect(),
            surface_indices: vec![0; 129 * 129],
            surfaces: source.surfaces().to_vec(),
            lod_policy: source.lod_policy(),
        })
        .unwrap()
        .with_streaming_policy(source.streaming_policy());
        let bounds = TerrainHeightBounds::new(&terrain);
        let chunk = root(&terrain);
        let [min, max] = bounds.chunk_bounds(&terrain, chunk);
        assert_eq!(min, Vec3::new(1013.727, 0.0, -1.0));
        assert_eq!(max, Vec3::new(1141.727, 128.0, 3.0));
        let matrix = Mat4::from_cols_array_2d(&[
            [96.66466, -25.611_404, 0.0, 0.0],
            [25.611_404, 96.66466, 0.0, 0.0],
            [0.0, 0.0, 0.1, 0.0],
            [-97990.58, 25962.973, 0.1, 1.0],
        ]);
        let clip = matrix * Vec3::new(min.x, min.y, 0.0).extend(1.0);
        assert!(clip.x > -clip.w && clip.x < clip.w);
        assert!(clip.y > -clip.w && clip.y < clip.w);
        assert!(clip.z > 0.0 && clip.z < clip.w);
        let rows = matrix.transpose();
        let right = rows.w_axis - rows.x_axis;
        let support = right.truncate().dot((min + max) * 0.5)
            + right.w
            + right.truncate().abs().dot((max - min) * 0.5);
        assert!(support < 0.0, "unpadded support: {support}");
        assert!(!Frustum::new(matrix).intersects(Bounds { min, max }));
        assert_eq!(visible(&terrain, &bounds, matrix, &[(chunk, ())]).len(), 1);
    }

    #[test]
    fn submission_is_a_subset_without_changing_residency_or_selection() {
        let terrain = terrain();
        let bounds = TerrainHeightBounds::new(&terrain);
        let residents: Vec<_> = root(&terrain)
            .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
            .unwrap()
            .into_iter()
            .map(|key| (key, ()))
            .collect();
        let before = residents.clone();
        let matrix = Mat4::from_scale(Vec3::new(0.01, 0.01, 0.1));
        let selection = crate::terrain::streaming::select(
            &terrain,
            &bounds,
            &residents.iter().map(|(key, ())| *key).collect::<Vec<_>>(),
            matrix,
            [128; 2],
        );
        let submitted = visible(&terrain, &bounds, matrix, &residents);
        assert!(!submitted.is_empty() && submitted.len() < residents.len());
        assert!(submitted.iter().all(|leaf| residents.contains(leaf)));
        assert_eq!(residents, before);
        let after = crate::terrain::streaming::select(
            &terrain,
            &bounds,
            &residents.iter().map(|(key, ())| *key).collect::<Vec<_>>(),
            matrix,
            [128; 2],
        );
        assert_eq!(selection.leaves, after.leaves);
        assert_eq!(selection.uploads, after.uploads);
        assert_eq!(selection.unready, after.unready);
        assert_eq!(selection.prefetch, after.prefetch);
        assert_eq!(selection.observation, after.observation);
    }

    #[test]
    fn geometry_statistics_separate_resident_coverage_from_submitted_leaves() {
        let terrain = terrain();
        let bounds = TerrainHeightBounds::new(&terrain);
        let keys = root(&terrain)
            .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
            .unwrap();
        let residents: Vec<_> = keys.iter().map(|key| (*key, ())).collect();
        let matrix = Mat4::from_scale(Vec3::new(0.01, 0.01, 0.1));
        let observation =
            crate::terrain::streaming::observe(&terrain, &bounds, &keys, matrix, [128; 2]);
        let drawn: Vec<_> = visible(&terrain, &bounds, matrix, &residents)
            .into_iter()
            .map(|(key, ())| *key)
            .collect();
        let measure = crate::draw::geometry_statistics::measure;
        let all = measure(&keys, &keys, observation);
        let culled = measure(&keys, &drawn, observation);
        assert_eq!(all.selected_leaves, culled.selected_leaves);
        assert_eq!(all.resident_leaves, culled.resident_leaves);
        assert_eq!(all.unresolved_leaves, culled.unresolved_leaves);
        assert_eq!(all.complete, culled.complete);
        assert_eq!(culled.drawn.count, drawn.len());
        assert!(culled.drawn.count < culled.resident_leaves);
        let empty = measure(&keys, &[], observation);
        assert_eq!(empty.drawn.count, 0);
        assert_eq!(empty.resident_leaves, residents.len());
        assert_eq!(empty.selected_leaves, observation.selected_leaves);
    }

    #[test]
    fn a_peak_in_another_node_does_not_keep_a_flat_leaf_visible() {
        let source = terrain();
        let terrain = SceneTerrain::new(worldrunner_render_plan::SceneTerrainParts {
            columns: 129,
            rows: 129,
            origin_east_millimeters: 0,
            origin_north_millimeters: 0,
            sample_spacing_millimeters: 1000,
            heights_millimeters: (0..129 * 129)
                .map(|i| if i == 0 { 100_000 } else { 3000 })
                .collect(),
            surface_indices: vec![0; 129 * 129],
            surfaces: source.surfaces().to_vec(),
            lod_policy: source.lod_policy(),
        })
        .unwrap()
        .with_streaming_policy(source.streaming_policy());
        let bounds = TerrainHeightBounds::new(&terrain);
        let away = root(&terrain)
            .children(terrain.streaming_policy().unwrap().minimum_leaf_texels())
            .unwrap()[3];
        let matrix = Mat4::from_translation(Vec3::Z * -0.05)
            * Mat4::from_scale(Vec3::new(0.001, 0.001, 0.01));
        let [min, max] = crate::terrain::streaming::chunk_bounds(&terrain, away);
        assert!(Frustum::new(matrix).intersects(Bounds { min, max }.outward_rounded()));
        assert!(!keep(&terrain, &bounds, matrix)(&away));
    }
}
