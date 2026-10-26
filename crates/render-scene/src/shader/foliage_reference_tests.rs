use super::ENVIRONMENT_WGSL;
use super::gpu_math_tests::evaluate_source;
use crate::device::GpuContext;
use glam::DVec3;

#[test]
fn attachment_geometry_matches_reference_using_gpu_trigonometry() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for height in [0.5_f64, 2.0] {
        for curve in [1.0_f64, 2.0] {
            let entry = format!(
                r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let force = vec3(0.5,0.0,0.0);
    let weight = pow({height} / 2.0, {curve} - 1.0);
    let center = scene_arc(vec3(0.0,0.0,{height}), force, scene_arc_curvature(force,2.0) * weight);
    let actual = scene_curved_position(SceneBendInput(vec3(0.3,0.0,{height}),vec3(0.0,0.0,1.0),force,vec2(2.0,1.0),{curve},0.0)) - center.xyz;
    result[0] = vec4(actual.x,actual.z,sin(-center.w),cos(-center.w));
}}"
            );
            let actual = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None);
            let expected = [0.3 * f64::from(actual[3]), 0.3 * f64::from(actual[2])];
            for (a, e) in actual.iter().zip(expected) {
                assert!(
                    (f64::from(*a) - e).abs() < ROUNDING_ERROR,
                    "height={height}, curve={curve}: actual={actual:?}, expected={expected:?}"
                );
            }
        }
    }
}

#[test]
fn arc_geometry_matches_reference_using_gpu_trigonometry() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for height in [0.5_f64, 2.0, 8.0] {
        for curvature in [0.06_f64, 0.25, 0.7] {
            let entry = format!(
                r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let angle = min({height} * {curvature}, 0.5 * PI);
    let actual = scene_arc(vec3(0.0,0.0,{height}),vec3(1.0,0.0,0.0),{curvature});
    result[0] = vec4(actual.x,actual.z,sin(angle),cos(angle));
}}"
            );
            let actual = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None);
            let angle = (height * curvature).min(std::f64::consts::FRAC_PI_2);
            // Read the backend's own trig values. This isolates arc geometry
            // from the 2^-11 intrinsic error without relaxing metre accuracy.
            let expected = [
                (1.0 - f64::from(actual[3])) / curvature + (height - angle / curvature).max(0.0),
                f64::from(actual[2]) / curvature,
            ];
            for (a, e) in actual.iter().zip(expected) {
                assert!(
                    (f64::from(*a) - e).abs() < ROUNDING_ERROR,
                    "height={height}, curvature={curvature}: actual={actual:?}, expected={expected:?}"
                );
            }
        }
    }
}

// Integrate the model's tangent field numerically instead of restating its
// closed-form displacement. The two authored directions need not be orthogonal.
fn circular(vector: DVec3, force: DVec3, radius: f64) -> (DVec3, f64) {
    let length = vector.length();
    if length == 0.0 || force == DVec3::ZERO {
        return (vector, 0.0);
    }
    let angle = (length / radius).min(std::f64::consts::FRAC_PI_2);
    let direction = vector / length;
    let across = force / force.length().max(0.0001);
    let step = length / 4096.0;
    let mut end = DVec3::ZERO;
    for index in 0..4096 {
        let phase = ((f64::from(index) + 0.5) * step / radius).min(std::f64::consts::FRAC_PI_2);
        end += (direction * phase.cos() + across * phase.sin()) * step;
    }
    (end, angle)
}

fn radius(force: DVec3, extent: f64) -> f64 {
    extent / (2.0 * (force.length() / extent).atan())
}

fn reference(point: DVec3, force: DVec3, detail: f64, curve: f64) -> DVec3 {
    let height = point.z.min(2.0);
    let attachment = point - DVec3::Z * height;
    let weight = (height.abs() / 2.0).clamp(0.0, 1.0).powf(curve - 1.0);
    let (center, angle) = circular(
        DVec3::Z * height,
        force,
        radius(force, 2.0).max(0.5) / weight.max(1e-7),
    );
    let hinge = force.cross(DVec3::Z).normalize();
    let rotated = glam::DQuat::from_axis_angle(hinge, -angle) * attachment;
    let detail_force = force * detail;
    let (tip, _) = circular(rotated, detail_force, radius(detail_force, 1.0));
    center + rotated.lerp(tip, ((height / 2.0 - 0.3) * 8.0).clamp(0.0, 1.0))
}

pub(super) const TRIG_ERROR: f64 = 1.0 / 2048.0;
pub(super) const ROUNDING_ERROR: f64 = 2e-6;

// Each arc contributes at most 2*epsilon/curvature metres (unit authored
// directions). A rotation contributes at most 3*|attachment|*epsilon.
// Band errors are 0.1*epsilon and 0.2*epsilon, hence force error 0.05*epsilon.
// For extent 2, d(curvature)/d(force) <= 1/2; for radial extent 1 it is <= 2.
// Numerically: band errors <= 4.883e-5/9.766e-5, force <= 2.442e-5,
// axial curvature <= 1.221e-5*weight and radial curvature <= 1.661e-4.
// The tangent integral is |sample|^2-Lipschitz in curvature and at most
// 3-Lipschitz in its sample, including the straight continuation at pi/2.
// Keep the separate tight GPU-trig arc and rotation tests: cancellation at low
// curvature makes this end-to-end portable bound larger than a gentle bend.
fn reference_tolerance(point: DVec3, curve: f64) -> f64 {
    let height = point.z.min(2.0);
    if height == 0.0 {
        return ROUNDING_ERROR;
    }
    let length = (point - DVec3::Z * height).length();
    let weight = (height / 2.0).powf(curve - 1.0);
    let axial_curvature = (0.45_f64 / 2.0).atan() * weight;
    let radial_curvature = 2.0 * (0.45_f64 * 0.8).atan();
    let force_error = 0.05 * TRIG_ERROR;
    let axial_error = force_error * 0.5 * weight;
    let radial_error =
        2.0 * (1.2 * force_error + 0.55 * 0.2 * TRIG_ERROR + force_error * 0.2 * TRIG_ERROR);
    let center = 2.0 * TRIG_ERROR / axial_curvature + height * height * axial_error;
    let rotation = length * (3.0 * TRIG_ERROR + height * axial_error);
    center
        + 3.0 * rotation
        + length * length * radial_error
        + 2.0 * TRIG_ERROR / radial_curvature
        + ROUNDING_ERROR
}

#[test]
fn axial_and_radial_bending_match_cpu_reference_at_fixed_times() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    for point in [
        DVec3::ZERO,
        DVec3::new(0.3, 0.4, 0.5),
        DVec3::new(-0.6, 0.2, 2.0),
        DVec3::new(0.2, -0.3, 3.0),
    ] {
        for time in [0_u32, 1500, 100_000] {
            for curve in [1.0_f64, 2.0] {
                let primary = 1.0
                    + (f64::from(time % 6283) / 1000.0 + point.element_sum() * 0.15).sin() * 0.1;
                let detail =
                    1.0 + (f64::from(time % 2094) / 1000.0 * 3.0 + point.element_sum()).sin() * 0.2;
                let expected = reference(point, DVec3::X * 0.5 * primary, detail, curve);
                let entry = format!(
                    r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    let point = vec3<f32>({},{},{});
    let primary = scene_band(point, vec4<u32>(0u,bitcast<u32>(0.1),bitcast<u32>(0.15),bitcast<u32>(1.0)), {time}u, 0.0);
    let detail = scene_band(point, vec4<u32>(0u,bitcast<u32>(0.2),bitcast<u32>(1.0),bitcast<u32>(3.0)), {time}u, 0.0);
    result[0] = vec4(scene_curved_position(SceneBendInput(point, vec3(0.0,0.0,1.0), vec3(0.5,0.0,0.0) * primary, vec2(2.0,1.0), {curve}, detail)), 1.0);
}}",
                    point.x, point.y, point.z
                );
                let actual = evaluate_source(&context, ENVIRONMENT_WGSL, &entry, None, None);
                let error = DVec3::new(
                    f64::from(actual[0]),
                    f64::from(actual[1]),
                    f64::from(actual[2]),
                ) - expected;
                for (actual, expected) in actual.iter().zip(expected.to_array()) {
                    assert!(
                        (f64::from(*actual) - expected).abs() < reference_tolerance(point, curve),
                        "point={point:?}, time={time}, curve={curve}: actual={actual:?}, expected={expected:?}, component_errors={error:?}"
                    );
                }
            }
        }
    }
}
