//! The depth clip of the temporal stage on lavapipe, through `taa_reconstruct_test_rig`: the
//! confidence plane it writes for known scenes under a static camera with depth terms.
//!
//! The confidence of a texel is 1 or 0, exactly. A surface alone finds its own depth where its
//! motion points, and the tolerance includes the depth step to every neighbour, so a ramp keeps
//! full confidence. A texel whose history position was reached by a nearer surface of the
//! current frame has 0, unless a neighbour is that surface itself: the 3x3 neighbours of a
//! texel next to the dilated edge include the edge, which enlarges the tolerance to the full
//! step. A bar that moves two texels leaves the texel two before its dilated edge at 0 and the
//! texel before it at 1; a faster bar leaves more texels at 0.
use crate::TemporalCamera;
use crate::taa_reconstruct_test_rig::{Frame, run, static_camera};

fn frame(size: [u32; 2], body: &str, camera: TemporalCamera) -> Frame {
    run(&crate::gpu_tests::gpu(), size, body, camera)
}

/// Confidence is written as exactly 1.0 or 0.0; the bit patterns are compared.
fn full(c: f32) -> bool {
    c.to_bits() == 1.0_f32.to_bits()
}

fn none(c: f32) -> bool {
    c.to_bits() == 0.0_f32.to_bits()
}

const BAR: &str =
    "if p.x >= 6 && p.x <= 9 && p.y >= 2 && p.y <= 5 { return vec3<f32>(0.4, -2.0, 0.0); }
     return vec3<f32>(0.8, 0.0, 0.0);";

#[test]
fn a_depth_ramp_keeps_full_confidence_standing_still_and_panned_by_a_texel() {
    let ramp = |motion: &str| {
        format!(
            "return vec3<f32>(0.5 + 0.4 * f32(p.x) / 32.0 + 0.05 * f32(p.y) / 8.0, {motion}, 0.0);"
        )
    };
    let still = frame([32, 8], &ramp("0.0"), static_camera());
    assert!(
        still.confidence.iter().all(|c| full(*c)),
        "{:?}",
        &still.confidence[..32]
    );
    // One texel of motion: the history of texel 0 is outside the frame, every other texel
    // finds the ramp one step nearer at its history position.
    let panned = frame([32, 8], &ramp("-1.0"), static_camera());
    for y in 0..8 {
        for x in 1..32 {
            let c = panned.confidence[panned.at(x, y)];
            assert!(full(c), "({x}, {y}): {c}");
        }
    }
}

#[test]
fn the_texel_a_nearer_surface_vacated_has_no_confidence_beyond_the_dilated_edge() {
    let f = frame([16, 8], BAR, static_camera());
    let row = |x: usize| f.confidence[f.at(x, 3)];
    assert!(
        none(row(3)),
        "reached by the bar's previous position: {}",
        row(3)
    );
    for x in [0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15] {
        assert!(full(row(x)), "x {x}: {}", row(x));
    }
    for (i, c) in f.confidence.iter().enumerate() {
        assert!(none(*c) || full(*c), "texel {i}: {c}");
    }
}

#[test]
fn without_depth_terms_every_texel_keeps_its_history() {
    let f = frame([16, 8], BAR, TemporalCamera::NONE);
    assert!(f.confidence.iter().all(|c| full(*c)));
}
