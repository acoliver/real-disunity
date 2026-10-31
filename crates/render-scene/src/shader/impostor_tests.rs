use super::{ENVIRONMENT_WGSL, gpu_math_tests::evaluate_source};
use crate::device::GpuContext;

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap()
}

fn run(context: &GpuContext, body: &str) -> Vec<f32> {
    let entry = format!(
        "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;\n\
         @compute @workgroup_size(1) fn evaluate_math() {{ {body} }}"
    );
    evaluate_source(context, ENVIRONMENT_WGSL, &entry, None, None)
}

#[test]
fn octahedral_triangle_weights_and_diagonal_boundaries() {
    let context = context();
    for (view, expected) in [
        ("1.0,0.0,1.0", [0.5, 0.0, 0.5, 0.0]),
        ("1.0,1.0,2.0", [0.5, 0.5, 0.0, 0.0]),
        ("1.0,-1.0,2.0", [0.5, 0.5, 0.0, 0.0]),
        ("0.0,0.0,1.0", [1.0, 0.0, 0.0, 0.0]),
    ] {
        let result = run(
            &context,
            &format!(
                "result[0] = scene_atlas_frames(vec3<f32>({view}), vec2<f32>(3.0), true, true, 0.0).weights;"
            ),
        );
        for (actual, expected) in result.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-6, "{view}: {result:?}");
        }
    }
    let cells = run(
        &context,
        "let f = scene_atlas_frames(vec3<f32>(1.0,1.0,2.0), vec2<f32>(3.0), true, true, 0.0); result[0] = vec4<f32>(f.cells[0], f.cells[1]);",
    );
    assert_eq!(cells, [1.0, 1.0, 2.0, 1.0]);
}

#[test]
fn azimuth_wrap_and_elevation_rows_match_angular_reference() {
    let context = context();
    for (view, expected) in [
        ("1.0,0.0,0.0", [0.0, 0.0, 1.0, 0.0]),
        ("0.0,1.0,0.0", [1.0, 0.0, 1.0, 0.0]),
        ("0.0,-1.0,0.0", [3.0, 0.0, 1.0, 0.0]),
        ("1.0,0.0,1.0", [0.0, 1.0, 1.0, 0.0]),
    ] {
        let actual = run(
            &context,
            &format!(
                "let f = scene_atlas_frames(vec3<f32>({view}), vec2<f32>(4.0,3.0), false, true, 0.0); result[0] = vec4<f32>(f.cells[0]*f.weights.x + f.cells[1]*f.weights.y + f.cells[2]*f.weights.z + f.cells[3]*f.weights.w, dot(f.weights,vec4<f32>(1.0)),0.0);"
            ),
        );
        // These cardinal and quarter-turn directions are exact angular oracles.
        // Inverse-trig WGSL error is bounded by 4096 ULP; arguments are unit
        // length and angles bounded by pi, giving <0.003 in this four-cell grid.
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.003,
                "{view}: {actual} vs {expected}"
            );
        }
    }
    let weights = run(
        &context,
        "result[0] = scene_atlas_frames(vec3<f32>(1.0,1.0,0.0), vec2<f32>(4.0,3.0), false, true, 0.0).weights;",
    );
    for (actual, expected) in weights.iter().zip([0.5, 0.5, 0.0, 0.0]) {
        assert!((actual - expected).abs() < 0.003);
    }
}

#[test]
fn cell_border_clamps_and_virtual_plane_intersection() {
    let context = context();
    let uv = run(
        &context,
        "result[0] = vec4<f32>(scene_atlas_uv(vec2<f32>(-4.0,5.0), vec2<f32>(1.0,2.0), vec2<f32>(4.0), vec2<f32>(0.125)),0.0,0.0);",
    );
    assert_eq!(uv, [0.28125, 0.71875, 0.0, 0.0]);
    let plane = run(
        &context,
        "result[0] = vec4<f32>(scene_atlas_plane_uv(vec2<f32>(1.0), vec2<f32>(3.0), vec3<f32>(0.0,0.0,4.0), vec3<f32>(0.25,0.5,0.0), vec2<f32>(2.0)),0.0,0.0);",
    );
    assert_eq!(plane, [0.375, 0.25, 0.0, 0.0]);
}

#[test]
fn nearest_frame_mode_and_quadratic_snap_have_independent_weights() {
    let context = context();
    // With view (1,0,3), a 5x5 octahedral grid has coordinate (2.5,2.5).
    // Use (1,0,4) instead to avoid the implementation-defined rounding tie.
    let nearest = run(
        &context,
        "let f = scene_atlas_frames(vec3<f32>(1.0,0.0,4.0), vec2<f32>(5.0), true, false, 0.0); result[0] = vec4<f32>(f.cells[0],f.weights.xy);",
    );
    assert_eq!(nearest, [2.0, 2.0, 1.0, 0.0]);
    let snapped = run(
        &context,
        "result[0] = scene_atlas_frames(vec3<f32>(1.0,0.0,4.0), vec2<f32>(5.0), true, true, 0.25).weights;",
    );
    // Coordinate 2.4 approaches nearest 2 by a quarter: 2.3, hence 0.7/0/0.3.
    for (actual, expected) in snapped.iter().zip([0.7, 0.0, 0.3, 0.0]) {
        assert!((actual - expected).abs() < 1e-6);
    }
}

#[test]
fn equal_area_relief_normals_match_independent_sphere_oracle() {
    let context = context();
    for [x, y, z] in [
        [0.0_f64, 0.0, 1.0],
        [0.6, 0.0, 0.8],
        [-0.6, 0.8, 0.0],
        [0.0, 0.8, -0.6],
    ] {
        let a = x / (z + 1.0).sqrt() / 8.0_f64.sqrt() + 0.5;
        let b = y / (z + 1.0).sqrt() / 8.0_f64.sqrt() + 0.5;
        let values = run(
            &context,
            &format!("result[0]=vec4<f32>(scene_relief_decode(vec2<f32>({a},{b})),1.0);"),
        );
        for (actual, expected) in values.iter().zip([x, y, z, 1.0]) {
            assert!((f64::from(*actual) - expected).abs() < 0.000_002);
        }
    }
}
#[test]
fn distance_fade_endpoints_and_capsule_force_match_independent_oracles() {
    let context = context();
    let result = run(
        &context,
        "result[0]=vec4<f32>(scene_distance_coverage(1.0,vec2(2.0,4.0)),scene_distance_coverage(2.0,vec2(2.0,4.0)),scene_distance_coverage(3.0,vec2(2.0,4.0)),scene_distance_coverage(4.0,vec2(2.0,4.0)));",
    );
    assert_eq!(result, [0.0, 0.0, 0.5, 1.0]);
    for (position, expected) in [
        ("0.0,0.0,0.0", [2.0, 0.0, 0.0, 0.0]),
        ("2.0,0.0,0.0", [1.0, 0.0, 0.0, 0.0]),
        ("0.0,0.0,3.0", [0.0, 0.0, 1.0, 0.0]),
        ("3.0,0.0,0.0", [0.0; 4]),
    ] {
        let actual = run(
            &context,
            &format!(
                "result[0]=vec4<f32>(scene_static_modifier(vec3({position}),vec4(0.0,0.0,0.0,1.0),vec4(1.0,3.0,2.0,0.0)),0.0);"
            ),
        );
        assert_eq!(actual, expected);
    }
}

#[test]
fn cancelled_primary_force_keeps_independent_radial_response() {
    let context = context();
    let actual = run(
        &context,
        "let input=SceneBendInput(vec3(1.0,0.0,2.0),vec3(0.0,0.0,1.0),vec3(0.0),vec2(2.0,1.0),1.0,1.0); result[0]=vec4(scene_curved_position_forces(input,vec3(1.0,0.0,0.0)),0.0);",
    );
    // Independent quarter-circle integrals yield x=4/pi and unchanged axial Z.
    // WGSL inverse trig permits 4096 ULP; sin/cos absolute error is 2^-11
    // after range reduction. Propagating both through the integrals is <0.002.
    assert!((actual[0] - 4.0 / std::f32::consts::PI).abs() < 0.002);
    assert_eq!(&actual[1..], &[0.0, 2.0, 0.0]);
}

/// Coverage is a subtract and a divide in f32 on values up to 5 (Vulkan allows 2.5 ULP for the
/// divide), so it is within the shared arithmetic allowance at range 5 plus one ULP of the
/// exact dyadic value, not bit identical on every driver.
fn assert_coverage(actual: f32, expected: f64) {
    let bound =
        worldrunner_render_gpu::tolerance_test_support::ARITHMETIC * 5.0 + 2.0_f64.powi(-23);
    assert!(
        (f64::from(actual) - expected).abs() <= bound,
        "coverage {actual} versus {expected}, bound {bound}"
    );
}

#[test]
fn terminal_fade_matches_independent_linear_coverage() {
    let context = context();
    for (distance, expected) in [
        (1.0, 1.0),
        (2.0, 1.0),
        (2.5, 0.75),
        (3.0, 0.5),
        (4.0, 0.0),
        (5.0, 0.0),
    ] {
        let actual = run(
            &context,
            &format!(
                "result[0]=vec4(scene_distance_coverage({distance},vec2(2.0,-4.0)),0.0,0.0,0.0);"
            ),
        );
        assert_coverage(actual[0], expected);
    }
    let actual = run(
        &context,
        "result[0]=vec4(scene_distance_coverage(2.0,vec2(2.0,-2.5)),scene_distance_coverage(2.25,vec2(2.0,-2.5)),scene_distance_coverage(2.5,vec2(2.0,-2.5)),scene_distance_coverage(3.0,vec2(2.0,-2.5)));",
    );
    for (actual, expected) in actual.into_iter().zip([1.0, 0.25, 0.0, 0.0]) {
        assert_coverage(actual, expected);
    }
}

#[test]
fn per_vertex_fold_samples_local_slope_before_wind() {
    let context = context();
    let source = ENVIRONMENT_WGSL
        .replace(
            "@group(1) @binding(0) var<uniform> material: Material;",
            "var<private> material: Material;",
        )
        .replace(
            "@group(0) @binding(0) var<uniform> frame: Frame;",
            "var<private> frame: Frame;",
        );
    let ground = [
        0.0, 0.0, 1.0, 3.0, 2.0, 1.0, 3.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
    ];
    for (policy, expected) in [
        (0_u32, [0.0_f32, 0.0, 1.0]),
        (
            1,
            [
                -std::f32::consts::FRAC_1_SQRT_2,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
        ),
    ] {
        let body = format!(
            r"
@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    material.contact_surface=vec4(0.0,0.0,2.0,0.0);
    material.wind_clock.w={policy}u;
    var instance: InstanceInput;
    instance.model_column_0=vec4(1.0,0.0,0.0,0.0);
    instance.model_column_1=vec4(0.0,1.0,0.0,0.0);
    instance.model_column_2=vec4(0.0,0.0,1.0,0.0);
    instance.model_column_3=vec4(0.25,0.25,0.0,1.0);
    var vertex: VertexInput;
    vertex.position=vec3(1.0,0.0,0.0);
    vertex.normal=vec3(0.0,0.0,1.0);
    result[0]=material_vertex(vertex,instance).ground_fold;
}}"
        );
        let actual = evaluate_source(&context, &source, &body, None, Some(&ground));
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 1e-6, "{policy}: {actual:?}");
        }
        assert_eq!(actual[3].to_bits(), 0.0_f32.to_bits());
    }
}
