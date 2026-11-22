//! Group culling decides for a whole group only what the per-placement tests decide for every
//! box inside it.
use glam::{Mat4, Vec3};

use super::super::slice_hull::{LightMap, SliceHull, reference as hull_reference};
use super::frustum::reference as frustum_reference;
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

/// The region the separate `excludes` and `contains` tests make.
fn region(excluded: bool, contained: bool) -> Region {
    if excluded {
        Region::Outside
    } else if contained {
        Region::Inside
    } else {
        Region::Straddles
    }
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
            let (excluded, contained) = (
                frustum_reference::excludes(&frustum, group),
                frustum_reference::contains(&frustum, group),
            );
            let (missed, covered) = (
                hull_reference::excludes(&hull, group, matrix),
                hull_reference::covers(&hull, group, matrix),
            );
            let light = LightMap::new(matrix);
            assert_eq!(frustum.region(group), region(excluded, contained));
            assert_eq!(hull.region(group, &light), region(missed, covered));
            for (count, decision) in decided
                .iter_mut()
                .zip([excluded, contained, missed, covered])
            {
                *count += usize::from(decision);
            }
            for inner in inner_boxes(&mut rng, group) {
                let (inside, overlapping) =
                    (frustum.intersects(inner), hull.overlaps(inner, &light));
                assert_eq!(inside, frustum_reference::intersects(&frustum, inner));
                assert_eq!(overlapping, hull_reference::overlaps(&hull, inner, matrix));
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

/// The bits of each coordinate, with every NaN as one value: an optimised build may give a
/// NaN another sign or payload, and no test of a box reads more of a NaN than that it is one.
fn bits(bounds: Bounds) -> [u32; 6] {
    let bits = |value: f32| {
        if value.is_nan() {
            f32::NAN.to_bits()
        } else {
            value.to_bits()
        }
    };
    [bounds.min, bounds.max]
        .map(|point| point.to_array().map(bits))
        .concat()
        .try_into()
        .unwrap()
}

/// Boxes at the extremes of float range, flat, inverted or not numbers, which a world never
/// holds but the tests must still decide as they always did.
fn odd_boxes(rng: &mut Rng) -> Vec<Bounds> {
    let values = [
        0.0,
        -0.0,
        1.0e-30,
        1.0e30,
        -1.0e30,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::MAX,
    ];
    let pick = |rng: &mut Rng| values[rng.index(values.len())];
    (0..200)
        .map(|_| Bounds {
            min: Vec3::new(pick(rng), pick(rng), pick(rng)),
            max: Vec3::new(pick(rng), pick(rng), pick(rng)),
        })
        .collect()
}

#[test]
fn prepared_tests_equal_the_unprepared_for_odd_boxes_and_hulls() {
    let mut rng = Rng::new(17);
    let mut hulls = vec![
        SliceHull::new(&[], Mat4::IDENTITY, 0.0),
        SliceHull::new(&[Vec3::ZERO], Mat4::IDENTITY, 0.0),
        SliceHull::new(&[Vec3::ZERO, Vec3::X], Mat4::IDENTITY, 0.0),
    ];
    for count in [3, 4, 5, 9] {
        let corners: Vec<Vec3> = (0..count)
            .map(|_| Vec3::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0), 0.0))
            .collect();
        hulls.push(SliceHull::new(&corners, Mat4::IDENTITY, 0.05));
    }
    let mut boxes = odd_boxes(&mut rng);
    for _ in 0..300 {
        boxes.push(random_box(&mut rng, 3.0));
        boxes.push(random_box(&mut rng, 900.0));
    }
    for _ in 0..40 {
        let (matrix, hull) = random_views(&mut rng);
        hulls.push(hull);
        let frustum = Frustum::new(matrix);
        let light = LightMap::new(matrix);
        for &bounds in &boxes {
            assert_eq!(
                frustum.intersects(bounds),
                frustum_reference::intersects(&frustum, bounds),
                "{bounds:?}"
            );
            assert_eq!(
                frustum.region(bounds),
                region(
                    frustum_reference::excludes(&frustum, bounds),
                    frustum_reference::contains(&frustum, bounds)
                ),
                "{bounds:?}"
            );
            assert_eq!(
                bits(light.transformed(bounds)),
                bits(bounds.transformed(matrix))
            );
            assert_eq!(
                bits(light.enclosing(bounds)),
                bits(bounds.transformed_enclosing(matrix))
            );
            for hull in &hulls {
                assert_eq!(
                    hull.overlaps(bounds, &light),
                    hull_reference::overlaps(hull, bounds, matrix),
                    "{bounds:?} {hull:?}"
                );
                assert_eq!(
                    hull.region(bounds, &light),
                    region(
                        hull_reference::excludes(hull, bounds, matrix),
                        hull_reference::covers(hull, bounds, matrix)
                    ),
                    "{bounds:?} {hull:?}"
                );
            }
        }
    }
}
