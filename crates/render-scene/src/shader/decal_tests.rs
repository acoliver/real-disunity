use super::gpu_math_tests::evaluate_source;
use crate::device::GpuContext;
use glam::{Quat, Vec3};

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap()
}
fn evaluate(context: &GpuContext, expression: &str) -> Vec<f32> {
    let entry = format!(
        "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;\n@compute @workgroup_size(1) fn evaluate_math() {{ result[0] = {expression}; }}"
    );
    evaluate_source(context, super::ENVIRONMENT_WGSL, &entry, None, None)
}

#[test]
fn flat_decal_preserves_partial_fold_magnitude_continuously() {
    let context = context();
    let receiver = Vec3::new(-0.45, 0.0, 0.85);
    let folded = "scene_fold_direction(vec3<f32>(0.0,0.0,1.0), vec4<f32>(-0.6,0.0,0.8,0.5), vec2<f32>(2.0,0.0))";
    let baseline = evaluate(&context, &format!("vec4<f32>({folded}, 1.0)"));
    for (actual, expected) in baseline[..3].iter().zip(receiver.to_array()) {
        assert!((actual - expected).abs() < 0.000_001);
    }
    for weight in [0.0_f32, 0.000_001, 0.4, 1.0] {
        let result = evaluate(
            &context,
            &format!(
                "vec4<f32>(scene_decal_blend_normal({folded}, scene_decal_correct(vec3<f32>(0.0,0.0,1.0), scene_decal_decode(vec2<f32>(0.5))), {weight}), 1.0)"
            ),
        );
        let actual = Vec3::from_slice(&result[..3]);
        let magnitude = receiver.length() * (1.0 - weight) + weight;
        let expected = receiver.lerp(Vec3::Z, weight).normalize() * magnitude;
        assert!(actual.abs_diff_eq(expected, 0.000_001));
        assert!((actual.length() - magnitude).abs() < 0.000_001);
        if weight == 0.0 {
            assert_eq!(result, baseline);
        }
        if weight <= 0.000_001 {
            assert!(actual.distance(receiver) < 0.000_002);
        }
    }
}

#[test]
fn normal_basis_follows_rotated_nonuniform_hulls_for_every_projection_axis() {
    let context = context();
    for (axis, expected) in [
        (0, [-0.6, 0.8, 0.0]),
        (1, [-0.8, 0.0, 0.6]),
        (2, [0.0, 0.6, 0.8]),
    ] {
        let result = evaluate(
            &context,
            &format!(
                "vec4<f32>(scene_decal_basis(SceneDecalVolume(mat4x4<f32>(vec4<f32>(0.0,-0.333333333,0.0,0.0),vec4<f32>(0.5,0.0,0.0,0.0),vec4<f32>(0.0,0.0,0.25,0.0),vec4<f32>(0.0,0.0,0.0,1.0)),vec4<f32>(0.0),vec4<f32>(0.0),vec4<u32>(0u,0u,{axis}u,0u),vec4<f32>(0.0),vec4<f32>(0.0))) * vec3<f32>(0.6,0.0,0.8),1.0)"
            ),
        );
        for (actual, expected) in result[..3].iter().zip(expected) {
            assert!((actual - expected).abs() < 0.000_001);
        }
    }
}

#[test]
fn projected_normal_decode_rotation_and_receiver_correction_match_shortest_arc_oracle() {
    let context = context();
    for (encoded, detail) in [
        ([0.5, 0.5], Vec3::Z),
        ([0.75, 0.5], Vec3::new(0.0, -0.5, 0.75_f32.sqrt())),
        ([0.5, 0.8], Vec3::new(0.6, 0.0, 0.8)),
        ([1.0, 1.0], Vec3::new(1.0, -1.0, 0.0)),
    ] {
        for surface in [Vec3::Z, Vec3::new(-0.5, 0.0, 1.0).normalize(), -Vec3::Z] {
            let expected = if surface == -Vec3::Z {
                Quat::from_rotation_x(std::f32::consts::PI) * detail
            } else {
                Quat::from_rotation_arc(Vec3::Z, surface) * detail
            };
            let result = evaluate(
                &context,
                &format!(
                    "vec4<f32>(scene_decal_correct(vec3<f32>({},{},{}), scene_decal_decode(vec2<f32>({},{}))), 1.0)",
                    surface.x, surface.y, surface.z, encoded[0], encoded[1]
                ),
            );
            for (actual, expected) in result[..3].iter().zip(expected.to_array()) {
                assert!(
                    (actual - expected).abs() < 0.000_002,
                    "{result:?} {expected}"
                );
            }
        }
    }
}

#[test]
fn power_and_linear_angle_curves_match_scalar_oracles_on_both_sides() {
    let context = context();
    for cosine in [-1.0_f32, -0.35, 0.0, 0.35, 0.7, 1.0] {
        for double_sided in [false, true] {
            let angle = if double_sided {
                cosine.abs()
            } else {
                cosine.max(0.0)
            };
            let side = u32::from(double_sided);
            let result = evaluate(
                &context,
                &format!(
                    "vec4<f32>(scene_decal_angle({cosine}, vec2<f32>(0.2,0.6), vec3<f32>(0.7,2.4,{side})), scene_decal_angle({cosine}, vec2<f32>(0.2,0.6), vec3<f32>(0.0,0.0,{side})), 0.0, 1.0)"
                ),
            );
            assert!((result[0] - (angle / 0.7).min(1.0).powf(2.4)).abs() < 0.000_002);
            assert!((result[1] - ((angle - 0.2) / 0.6).clamp(0.0, 1.0)).abs() < 0.000_002);
        }
    }
}

#[test]
fn uv_scale_offset_and_packed_surface_blend_match_independent_values() {
    let context = context();
    let result = evaluate(
        &context,
        "vec4<f32>((scene_decal_uv(vec3<f32>(0.1,-0.2,0.3), 2u) + 0.5) * vec2<f32>(2.0,-3.0) + vec2<f32>(0.25,0.1), mix(vec2<f32>(0.8,0.1), vec3<f32>(0.2,0.95,0.7).rb, 0.25))",
    );
    for (actual, expected) in result.iter().zip([1.45, -0.8, 0.65, 0.25]) {
        assert!((actual - expected).abs() < 0.000_001);
    }
}
