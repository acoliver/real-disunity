//! GPU checks of the skinning blend against hand-computed expectations.
//!
//! Joint 0 is the identity. Joint 1 turns a quarter turn about +Z around the pivot (1, 0, 0).
//! Each expectation is written out by hand from that geometry.

use super::gpu_math_tests::evaluate_source;
use crate::device::GpuContext;

const SKINNING_WGSL: &str = include_str!("skinning.wgsl");

/// Filler joints sit at entries 0 and 1, so the ribbon starts at palette offset 2.
const OFFSET: u32 = 2;

/// Rows of each joint, row-major 3x4.
const PALETTE: [[f32; 12]; 4] = [
    [9.0; 12],
    [9.0; 12],
    // Joint 0: identity.
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    // Joint 1: rotation rows (0,-1,0), (1,0,0), (0,0,1); translation (1,-1,0).
    [0.0, -1.0, 0.0, 1.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0],
];

/// Runs `call` (a WGSL expression of type `vec3<f32>`) with the palette bound.
fn skin(call: &str) -> [f32; 3] {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let entry = format!(
        r"
@group(1) @binding(7) var<storage, read> scene_skin_palette: array<SceneSkinRow>;
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    result[0] = vec4<f32>({call}, 1.0);
}}"
    );
    let palette = PALETTE.as_flattened();
    let values = evaluate_source(&context, SKINNING_WGSL, &entry, None, Some(palette));
    [values[0], values[1], values[2]]
}

/// Joints and weights as the WGSL `vec2<u32>` skin word, one byte per lane.
fn word(joints: [u8; 4], weights: [u8; 4]) -> String {
    format!(
        "vec2<u32>({}u, {}u)",
        u32::from_le_bytes(joints),
        u32::from_le_bytes(weights)
    )
}

fn assert_near(actual: [f32; 3], expected: [f32; 3]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-6, "{actual:?} against {expected:?}");
    }
}

#[test]
fn a_fully_weighted_point_follows_its_one_joint() {
    let point = "vec3<f32>(2.0, 0.0, 0.0)";
    let rest = word([0, 1, 1, 1], [255, 0, 0, 0]);
    let turned = word([1, 0, 0, 0], [255, 0, 0, 0]);
    assert_near(
        skin(&format!("scene_skin_point({OFFSET}u, {rest}, {point})")),
        [2.0, 0.0, 0.0],
    );
    // R((2,0,0) - (1,0,0)) + (1,0,0) = (0,1,0) + (1,0,0).
    assert_near(
        skin(&format!("scene_skin_point({OFFSET}u, {turned}, {point})")),
        [1.0, 1.0, 0.0],
    );
}

#[test]
fn split_weights_blend_the_joint_results_affinely() {
    let blended = word([0, 1, 0, 0], [128, 127, 0, 0]);
    let (w0, w1) = (128.0 / 255.0, 127.0 / 255.0);
    let point = skin(&format!(
        "scene_skin_point({OFFSET}u, {blended}, vec3<f32>(2.0, 0.0, 0.0))"
    ));
    assert_near(point, [w0 * 2.0 + w1, w1, 0.0]);
}

#[test]
fn directions_take_only_the_linear_part() {
    let turned = word([1, 0, 0, 0], [255, 0, 0, 0]);
    // The translation (1,-1,0) must not reach a direction: R (1,0,0) = (0,1,0).
    let direction = skin(&format!(
        "scene_skin_direction({OFFSET}u, {turned}, vec3<f32>(1.0, 0.0, 0.0))"
    ));
    assert_near(direction, [0.0, 1.0, 0.0]);
}

#[test]
fn an_unweighted_vertex_keeps_its_source_values_exactly() {
    // Joint 3 is filler; a zero weight word must not read it at all.
    let unweighted = word([3, 3, 3, 3], [0, 0, 0, 0]);
    let point = "vec3<f32>(0.1, -7.25, 3.0)";
    let skinned = skin(&format!(
        "scene_skin_point({OFFSET}u, {unweighted}, {point})"
    ));
    assert_eq!(
        skinned.map(f32::to_bits),
        [0.1_f32, -7.25, 3.0].map(f32::to_bits)
    );
}
