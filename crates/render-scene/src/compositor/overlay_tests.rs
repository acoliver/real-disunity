use super::FrameRenderer;
use crate::{
    Viewport,
    device::GpuContext,
    draw::{DrawCount, SceneGpu},
    dynamic::DynamicQuad,
    offscreen::OffscreenTarget,
    renderer::ScenePipelines,
    simulation::UiOverlay,
};

fn overlay_frame() -> crate::frame::FrameUniform {
    let view = crate::SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -5_000, 0],
            target_millimeters: [0; 3],
        },
        worldrunner_render_plan::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: 1,
            height: 1,
        },
    )
    .unwrap();
    crate::frame::FrameUniform::unlit(&view)
}

#[test]
fn abandoned_overlay_output_does_not_publish_draw_statistics() {
    let _session = crate::test_support::SceneTestSession::acquire();
    for depth_test in [false, true] {
        check_abandonment(depth_test, None);
    }
}
#[test]
fn fully_clipped_depth_tested_overlay_does_not_publish_draws() {
    let _session = crate::test_support::SceneTestSession::acquire();
    check_abandonment(true, Some([2, 2, 1, 1]));
}
fn check_abandonment(depth_test: bool, clip_rect: Option<[u32; 4]>) {
    let context = GpuContext::shared_for_test().unwrap();
    let device = context.device();
    let pipelines = ScenePipelines::new(device, OffscreenTarget::COLOR_FORMAT);
    let mut scene = SceneGpu::empty(device, context.submission(), &pipelines).unwrap();
    scene
        .set_ui_overlay(
            device,
            context.submission(),
            &UiOverlay {
                textures: vec![],
                quads: vec![DynamicQuad {
                    overlay: true,
                    depth_test,
                    clip_rect,
                    corners: Some([[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]]),
                    ..DynamicQuad::default()
                }],
            },
        )
        .unwrap();
    let mut renderer = FrameRenderer::new(device);
    renderer.prepare_scene_inputs(device, &scene);
    let snapshot = depth_test
        .then(|| crate::water::snapshots::Snapshots::new(device, renderer.targets().config()));
    renderer
        .overlay
        .prepare_depth(device, snapshot.as_ref(), &overlay_frame());
    let target = OffscreenTarget::new(
        device,
        Viewport {
            width: 1,
            height: 1,
        },
    )
    .unwrap();
    assert_submission(&context, &renderer, &scene, &target);
    let expected = if clip_rect.is_some() {
        DrawCount::default()
    } else {
        DrawCount {
            calls: 1,
            instances: 1,
        }
    };
    assert_eq!(scene.statistics().passes.ui, expected);
    assert_eq!(
        scene.statistics().passes.ui_depth_tested,
        if depth_test {
            expected
        } else {
            DrawCount::default()
        }
    );
}
fn assert_submission(
    context: &GpuContext,
    renderer: &FrameRenderer,
    scene: &SceneGpu,
    target: &OffscreenTarget,
) {
    let encode = |encoder: &mut wgpu::CommandEncoder,
                  _: &mut worldrunner_render_gpu::submission::Publications| {
        renderer.encode_output(encoder, target.color_view())
    };
    let abandoned = context
        .submission()
        .prepare(context.device(), encode)
        .unwrap();
    assert_eq!(scene.statistics().passes.ui, DrawCount::default());
    assert_eq!(
        scene.statistics().passes.ui_depth_tested,
        DrawCount::default()
    );
    drop(abandoned);
    assert_eq!(scene.statistics().passes.ui, DrawCount::default());
    assert_eq!(
        scene.statistics().passes.ui_depth_tested,
        DrawCount::default()
    );
    context
        .submission()
        .prepare(context.device(), encode)
        .unwrap()
        .submit()
        .unwrap();
}

#[test]
fn overlay_depth_inventory_reserves_snapshots_only_for_depth_consumers() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let limits = wgpu::Limits::default();
    let mut settings =
        super::RendererConfiguration::legacy([64, 64], OffscreenTarget::COLOR_FORMAT);
    let before = settings.allocations().validate(&limits).unwrap();
    settings.ui_depth = true;
    let with_depth = settings.allocations().validate(&limits).unwrap();
    assert_eq!(
        with_depth - before,
        64 * 64 * (3 * 8 + 6 * 4) + 3 * (304 + 192 + 80)
    );
    settings.ui_depth = false;
    assert_eq!(settings.allocations().validate(&limits).unwrap(), before);
}
