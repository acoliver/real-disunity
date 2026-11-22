//! How often a moving-camera frame refreshes each batch's cached bounds, and that skipping
//! the refreshes a frame does not need changes no fit and no selection.
use std::hash::{Hash, Hasher};

use super::large_scene::{FrameViews, slice_hull, walk, world};
use super::placement::CHECKS;
use super::*;
use worldrunner_render_plan::SceneTransform;

fn checks() -> usize {
    CHECKS.with(std::cell::Cell::get)
}

fn meshes_of(source: &worldrunner_render_plan::RenderScene) -> Vec<MeshSelection> {
    source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect()
}

/// One frame as the retained draw runs it: a caster fit per cascade, then the camera and the
/// cascades selected together. Returns the fits and a hash of every view's selection.
/// With `recheck`, every fit and the selection check every batch first, as each did before
/// frames shared one check.
fn frame(
    (instances, meshes): (&Instances, &[MeshSelection]),
    views: &FrameViews,
    recheck: bool,
) -> ([Mat4; 4], u64) {
    let fits = views.receivers.map(|receiver| {
        if recheck {
            instances.recheck_bounds();
        }
        caster_matrix(instances, meshes, None, receiver)
    });
    if recheck {
        instances.recheck_bounds();
    }
    let mut selections: Vec<(Mat4, Selection)> = vec![(views.camera, Selection::default())];
    for (index, &matrix) in fits.iter().enumerate() {
        selections.push((
            matrix,
            Selection {
                casters_only: true,
                slice: Some(slice_hull(views, index, matrix)),
                ..Selection::default()
            },
        ));
    }
    with_placements(
        instances,
        meshes,
        (views.eye_millimeters, views.fade_eye),
        &std::cell::RefCell::default(),
        |placements| {
            for (matrix, selection) in &mut selections {
                placements.select_each(&mut [(*matrix, selection)], 1);
            }
        },
    );
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (_, selection) in &selections {
        for range in &selection.ranges {
            (range.key, range.lod, range.slots.clone()).hash(&mut hasher);
        }
        selection.stats.considered.hash(&mut hasher);
        selection.stats.culled.hash(&mut hasher);
        selection.stats.drawn_per_lod.hash(&mut hasher);
    }
    (fits, hasher.finish())
}

#[test]
fn a_frame_checks_each_batch_once_and_unchanged_frames_not_at_all() {
    let source = world(3_000, 20, 5);
    let mut instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let batches = instances.batches.len();
    assert!(batches > 20, "the world spreads over many batches");

    let before = checks();
    frame((&instances, &meshes), &walk(0), false);
    assert_eq!(
        checks() - before,
        batches,
        "four cascade fits and the selection share one refresh of each batch"
    );

    let before = checks();
    for step in 1..6 {
        frame((&instances, &meshes), &walk(step), false);
    }
    assert_eq!(checks() - before, 0, "a moving camera edits no placement");

    let handle = instances.handles().next().unwrap();
    let (object, _) = instances.get(handle).unwrap();
    let moved = object.with_transform(SceneTransform {
        position_millimeters: [1_000, 2_000, 3_000],
        ..*object.transform()
    });
    instances.set_object(handle, moved).unwrap();
    let before = checks();
    frame((&instances, &meshes), &walk(6), false);
    assert_eq!(
        checks() - before,
        batches,
        "one refresh per batch after an edit"
    );
    let before = checks();
    frame((&instances, &meshes), &walk(7), false);
    assert_eq!(checks() - before, 0);
}

fn assert_same_frames(
    frames: std::ops::Range<u16>,
    memoized: (&Instances, &[MeshSelection]),
    reference: (&Instances, &[MeshSelection]),
) {
    for step in frames {
        let views = walk(step);
        let (fits, selected) = frame(memoized, &views, false);
        let (expected, expected_selected) = frame(reference, &views, true);
        assert_eq!(
            fits.map(|fit| fit.to_cols_array().map(f32::to_bits)),
            expected.map(|fit| fit.to_cols_array().map(f32::to_bits)),
            "frame {step}: cascade fits"
        );
        assert_eq!(selected, expected_selected, "frame {step}: selections");
    }
}

#[test]
fn skipping_unneeded_checks_changes_no_fit_and_no_selection() {
    let source = world(3_000, 20, 9);
    let mut warm = Instances::new(&source);
    let mut cold = Instances::new(&source);
    let mut warm_meshes = meshes_of(&source);
    let mut cold_meshes = meshes_of(&source);
    assert_same_frames(0..8, (&warm, &warm_meshes), (&cold, &cold_meshes));

    // Placements edited between frames, hidden, moved and removed. Each world numbers its
    // handles the same way.
    let handles: Vec<_> = warm.handles().zip(cold.handles()).collect();
    assert_eq!(handles.len(), warm.handles().count());
    for (index, &(handle, twin)) in handles.iter().enumerate().step_by(37) {
        let (object, visible) = warm.get(handle).unwrap();
        let moved = object.with_transform(SceneTransform {
            position_millimeters: [i64::try_from(index).unwrap() * 200 - 50_000, 30_000, 1_000],
            ..*object.transform()
        });
        for (instances, handle) in [(&mut warm, handle), (&mut cold, twin)] {
            instances.set_object(handle, moved).unwrap();
            if index % 3 == 0 {
                instances.set_visible(handle, !visible).unwrap();
            }
        }
    }
    assert_same_frames(8..14, (&warm, &warm_meshes), (&cold, &cold_meshes));
    for &(handle, twin) in handles.iter().step_by(101) {
        warm.remove(handle).unwrap();
        cold.remove(twin).unwrap();
    }
    assert_same_frames(14..18, (&warm, &warm_meshes), (&cold, &cold_meshes));

    // A mesh replaced by another selection, which forgets its batches' bounds.
    for (instances, meshes) in [(&mut warm, &mut warm_meshes), (&mut cold, &mut cold_meshes)] {
        meshes[3] = MeshSelection::new(&source.geometry().meshes()[4]);
        instances.forget_mesh_bounds(3);
    }
    assert_same_frames(18..22, (&warm, &warm_meshes), (&cold, &cold_meshes));
}
