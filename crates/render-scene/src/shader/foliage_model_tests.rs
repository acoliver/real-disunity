use super::ENVIRONMENT_WGSL;
use super::gpu_math_tests::evaluate_source;
use crate::device::GpuContext;
use glam::DVec3;

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap()
}

fn evaluate(context: &GpuContext, body: &str) -> Vec<f32> {
    let source = ENVIRONMENT_WGSL
        .replace(
            "@group(1) @binding(0) var<uniform> material: Material;",
            "var<private> material: Material;",
        )
        .replace(
            "@group(0) @binding(0) var<uniform> frame: Frame;",
            "var<private> frame: Frame;",
        );
    let entry = format!(
        "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;\n@compute @workgroup_size(1) fn evaluate_math() {{ {body} }}"
    );
    let ground = [0.0, 0.0, 1.0, 2.0, 2.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    evaluate_source(context, &source, &entry, None, Some(&ground))
}

// Midpoint integration of the tangent field is independent of the shader's
// trigonometric antiderivative, including its straight continuation at pi/2.
fn integrate(sample: DVec3, force: DVec3, curvature: f64) -> DVec3 {
    let step = sample.length() / 16384.0;
    let mut end = DVec3::ZERO;
    for index in 0..16384 {
        let angle = ((f64::from(index) + 0.5) * step * curvature).min(std::f64::consts::FRAC_PI_2);
        end += (sample.normalize() * angle.cos() + force.normalize() * angle.sin()) * step;
    }
    end
}

#[test]
fn authored_basis_pins_oblique_parallel_and_radial_stretch_and_tangents() {
    let context = context();
    for (sample, force, curvature) in [
        (DVec3::Z * 2.0, DVec3::Z, 0.25),
        (DVec3::Z * 2.0, DVec3::new(1.0, 0.0, 1.0), 0.25),
        (DVec3::new(0.6, 0.8, 0.0), DVec3::X, 0.7),
        (DVec3::X, DVec3::X, 3.0),
        (DVec3::Z * 2.0, DVec3::X, 0.25),
    ] {
        let literal = |v: DVec3| format!("vec3<f32>({:?},{:?},{:?})", v.x, v.y, v.z);
        let body = format!(
            "let sample = {}; let force = {}; result[0] = scene_arc(sample, force, {curvature});",
            literal(sample),
            literal(force)
        );
        let actual = evaluate(&context, &body);
        let expected = integrate(sample, force, curvature);
        for (a, e) in actual.iter().zip(expected.to_array()) {
            assert!(
                (f64::from(*a) - e).abs() < 2e-6,
                "{sample:?}, {force:?}: {actual:?} vs {expected:?}"
            );
        }
        let body = format!(
            "let sample = {}; let force = {}; let unit = normalize(sample); let delta = 0.001; result[0] = vec4((scene_arc(sample + delta * unit, force, {curvature}).xyz - scene_arc(sample - delta * unit, force, {curvature}).xyz) / (2.0 * delta), 1.0);",
            literal(sample),
            literal(force)
        );
        let tangent = evaluate(&context, &body);
        let angle = (sample.length() * curvature).min(std::f64::consts::FRAC_PI_2);
        let expected = sample.normalize() * angle.cos() + force.normalize() * angle.sin();
        for (a, e) in tangent.iter().zip(expected.to_array()) {
            assert!((f64::from(*a) - e).abs() < 0.0003);
        }
        if sample.dot(force) > 0.0 && angle < std::f64::consts::FRAC_PI_2 {
            assert!(expected.length() > 1.0);
        } else if sample.dot(force) == 0.0 {
            assert!((expected.length() - 1.0).abs() < 1e-12);
        }
    }
}

const MATERIAL_ENTRY: &str = r"
    material.wind_clock = vec4<u32>(1000u, 0u, 0u, 0u);
    material.wind_direction_amplitude = vec4(0.0, 1.0, 0.0, 0.5);
    material.foliage_axis_curve = vec4(0.0, 0.0, 1.0, 1.0);
    material.foliage_extent_strength_phase = vec4(2.0, 1.0, 1.0, 1.0);
    material.foliage_primary = vec4(0u, 0u, 0u, bitcast<u32>(1.0));
    material.foliage_radial = material.foliage_primary;
    frame.view_projection = mat4x4<f32>(vec4(1.0,0.0,0.0,0.0),vec4(0.0,1.0,0.0,0.0),vec4(0.0,0.0,1.0,0.0),vec4(0.0,0.0,0.0,1.0));
    frame.camera_position = vec4(0.0, -4.0, 3.0, 1.0);
    var instance: InstanceInput;
    instance.model_column_0 = vec4(0.0,0.0,-1.0,0.0);
    instance.model_column_1 = vec4(0.0,1.0,0.0,0.0);
    instance.model_column_2 = vec4(1.0,0.0,0.0,0.0);
    instance.model_column_3 = vec4(0.0,0.0,0.0,1.0);
    instance.tint = vec4(1.0);
    var vertex: VertexInput;
    vertex.normal = vec3(0.0,-1.0,0.0);
    vertex.tangent = vec4(1.0,0.0,0.0,-1.0);
";

#[test]
fn zero_and_small_force_are_straight_through_material_vertex() {
    let context = context();
    for settings in [
        "material.wind_direction_amplitude.w = 0.0;",
        "material.foliage_extent_strength_phase.z = 0.0;",
        "material.wind_clock.y = 750000u; material.foliage_primary.y = bitcast<u32>(1.0);",
    ] {
        for axis in ["vec3(0.0,0.0,1.0)", "normalize(vec3(1.0,2.0,3.0))"] {
            let body = format!(
                "{MATERIAL_ENTRY} {settings} material.foliage_axis_curve = vec4({axis},1.0); material.foliage_extent_strength_phase.y = 0.001; vertex.position = vec3(1.0,0.0,2.0); let before = transform_vertex(vertex,instance); let after = material_vertex(vertex,instance); result[0] = vec4(after.world_position - before.world_position, 0.0);"
            );
            assert_eq!(evaluate(&context, &body), vec![0.0; 4]);
        }
    }
    let mut previous = 1.0_f32;
    for amplitude in [0.0001, 0.00001, 0.000_001, 0.000_000_1] {
        let body = format!(
            "{MATERIAL_ENTRY} material.wind_direction_amplitude.w = {amplitude:?}; vertex.position = vec3(0.3,0.0,2.0); let before = transform_vertex(vertex,instance); let after = material_vertex(vertex,instance); result[0] = vec4(length(after.world_position - before.world_position), 0.0, 0.0, 0.0);"
        );
        let displacement = evaluate(&context, &body)[0];
        assert!(
            displacement < previous,
            "amplitude={amplitude}: {displacement}"
        );
        previous = displacement;
    }
    assert!(previous < 1e-6);
}

#[test]
fn billboard_wind_keeps_instance_axis_and_offset_hinge() {
    let context = context();
    for mode in [1, 2] {
        // Original rotation maps local Z to world X. The local hinge (0,0,0.4)
        // therefore remains at world (0.4,0,0), even on a camera-facing card.
        let setup = format!(
            "{MATERIAL_ENTRY} material.foliage_origin = vec4(0.0,0.0,0.4,0.0); vertex.lod_billboard_mode = {mode}u;"
        );
        let body = format!(
            "{setup} result[0] = vec4(scene_geometry_displacement(vec3(0.4,0.0,0.0), 0.0, instance), 0.0);"
        );
        assert_eq!(evaluate(&context, &body), vec![0.0; 4]);
        for point in [DVec3::ZERO, DVec3::new(0.0, 0.0, 2.0)] {
            let displayed = if mode == 1 {
                point
            } else {
                // Camera at (0,-4,3): spherical right=X, up=(0,0.6,0.8).
                DVec3::new(point.x, 0.6 * point.z, 0.8 * point.z)
            };
            let relative = displayed - DVec3::X * 0.4;
            let height = relative.x;
            let curvature = (2.0 * (0.5_f64 / 2.0).atan()) / 2.0;
            let center = integrate(DVec3::X * height, DVec3::Y, curvature);
            let angle = height.abs() * curvature;
            let attachment = relative - DVec3::X * height;
            let rotated = glam::DQuat::from_axis_angle(DVec3::Z, angle) * attachment;
            // Negative height disables radial detail for these two samples.
            let expected = DVec3::X * 0.4 + center + rotated;
            let body = format!(
                "{setup} vertex.position = vec3({:?},{:?},{:?}); result[0] = vec4(material_vertex(vertex,instance).world_position,1.0);",
                point.x, point.y, point.z
            );
            let actual = evaluate(&context, &body);
            for (a, e) in actual.iter().zip(expected.to_array()) {
                assert!(
                    (f64::from(*a) - e).abs() < 2e-6,
                    "mode={mode}, point={point:?}: {actual:?} vs {expected:?}"
                );
            }
            assert!((expected - displayed).length() > 0.01);
        }
    }
}

#[test]
fn curved_material_retains_authored_shading_frame() {
    let context = context();
    for field in ["world_normal", "world_tangent.xyz"] {
        let body = format!(
            "{MATERIAL_ENTRY} vertex.position = vec3(0.3,0.0,2.0); let before = transform_vertex(vertex,instance); let after = material_vertex(vertex,instance); result[0] = vec4(after.{field}, distance(after.world_position,before.world_position));"
        );
        let actual = evaluate(&context, &body);
        let expected = if field == "world_normal" {
            [0.0, -1.0, 0.0]
        } else {
            [0.0, 0.0, -1.0]
        };
        assert_eq!(&actual[..3], &expected);
        assert!(actual[3] > 0.01);
    }
    let body = format!(
        "{MATERIAL_ENTRY} vertex.position = vec3(0.3,0.0,2.0); let after = material_vertex(vertex,instance); result[0] = vec4(scene_fold_direction(after.world_normal, vec4(0.0,0.0,1.0,1.0),vec2(2.0,0.0)), after.world_tangent.w);"
    );
    assert_eq!(evaluate(&context, &body), vec![0.0, -0.5, 0.5, -1.0]);
}
