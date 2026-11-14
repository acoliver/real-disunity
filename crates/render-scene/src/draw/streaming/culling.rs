use crate::draw::visibility::{Bounds, Frustum};
use crate::terrain::streaming::{TerrainChunk, chunk_bounds};
use glam::Mat4;
use worldrunner_render_plan::SceneTerrain;

pub(super) fn keep(terrain: &SceneTerrain, matrix: Mat4) -> impl Fn(&TerrainChunk) -> bool + '_ {
    let frustum = Frustum::new(matrix);
    move |chunk| {
        let [min, max] = chunk_bounds(terrain, *chunk);
        frustum.intersects(Bounds { min, max })
    }
}

/// Borrows only submitted leaves; residency and streaming work stay untouched.
pub(super) fn visible<'a, T>(
    terrain: &SceneTerrain,
    matrix: Mat4,
    residents: &'a [(TerrainChunk, T)],
) -> Vec<&'a (TerrainChunk, T)> {
    let keep = keep(terrain, matrix);
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
        let residents = [(root(&terrain), ())];
        let fit = Mat4::from_scale(Vec3::new(0.001, 0.001, 0.01));
        assert_eq!(visible(&terrain, fit, &residents).len(), 1);
        for offset in [
            Vec3::X * -2.0,
            Vec3::X * 2.0,
            Vec3::Y * -2.0,
            Vec3::Y * 2.0,
            Vec3::Z * -2.0,
            Vec3::Z * 2.0,
        ] {
            let matrix = Mat4::from_translation(offset) * fit;
            assert!(visible(&terrain, matrix, &residents).is_empty(), "{offset}");
        }
    }

    #[test]
    fn skirt_and_maximum_height_touching_the_view_are_kept() {
        let terrain = terrain();
        let residents = [(root(&terrain), ())];
        let fit = Mat4::from_scale(Vec3::new(0.001, 0.001, 1.0));
        // Only the skirt bottom touches the far plane; every surface sample is beyond it.
        let skirt = Mat4::from_translation(Vec3::Z * 2.0) * fit;
        assert_eq!(visible(&terrain, skirt, &residents).len(), 1);
        assert!(
            visible(
                &terrain,
                Mat4::from_translation(Vec3::Z * 0.01) * skirt,
                &residents
            )
            .is_empty()
        );
        // Only the terrain-wide maximum touches the near plane.
        let peak = Mat4::from_translation(Vec3::Z * -3.0) * fit;
        assert_eq!(visible(&terrain, peak, &residents).len(), 1);
        assert!(
            visible(
                &terrain,
                Mat4::from_translation(Vec3::Z * -0.01) * peak,
                &residents
            )
            .is_empty()
        );
    }

    #[test]
    fn submission_is_a_subset_without_changing_residency_or_selection() {
        let terrain = terrain();
        let residents: Vec<_> = root(&terrain)
            .children()
            .unwrap()
            .into_iter()
            .map(|key| (key, ()))
            .collect();
        let before = residents.clone();
        let matrix = Mat4::from_scale(Vec3::new(0.01, 0.01, 0.1));
        let selection = crate::terrain::streaming::select(
            &terrain,
            &residents.iter().map(|(key, ())| *key).collect::<Vec<_>>(),
            matrix,
            [128; 2],
        );
        let submitted = visible(&terrain, matrix, &residents);
        assert!(!submitted.is_empty() && submitted.len() < residents.len());
        assert!(submitted.iter().all(|leaf| residents.contains(leaf)));
        assert_eq!(residents, before);
        let after = crate::terrain::streaming::select(
            &terrain,
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
        let keys = root(&terrain).children().unwrap();
        let residents: Vec<_> = keys.iter().map(|key| (*key, ())).collect();
        let matrix = Mat4::from_scale(Vec3::new(0.01, 0.01, 0.1));
        let observation = crate::terrain::streaming::observe(&terrain, &keys, matrix, [128; 2]);
        let drawn: Vec<_> = visible(&terrain, matrix, &residents)
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
}
