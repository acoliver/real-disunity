use super::gpu_math_tests::evaluate_source;
use crate::device::GpuContext;
use glam::{Quat, Vec3};

#[test]
fn default_normal_frame_preserves_projection_float_bytes() {
    let context = context();
    for rotation in [
        Quat::IDENTITY,
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Quat::from_rotation_y(0.4),
    ] {
        for geometric in [Vec3::Z, Vec3::new(-0.5, 0.0, 1.0).normalize()] {
            let expression = frame_expression(rotation, geometric, false);
            let current = evaluate(&context, &expression);
            let stock = evaluate(
                &context,
                &expression.replace("frame_test_normal", "frame_stock_normal"),
            );
            assert_eq!(
                current.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                stock.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn normal_correction_frames_match_rotated_plane_and_slope_oracles() {
    let context = context();
    for rotation in [
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Quat::from_rotation_z(0.7) * Quat::from_rotation_y(0.4),
    ] {
        for geometric in [Vec3::Z, Vec3::new(-0.5, 0.0, 1.0).normalize()] {
            for world in [false, true] {
                let local = rotation.conjugate() * geometric;
                let detail = Vec3::new(0.6, 0.0, 0.8);
                let expected = if world {
                    Quat::from_rotation_arc(Vec3::Z, geometric) * detail
                } else {
                    rotation * (Quat::from_rotation_arc(Vec3::Z, local) * detail)
                };
                let result = evaluate(&context, &frame_expression(rotation, geometric, world));
                assert!(
                    Vec3::from_slice(&result).abs_diff_eq(expected, 0.000_002),
                    "world={world}, rotation={rotation:?}, geometric={geometric:?}: {result:?} != {expected:?}"
                );
            }
        }
    }
}

#[test]
fn receiver_world_uses_reference_rotation_until_the_exact_south_pole() {
    let context = context();
    for z in [-0.999_998_1_f32, -0.999_999_5, -1.0] {
        let x = ((1.0 + z) * (1.0 - z)).sqrt();
        let geometric = Vec3::new(x, 0.0, z);
        let angle = f64::from(x).atan2(f64::from(z));
        let (sin, cos) = angle.sin_cos();
        let reference = glam::DVec3::new(cos * 0.6 + sin * 0.8, 0.0, -sin * 0.6 + cos * 0.8);
        let half_turn = glam::DVec3::new(0.6, 0.0, -0.8);
        for world in [false, true] {
            let expected = if z.to_bits() == (-1.0_f32).to_bits() || (!world && z < -0.999_999) {
                half_turn
            } else {
                reference
            };
            let result = evaluate(
                &context,
                &frame_expression(Quat::IDENTITY, geometric, world),
            );
            assert!(
                Vec3::from_slice(&result)
                    .as_dvec3()
                    .abs_diff_eq(expected, 0.000_002),
                "world={world}, geometric={geometric:?}: {result:?} != {expected:?}"
            );
        }
    }
}

fn frame_expression(rotation: Quat, geometric: Vec3, world: bool) -> String {
    let inverse =
        glam::Mat4::from_scale_rotation_translation(Vec3::new(2.0, 3.0, 4.0), rotation, Vec3::ZERO)
            .inverse()
            .to_cols_array();
    let columns = inverse
        .chunks_exact(4)
        .map(|v| format!("vec4<f32>({},{},{},{})", v[0], v[1], v[2], v[3]))
        .collect::<Vec<_>>()
        .join(",");
    let flags = if world { 524_288 } else { 0 };
    format!(
        "vec4<f32>(frame_test_normal(SceneDecalVolume(mat4x4<f32>({columns}),vec4<f32>(0.0),vec4<f32>(0.0),vec4<u32>(0u,0u,2u,{flags}u),vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(1.0)),vec3<f32>({},{},{})),1.0)",
        geometric.x, geometric.y, geometric.z,
    )
}

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap()
}
fn evaluate(context: &GpuContext, expression: &str) -> Vec<f32> {
    let entry = format!(
        "fn frame_test_normal(decal: SceneDecalVolume, geometric: vec3<f32>) -> vec3<f32> {{ return scene_decal_correct_normal(decal, geometric, vec2<f32>(0.5,0.8)); }}
fn frame_stock_normal(decal: SceneDecalVolume, geometric: vec3<f32>) -> vec3<f32> {{ let basis = scene_decal_basis(decal); return basis * scene_decal_correct(transpose(basis) * geometric, scene_decal_decode(vec2<f32>(0.5,0.8))); }}\n@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;\n@compute @workgroup_size(1) fn evaluate_math() {{ result[0] = {expression}; }}"
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
                "vec4<f32>(scene_decal_basis(SceneDecalVolume(mat4x4<f32>(vec4<f32>(0.0,-0.333333333,0.0,0.0),vec4<f32>(0.5,0.0,0.0,0.0),vec4<f32>(0.0,0.0,0.25,0.0),vec4<f32>(0.0,0.0,0.0,1.0)),vec4<f32>(0.0),vec4<f32>(0.0),vec4<u32>(0u,0u,{axis}u,0u),vec4<f32>(0.0),vec4<f32>(0.0),vec4<f32>(1.0))) * vec3<f32>(0.6,0.0,0.8),1.0)"
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

#[test]
fn decal_color_matches_scalar_oracle_including_hdr_and_opacity_edges() {
    let context = context();
    for opacity in [0.0_f32, 0.4, 0.98, 1.0] {
        let lower = (1.0 - opacity).max(0.0001);
        let upper = (1.05 - opacity).min(1.0);
        for alpha in [
            0.0,
            lower - 0.00001,
            lower,
            (lower + upper) * 0.5,
            upper,
            1.0,
        ] {
            for edge in [false, true] {
                let result = evaluate(
                    &context,
                    &format!(
                        "scene_decal_color(vec4<f32>(0.2,0.4,0.8,{alpha}), vec4<f32>(1.77,0.0,0.5,{opacity}), {edge})"
                    ),
                );
                let expected_alpha = if edge && opacity == 0.0 {
                    0.0
                } else if edge {
                    let t = ((alpha - lower) / (upper - lower)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                } else {
                    alpha * opacity
                };
                for (actual, expected) in result.iter().zip([0.354, 0.0, 0.4, expected_alpha]) {
                    assert!(
                        (actual - expected).abs() < 0.000_01,
                        "{result:?} expected {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn default_decal_color_preserves_sample_bits() {
    let context = context();
    let sample = "vec4<f32>(0.1234567,1.77,0.0,0.3333333)";
    let original = evaluate(&context, sample);
    let modified = evaluate(
        &context,
        &format!("scene_decal_color({sample}, vec4<f32>(1.0), false)"),
    );
    assert_eq!(
        original.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        modified.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
}
