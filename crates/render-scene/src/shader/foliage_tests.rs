#[test]
fn band_reduces_complete_phase_before_calling_sine() {
    let context = context();
    let source = ENVIRONMENT_WGSL.replace("sin(", "recorded_sin(");
    for spatial in [-7.0_f64, -0.1, 2.9, 7.0] {
        for phase in [
            0.0_f64,
            1.5 * std::f64::consts::PI,
            2.0 * std::f64::consts::PI,
        ] {
            let entry = format!(
                r"
var<private> largest_angle: f32;
fn recorded_sin(angle: f32) -> f32 {{
    largest_angle = max(largest_angle, abs(angle));
    return sin(angle);
}}
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let value = scene_band(vec3({spatial},0.0,0.0),vec4(0u,bitcast<u32>(0.2),bitcast<u32>(1.0),bitcast<u32>(3.0)),1500u,{phase});
    result[0] = vec4(value,largest_angle,0.0,0.0);
}}"
            );
            let actual = evaluate_source(&context, &source, &entry, None, None);
            let expected = 1.0 + 0.2 * (4.5 + spatial + phase).sin();
            assert!(actual[1] <= std::f32::consts::PI, "{actual:?}");
            assert!(
                (f64::from(actual[0]) - expected).abs() < 0.2 * TRIG_ERROR + ROUNDING_ERROR,
                "spatial={spatial}, phase={phase}, actual={actual:?}, expected={expected}"
            );
        }
    }
}

#[test]
fn curved_deformation_is_axis_covariant_and_base_ring_is_fixed() {
    let context = context();
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let force = vec3(0.5,0.0,0.0);
    let a = SceneBendInput(vec3(0.3,0.4,1.2), vec3(0.0,0.0,1.0), force, vec2(2.0,1.0), 1.0, 1.1);
    let b = SceneBendInput(vec3(0.3,1.2,-0.4), vec3(0.0,1.0,0.0), force, vec2(2.0,1.0), 1.0, 1.1);
    let first = scene_curved_position(a);
    let second = scene_curved_position(b);
    let base = SceneBendInput(vec3(0.3,0.4,0.0), vec3(0.0,0.0,1.0), force, vec2(2.0,1.0), 1.0, 1.1);
    result[0] = vec4(second - vec3(first.x,first.z,-first.y), length(scene_curved_position(base) - base.position));
}";
    let values = evaluate_source(&context, ENVIRONMENT_WGSL, entry, None, None);
    assert!(values.iter().all(|value| value.abs() < 1e-6));
}

use super::ENVIRONMENT_WGSL;
use super::foliage_reference_tests::{ROUNDING_ERROR, TRIG_ERROR};
use super::gpu_math_tests::evaluate_source;
use crate::device::GpuContext;

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap()
}

#[test]
fn curved_axis_base_extent_and_arc_length_match_reference() {
    let context = context();
    for height in [0.0_f64, 0.5, 2.0] {
        let radius = 2.0 / (2.0 * (0.5_f64 / 2.0).atan());
        let theta = height / radius;
        let expected = [radius * (1.0 - theta.cos()), 0.0, radius * theta.sin()];
        let entry = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let input = SceneBendInput(vec3(0.0,0.0,{height}), vec3(0.0,0.0,1.0),
        vec3(0.5,0.0,0.0), vec2(2.0,1.0), 1.0, 1.0);
    result[0] = vec4(scene_curved_position(input), 1.0);
}}"
        );
        let actual = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None);
        for (actual, expected) in actual.iter().zip(expected) {
            // One intrinsic per nonzero component, scaled by the radius.
            assert!((f64::from(*actual) - expected).abs() < radius * TRIG_ERROR + ROUNDING_ERROR);
        }
        // Chord length changes under a circular arc; the represented arc retains length.
        let angle = (f64::from(actual[2]) / radius).asin();
        // asin is <= 1/cos(theta_max) Lipschitz over the perturbed interval.
        let max_sine = (2.0 / radius).sin() + TRIG_ERROR;
        let arc_error = radius * TRIG_ERROR / (1.0 - max_sine * max_sine).sqrt();
        assert!((angle * radius - height).abs() < arc_error + ROUNDING_ERROR);
    }
}

#[test]
fn both_frequency_bands_match_independent_clock_and_spatial_reference() {
    let context = context();
    for (period, frequency, perturb, spatial) in [
        (6283_u32, 1.0_f32, 0.1_f32, 0.15_f32),
        (2094, 3.0, 0.2, 1.0),
    ] {
        for time in [0_u32, 1500, 100_000, u32::MAX] {
            let expected = 1.0
                + ((f64::from(time % period) / 1000.0) * f64::from(frequency)
                    + 1.75 * f64::from(spatial))
                .sin()
                    * f64::from(perturb);
            let entry = format!(
                r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let band = vec4<u32>({period}u, bitcast<u32>({perturb:?}), bitcast<u32>({spatial:?}), bitcast<u32>({frequency:?}));
    result[0] = vec4(scene_band(vec3(0.25,0.5,1.0), band, {time}u, 0.0), 0.0,0.0,1.0);
}}"
            );
            let first = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None);
            let second = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None);
            assert!(
                // Multiplication by the authored amplitude scales sin's absolute bound.
                (f64::from(first[0]) - expected).abs()
                    < f64::from(perturb) * TRIG_ERROR + ROUNDING_ERROR,
                "time={time}, frequency={frequency}, actual={}, expected={expected}",
                first[0]
            );
            assert_eq!(first, second);
        }
    }
}

#[test]
fn rotated_attachment_preserves_length_without_detail_force() {
    let context = context();
    let entry = r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {
    let force = vec3(0.5,0.0,0.0);
    let a = SceneBendInput(vec3(0.0,0.0,0.5), vec3(0.0,0.0,1.0), force, vec2(2.0,1.0), 1.0, 0.0);
    let b = SceneBendInput(vec3(0.3,0.4,0.5), vec3(0.0,0.0,1.0), force, vec2(2.0,1.0), 1.0, 0.0);
    result[0] = vec4(length(scene_curved_position(b) - scene_curved_position(a)),0.0,0.0,1.0);
}";
    let result = evaluate_source(&context, ENVIRONMENT_WGSL, entry, None, None);
    // Rodrigues has three trig-weighted terms, each <= attachment length 0.5.
    assert!((f64::from(result[0]) - 0.5).abs() < 3.0 * 0.5 * TRIG_ERROR + ROUNDING_ERROR);
}
