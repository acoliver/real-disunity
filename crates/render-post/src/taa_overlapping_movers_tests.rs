//! Two movers that overlap in the frame and travel in opposite directions, through
//! `taa_reconstruct_test_rig` on lavapipe. A near bar (depth 0.4) moves right by two texels, a
//! farther bar (depth 0.6) moves left by two texels, and they overlap in the texels the near bar
//! hides. Depths and motions are exact in f32 and in the f16 velocity format (the frame is 32
//! texels wide, so two texels are 0.0625).
//!
//! Expectations derived from the stage design: the dilated motion of a texel is the motion of the
//! nearest valid depth in its 3x3 neighbourhood, so the near bar's motion covers the overlap and
//! the far bar's texels next to it; the far bar's trailing side is its right side, whose uncovered
//! texel is the mirror of the single-bar case in `taa_depth_clip_tests`: that case leaves the
//! texel three before the first bar texel at confidence 0, here the texel three after the last.
use crate::taa_reconstruct_test_rig::{Frame, run, static_camera};

const NEAR_BAR: &str =
    "if p.x >= 4 && p.x <= 8 && p.y >= 2 && p.y <= 5 { return vec3<f32>(0.4, -2.0, 0.0); }";
const FAR_BAR: &str =
    "if p.x >= 7 && p.x <= 14 && p.y >= 2 && p.y <= 5 { return vec3<f32>(0.6, 2.0, 0.0); }";
const BACKGROUND: &str = "return vec3<f32>(0.8, 0.0, 0.0);";

fn frame(bars: &[&str]) -> Frame {
    let body = format!("{}{BACKGROUND}", bars.concat());
    run(&crate::gpu_tests::gpu(), [32, 8], &body, static_camera())
}

fn motion_at(f: &Frame, x: usize) -> [f32; 2] {
    f.motion[f.at(x, 3)]
}

fn is_motion(f: &Frame, x: usize, expected: f32) -> bool {
    let m = motion_at(f, x);
    (m[0] - expected).abs() < 1e-6 && m[1].abs() < 1e-6
}

fn depth_at(f: &Frame, x: usize) -> f32 {
    f.depth[f.at(x, 3)]
}

fn confidence_at(f: &Frame, x: usize) -> f32 {
    f.confidence[f.at(x, 3)]
}

#[test]
fn the_nearer_movers_motion_wins_in_the_dilated_motion_of_the_overlap() {
    let f = frame(&[NEAR_BAR, FAR_BAR]);
    // The overlap texels 7 and 8 show the near bar; texel 9 is a far bar texel next to it.
    for x in 7..=9 {
        assert!(is_motion(&f, x, -0.0625), "x {x}: {:?}", motion_at(&f, x));
        assert!((depth_at(&f, x) - 0.4).abs() < 1e-6, "x {x}");
    }
    // Texels with only far bar texels in their neighbourhood keep the far bar's motion.
    for x in 10..=15 {
        assert!(is_motion(&f, x, 0.0625), "x {x}: {:?}", motion_at(&f, x));
        assert!((depth_at(&f, x) - 0.6).abs() < 1e-6, "x {x}");
    }
}

#[test]
fn the_farther_movers_uncovered_texel_has_no_confidence_and_nothing_is_nan() {
    let both = frame(&[NEAR_BAR, FAR_BAR]);
    let far_alone = frame(&[FAR_BAR]);
    assert_eq!(confidence_at(&both, 17).to_bits(), 0.0_f32.to_bits());
    // The near bar does not change what the far bar's trailing side finds.
    for x in 12..32 {
        assert_eq!(
            confidence_at(&both, x).to_bits(),
            confidence_at(&far_alone, x).to_bits(),
            "x {x}"
        );
    }
    for f in [&both, &far_alone] {
        assert!(f.depth.iter().all(|z| z.is_finite()));
        assert!(f.motion.iter().all(|m| m.iter().all(|v| v.is_finite())));
        assert!(
            f.confidence
                .iter()
                .all(|c| c.to_bits() == 0.0_f32.to_bits() || c.to_bits() == 1.0_f32.to_bits())
        );
    }
}
