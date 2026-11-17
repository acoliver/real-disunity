//! Group culling decides for a whole group only what the per-placement tests decide for every
//! box inside it.
use glam::{Mat4, Vec3};

use super::super::slice_hull::SliceHull;
use super::large_scene::Rng;
use super::*;

fn random_box(rng: &mut Rng, scale: f32) -> Bounds {
    let center = Vec3::new(
        rng.range(-scale, scale),
        rng.range(-scale, scale),
        rng.range(-scale * 0.1, scale * 0.1),
    );
    let half = Vec3::new(
        rng.range(0.0, 40.0),
        rng.range(0.0, 40.0),
        rng.range(0.0, 20.0),
    );
    Bounds {
        min: center - half,
        max: center + half,
    }
}

/// Boxes inside `group`, including ones that share its faces and degenerate ones.
fn inner_boxes(rng: &mut Rng, group: Bounds) -> Vec<Bounds> {
    let size = group.max - group.min;
    let mut boxes = vec![group];
    for _ in 0..24 {
        let a = group.min
            + size
                * Vec3::new(
                    rng.range(0.0, 1.0),
                    rng.range(0.0, 1.0),
                    rng.range(0.0, 1.0),
                );
        let b = group.min
            + size
                * Vec3::new(
                    rng.range(0.0, 1.0),
                    rng.range(0.0, 1.0),
                    rng.range(0.0, 1.0),
                );
        boxes.push(Bounds {
            min: a.min(b),
            max: a.max(b),
        });
        // Corner points of the group.
        let corner = Vec3::select(
            glam::BVec3::new(rng.chance(50), rng.chance(50), rng.chance(50)),
            group.min,
            group.max,
        );
        boxes.push(Bounds {
            min: corner,
            max: corner,
        });
    }
    boxes
}

fn random_views(rng: &mut Rng) -> (Mat4, SliceHull) {
    let frame = super::large_scene::views(
        Vec3::new(
            rng.range(-500.0, 500.0),
            rng.range(-500.0, 500.0),
            rng.range(1.0, 50.0),
        ),
        rng.range(0.0, std::f32::consts::TAU),
        rng.range(-0.6, 0.3),
    );
    let cascade = rng.index(4);
    let matrix = if rng.chance(50) {
        frame.camera
    } else {
        frame.receivers[cascade]
    };
    let hull = super::large_scene::slice_hull(&frame, cascade, frame.receivers[cascade]);
    (matrix, hull)
}

#[test]
fn group_rejection_and_acceptance_hold_for_every_box_inside_the_group() {
    let mut rng = Rng::new(3);
    let mut decided = [0_usize; 4];
    for _ in 0..400 {
        let (matrix, hull) = random_views(&mut rng);
        let frustum = Frustum::new(matrix);
        for _ in 0..40 {
            let group = random_box(&mut rng, 900.0);
            let (excluded, contained) = (frustum.excludes(group), frustum.contains(group));
            let (missed, covered) = (hull.excludes(group, matrix), hull.covers(group, matrix));
            for (count, decision) in decided
                .iter_mut()
                .zip([excluded, contained, missed, covered])
            {
                *count += usize::from(decision);
            }
            for inner in inner_boxes(&mut rng, group) {
                let (inside, overlapping) =
                    (frustum.intersects(inner), hull.overlaps(inner, matrix));
                assert!(!(excluded && inside), "{group:?} {inner:?}");
                assert!(!contained || inside, "{group:?} {inner:?}");
                assert!(!(missed && overlapping), "{group:?} {inner:?}");
                assert!(!covered || overlapping, "{group:?} {inner:?}");
            }
        }
    }
    // Every kind of decision occurs, so the assertions above were exercised.
    assert!(decided.iter().all(|&count| count > 100), "{decided:?}");
}
