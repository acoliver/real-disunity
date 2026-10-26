use super::ENVIRONMENT_WGSL;
use super::gpu_math_tests::evaluate_source;
use crate::device::GpuContext;
use glam::DVec3;

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
                for (actual, expected) in actual.iter().zip(expected.to_array()) {
                    assert!(
                        (f64::from(*actual) - expected).abs() < 2e-6,
                        "point={point:?}, time={time}, curve={curve}"
                    );
                }
            }
        }
    }
}
