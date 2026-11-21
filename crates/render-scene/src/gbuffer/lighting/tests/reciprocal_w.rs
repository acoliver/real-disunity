//! GPU regression for preserving homogeneous inverse w through fragment preparation.
use super::{ENVIRONMENT_WGSL, LIGHTING_WGSL, camera_bytes};
use crate::shader::gpu_math_tests::{configured_frame, evaluate_source};

#[test]
fn fragment_preserves_homogeneous_reciprocal_clip_w() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context =
        crate::device::GpuContext::shared_for_test_with_features(wgpu::Features::empty()).unwrap();
    let shader = fragment_source();
    let translation = 16_777_216.0;
    // Unit x/y scale, camera at Z=2^24 looking along -Z, near=1 and far=2.
    let orthographic = glam::Mat4::from_cols(
        glam::Vec4::X,
        glam::Vec4::Y,
        -glam::Vec4::Z,
        glam::Vec4::new(0.0, 0.0, translation - 1.0, 1.0),
    );
    let perspective = glam::Mat4::from_cols(
        glam::Vec4::X,
        glam::Vec4::Y,
        glam::Vec4::new(0.0, 0.0, -2.0, -1.0),
        glam::Vec4::new(0.0, 0.0, 2.0 * translation - 2.0, translation),
    );
    // At depth=1/2, H.z=12_582_911 and H.w=3/4 exactly. Round-to-nearest
    // P.z is 2^24-1, whose reprojected reciprocal w is 1 instead of 3/4.
    let mut observations = Vec::new();
    for (name, matrix) in [("orthographic", orthographic), ("perspective", perspective)] {
        let mut frame = configured_frame(
            worldrunner_render_lighting::ExposureOwnership::SceneLinear(1.0),
        );
        frame.view_projection = matrix;
        for depth in [0.0, 0.5, 1.0] {
            let entry = fragment_entry(matrix, depth);
            let actual = evaluate_source(&context, &shader, &entry, Some(&frame), None);
            // The exact inverse bottom row gives 1 for ortho, 1-depth/2 for perspective.
            let expected = if name == "orthographic" {
                1.0
            } else {
                1.0 - depth * 0.5
            };
            observations.push((name, depth, expected, actual));
        }
    }
    for (name, depth, expected, actual) in observations {
        assert_eq!(
            actual[1].to_bits(),
            expected.to_bits(),
            "{name} depth={depth}: H.w"
        );
        assert_eq!(
            actual[0].to_bits(),
            actual[1].to_bits(),
            "{name} depth={depth}: fragment.w={} must preserve H.w={}",
            actual[0],
            actual[1]
        );
    }
}

fn fragment_source() -> String {
    // Keep production reconstruction, surface decoding and fragment preparation intact.
    // Only resource reads become synthetic inputs; lighting after preparation is outside scope.
    let (setup, _) = LIGHTING_WGSL.split_once("// SCENE_DEFERRED_PIXEL").unwrap();
    let mut setup = setup
        .lines()
        .filter(|line| !line.starts_with("@group(1)"))
        .collect::<Vec<_>>()
        .join("\n");
    for (resource, value) in [
        ("g0", "vec4<f32>(0.5, 0.5, 0.0, 0.0)"),
        ("g1", "vec4<f32>(0.5, 0.5, 0.5, 0.0)"),
        ("g2", "vec4<f32>(1.0, 0.0, 0.0, 1.0)"),
        (
            "g3",
            "vec4<u32>(scene_gbuffer_g3_encode(vec3<f32>(0.0, 0.0, 1.0), 0u))",
        ),
        ("depth", "test_depth"),
    ] {
        setup = setup.replace(
            &format!("textureLoad(scene_deferred_{resource}, texel, 0)"),
            value,
        );
    }
    setup = setup.replace("textureDimensions(scene_deferred_depth)", "vec2<u32>(1u)");
    // Observe H before division without reproducing any reconstruction arithmetic.
    let (before, after) = setup.split_once("inverse[3])));").unwrap();
    format!(
        "{ENVIRONMENT_WGSL}\n{}\n\
var<private> scene_deferred_camera: SceneDeferredCamera;
var<private> scene_lighting_records: array<SceneLightingRecord, 1>;
var<private> test_depth: f32;
var<private> test_homogeneous: vec4<f32>;
{before}inverse[3])));
    test_homogeneous = scene;
{after}
    return scene_deferred_fragment;
}}",
        crate::gbuffer::GBUFFER_WGSL
    )
}

fn fragment_entry(matrix: glam::Mat4, depth: f32) -> String {
    let camera = camera_bytes(matrix, glam::Mat4::IDENTITY);
    let columns = camera[..64]
        .chunks_exact(16)
        .map(|column| {
            let words = column
                .chunks_exact(4)
                .map(|word| format!("{}u", u32::from_le_bytes(word.try_into().unwrap())))
                .collect::<Vec<_>>()
                .join(", ");
            format!("bitcast<vec4<f32>>(vec4<u32>({words}))")
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "@group(0) @binding(1) var<storage, read_write> result: array<vec4<f32>>;
@compute @workgroup_size(1) fn evaluate_math() {{
    scene_deferred_camera.scene_from_clip = mat4x4<f32>({columns});
    scene_deferred_camera.scene_from_view = mat3x3<f32>(
        vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    test_depth = bitcast<f32>({}u);
    let fragment = scene_deferred_shade(vec4<f32>(0.5, 0.5, 0.0, 1.0));
    result[0] = vec4<f32>(fragment.w, test_homogeneous.w, fragment.xy);
}}",
        depth.to_bits()
    )
}
