use super::*;
use worldrunner_render_plan::{SceneSsrGridDomain, SceneSsrTraversal};

fn contact_source(policy: &SceneSsr, depths: [f32; 2]) -> String {
    let mut source = grid::trace_source(1, false, policy, [65, 65]);
    let start = source.find("    let intersection = grid_walk(").unwrap();
    let end = start + source[start..].find("    if intersection.depth <").unwrap();
    let divisor = if policy.resolution_scale_micro == 500_000 {
        2.0
    } else {
        1.0
    };
    let walk = format!(
        "    let intersection = grid_walk(GridRay(vec2<f32>(41.25, 22.25)/{divisor},
        vec2<f32>(44.25, 22.25)/{divisor}, vec2<f32>({}, {}), vec2<f32>(1.0)),
        GridPolicy(grid_size, 1.0, 0.0, 256u, vec2<f32>(0.25, 0.0), 0u));\n",
        depths[0], depths[1]
    );
    source.replace_range(start..end, &walk);
    source
}

#[test]
fn gpu_grid_confidence_preserves_crossings_and_fades_thickness_edges() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for scale in [1_000_000, 500_000] {
        let policy = SceneSsr {
            traversal: SceneSsrTraversal::Grid,
            grid_domain: SceneSsrGridDomain::ReflectionOutput,
            resolution_scale_micro: scale,
            thickness_millimeters: 250,
            ..settings()
        };
        for (depths, confidence) in [
            ([4.75, 5.5], 1.0),
            ([5.125; 2], 1.0),
            ([5.1875; 2], 0.5),
            ([5.25; 2], 0.0),
        ] {
            let source = contact_source(&policy, depths);
            let bytes =
                controls_tests::capture_with_shader(&context, policy, 1.0, false, Some(&source));
            let size = 65u32.div_ceil(if scale == 500_000 { 2 } else { 1 });
            let x = size / 2;
            let y = if scale == 500_000 { 23 } else { 45 };
            let offset = usize::try_from((y * size + x) * 8).unwrap();
            crate::tolerance_tests::assert_half_pixel(
                &bytes[offset..offset + 8],
                [confidence, 0.0, 0.0, confidence],
                &format!("grid scale {scale}, depths {depths:?}"),
            );
        }
    }
}
