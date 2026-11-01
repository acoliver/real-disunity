//! The depth and motion reconstruction of the temporal stage on lavapipe, through
//! `taa_reconstruct_test_rig`. Depths and motions are chosen so every expectation is exact in f32
//! and in the f16 velocity format (motions are multiples of 1/16 or 1/8 of a unit), so values are
//! compared exactly, or with a tolerance far below the smallest difference a failure would make.
//!
//! A raster depth attachment clamps what a fragment writes to 0..1, so a negative, infinite or NaN
//! depth reaches the stage as whatever the hardware stores. Those cases check that nothing that
//! reaches the stage is scattered or turned into NaN, not which clamp the hardware applies.
//! The rig paints the motion input and the depth input in separate passes, because a backend may
//! drop a fragment's colour when its depth is not finite; the motion input of those cases is
//! then the same on every backend.
use crate::TemporalCamera;
use crate::taa_reconstruct_test_rig::{Frame, run};

const INF_BITS: u32 = 0x7f80_0000;
/// Largest relative rounding error of a normal f16 is 2^-11; the bound leaves a factor two.
const F16_RELATIVE: f32 = 1.0 / 1024.0;

fn frame(size: [u32; 2], body: &str) -> Frame {
    run(&crate::gpu_tests::gpu(), size, body, TemporalCamera::NONE)
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-6
}

fn same(a: [f32; 2], b: [f32; 2]) -> bool {
    near(a[0], b[0]) && near(a[1], b[1])
}

fn scatter_is(frame: &Frame, x: usize, y: usize, expected: f32) -> bool {
    frame.scatter[frame.at(x, y)] == expected.to_bits()
}

#[test]
fn edge_texels_of_a_bar_take_the_bars_depth_and_motion() {
    let f = frame(
        [16, 8],
        "if p.x >= 6 && p.x <= 9 && p.y >= 2 && p.y <= 5 { return vec3<f32>(0.4, -2.0, 0.0); }
         return vec3<f32>(0.8, 0.0, 0.0);",
    );
    for y in 0..8 {
        for x in 0..16 {
            let dilated = (5..=10).contains(&x) && (1..=6).contains(&y);
            let (depth, motion) = if dilated {
                (0.4, [-0.125, 0.0])
            } else {
                (0.8, [0.0; 2])
            };
            let i = f.at(x, y);
            assert!(
                (f.depth[i] - depth).abs() < 1e-6,
                "depth at ({x}, {y}): {}",
                f.depth[i]
            );
            assert!(
                same(f.motion[i], motion),
                "motion at ({x}, {y}): {:?}",
                f.motion[i]
            );
        }
    }
}

#[test]
fn equal_nearest_depths_go_to_the_first_texel_in_raster_order() {
    let f = frame(
        [16, 8],
        "if p.y == 3 && p.x == 4 { return vec3<f32>(0.5, -1.0, 0.0); }
         if p.y == 3 && p.x == 6 { return vec3<f32>(0.5, 1.0, 0.0); }
         return vec3<f32>(0.9, 0.0, 0.0);",
    );
    assert!(
        same(f.motion[f.at(5, 3)], [-0.0625, 0.0]),
        "{:?}",
        f.motion[f.at(5, 3)]
    );
    assert!((f.depth[f.at(5, 3)] - 0.5).abs() < 1e-6);
}

fn crossing(near_left: bool) -> Frame {
    let (left, right) = if near_left {
        ("0.3", "0.7")
    } else {
        ("0.7", "0.3")
    };
    frame(
        [16, 8],
        &format!(
            "if p.x < 8 {{ return vec3<f32>({left}, 4.0, 0.0); }}
             return vec3<f32>({right}, -4.0, 0.0);"
        ),
    )
}

#[test]
fn conflicting_destinations_keep_the_nearer_surface_whichever_side_it_is_on() {
    for near_left in [true, false] {
        let f = crossing(near_left);
        for y in 1..=6 {
            for x in 6..=10 {
                assert!(
                    scatter_is(&f, x, y, 0.3),
                    "near_left {near_left} at ({x}, {y}): {:#x}",
                    f.scatter[f.at(x, y)]
                );
            }
        }
        let again = crossing(near_left);
        assert_eq!(
            f.scatter, again.scatter,
            "same code on one device is byte equal"
        );
    }
}

#[test]
fn destinations_outside_the_frame_are_ignored_and_texels_inside_receive() {
    let f = frame([16, 8], "return vec3<f32>(0.5, 1.5, 1.5);");
    for y in 0..8 {
        for x in 0..16 {
            let reached = x >= 1 && y >= 1;
            assert_eq!(
                scatter_is(&f, x, y, 0.5),
                reached,
                "({x}, {y}): {:#x}",
                f.scatter[f.at(x, y)]
            );
            if !reached {
                assert_eq!(f.scatter[f.at(x, y)], INF_BITS);
            }
        }
    }
}

#[test]
fn texels_at_the_border_still_scatter_inward() {
    let f = frame([16, 8], "return vec3<f32>(0.5, -3.0, 0.0);");
    for y in 0..8 {
        for x in 0..16 {
            let expected = if x <= 13 { 0.5f32.to_bits() } else { INF_BITS };
            assert_eq!(f.scatter[f.at(x, y)], expected, "({x}, {y})");
        }
    }
}

#[test]
fn invalid_depths_write_nothing_and_no_nan() {
    let zero = "let z = f32(p.x - p.x); ";
    for (label, depth) in [
        ("cleared", "0.0"),
        ("sky", "1.0"),
        ("negative", "-1.0"),
        ("infinite", "1.0 / z"),
        ("nan", "z / z"),
    ] {
        let f = frame(
            [16, 8],
            &format!("{zero}return vec3<f32>({depth}, 2.0, 0.0);"),
        );
        assert!(
            f.scatter.iter().all(|bits| *bits == INF_BITS),
            "{label} scattered"
        );
        assert!(
            f.depth.iter().all(|z| near(*z, 1.0)),
            "{label} depth {:?}",
            &f.depth[..4]
        );
        assert!(
            f.motion.iter().all(|m| same(*m, [0.125, 0.0])),
            "{label} motion {:?}",
            f.motion[0]
        );
    }
}

#[test]
fn a_motion_that_is_not_finite_is_unusable_and_never_scattered() {
    for (label, motion) in [("infinite", "1.0 / z"), ("nan", "z / z")] {
        let f = frame(
            [16, 8],
            &format!("let z = f32(p.x - p.x); return vec3<f32>(0.5, {motion}, 0.0);"),
        );
        assert!(
            f.scatter.iter().all(|bits| *bits == INF_BITS),
            "{label} scattered"
        );
        assert!(
            f.depth.iter().all(|z| (*z - 0.5).abs() < 1e-6),
            "{label} depth"
        );
        assert!(
            f.motion
                .iter()
                .all(|m| m[0].is_infinite() && m[1].is_infinite()),
            "{label} motion {:?}",
            f.motion[0]
        );
    }
}

#[test]
fn odd_extents_cover_every_texel_without_reaching_outside() {
    for size in [[107, 64], [123, 74], [1, 1], [3, 1]] {
        let f = frame(size, "return vec3<f32>(0.5, 0.5, 0.0);");
        let width = f32::from(u16::try_from(size[0]).unwrap());
        assert_eq!(f.depth.len(), f.scatter.len());
        assert!(
            f.depth.iter().all(|z| (*z - 0.5).abs() < 1e-6),
            "{size:?} depth"
        );
        assert!(
            f.motion.iter().all(
                |m| (m[0] - 0.5 / width).abs() <= 0.5 / width * F16_RELATIVE && near(m[1], 0.0)
            ),
            "{size:?} motion {:?}",
            f.motion[0]
        );
        assert!(
            f.scatter.iter().all(|bits| *bits == 0.5f32.to_bits()),
            "{size:?} scatter"
        );
    }
}
