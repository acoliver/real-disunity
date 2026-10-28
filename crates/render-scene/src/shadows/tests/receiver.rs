use super::*;

fn receiver_scene() -> RenderScene {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for y in 0_i32..=16 {
        for x in 0_i32..=16 {
            let px = (x - 8) * 100;
            let py = (y - 8) * 100;
            vertices.push(SceneVertex {
                tangent_micro: None,
                position_millimeters: [
                    i64::from(px),
                    i64::from(py),
                    i64::from(1_000 - (px * px + py * py) / 2_000),
                ],
                normal_micro: [i64::from(px) * 1_000, i64::from(py) * 1_000, 1_000_000],
                texcoord_micro: [i64::from(x) * 62_500, i64::from(y) * 62_500],
            });
            if x < 16 && y < 16 {
                let i = u32::try_from(y * 17 + x).unwrap();
                indices.extend([i, i + 1, i + 18, i, i + 18, i + 17]);
            }
        }
    }
    let dome =
        SceneMesh::new(vec![SceneMeshLod::new(0, vertices, indices, None).unwrap()]).unwrap();
    fixture::caster_scene(vec![fixture::object([0, 0, 0], 1, 1)], dome, None, false)
}

const SCALAR_RECEIVER: &str = r"
fn raw_sunlight_visibility(position: vec3<f32>, normal: vec3<f32>) -> f32 {
    let clip = shadow.clip_from_world[0] * vec4<f32>(position + normal * shadow.bias.y, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    let gradient = rd_shadow_plane_gradient(dpdx(uv), dpdy(uv), dpdx(ndc.z), dpdy(ndc.z));
    return cascade_visibility(ndc, gradient, 0);
}
";

fn scalar_pipelines(context: &GpuContext, renderer: &ShadowedScene) -> ScenePipelines {
    let original = include_str!("../../shader/shadows.wgsl");
    let (prefix, rest) = original.split_once("fn raw_sunlight_visibility").unwrap();
    let (_, suffix) = rest.split_once("fn direct_sunlight_visibility").unwrap();
    let receiver = format!("{prefix}{SCALAR_RECEIVER}fn direct_sunlight_visibility{suffix}");
    let source = resources::SHADOWED_WGSL.replace(original, &receiver);
    let terrain = resources::SHADOWED_TERRAIN_WGSL.replace(original, &receiver);
    ScenePipelines::with_screen_space(
        context.device(),
        (OffscreenTarget::COLOR_FORMAT, 1),
        &source,
        &terrain,
        Some(&renderer.shadow_layout),
        None,
    )
}

#[test]
fn gpu_single_map_receiver_matches_scalar_reference_exactly() {
    let context = context();
    let source = receiver_scene();
    let fit = DirectionalShadowSettings {
        half_extent_meters: 8.0,
        depth_half_extent_meters: 16.0,
        depth_bias: 0.0008,
        normal_bias_meters: 0.012,
        ..settings()
    };
    let mut renderer = ShadowedScene::new(&context, &source, fit).unwrap();
    let mut reference = scalar_pipelines(&context, &renderer);
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for offset in 0..32 {
        let camera = SceneView::new(
            crate::CameraPose {
                eye_millimeters: [offset * 113, -4_000, 2_000 + offset * 137],
                target_millimeters: [0, 0, 0],
            },
            *source.effects().unwrap().camera(),
            view().viewport(),
        )
        .unwrap();
        render_target(&mut renderer, &context, &camera, &target).unwrap();
        let actual = target.read_rgba8(&context).unwrap();
        std::mem::swap(&mut renderer.pipelines, &mut reference);
        render_target(&mut renderer, &context, &camera, &target).unwrap();
        let expected = target.read_rgba8(&context).unwrap();
        std::mem::swap(&mut renderer.pipelines, &mut reference);
        let changed = actual
            .pixels
            .chunks_exact(4)
            .zip(expected.pixels.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(changed, 0, "single-map receiver differs at view {offset}");
    }
}
