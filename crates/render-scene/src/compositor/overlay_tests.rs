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

#[test]
fn abandoned_overlay_output_does_not_publish_draw_statistics() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
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
                    corners: Some([[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]]),
                    ..DynamicQuad::default()
                }],
            },
        )
        .unwrap();
    let mut renderer = FrameRenderer::new(device);
    renderer.prepare_scene_inputs(device, &scene);
    let target = OffscreenTarget::new(
        device,
        Viewport {
            width: 1,
            height: 1,
        },
    )
    .unwrap();
    let output = target.color_view();
    let encode = |encoder: &mut wgpu::CommandEncoder,
                  _: &mut worldrunner_render_gpu::submission::Publications| {
        renderer.encode_output(encoder, output)
    };
    let abandoned = context.submission().prepare(device, encode).unwrap();
    assert_eq!(scene.statistics().passes.ui, DrawCount::default());
    drop(abandoned);
    assert_eq!(scene.statistics().passes.ui, DrawCount::default());
    context
        .submission()
        .prepare(device, encode)
        .unwrap()
        .submit()
        .unwrap();
    assert_eq!(
        scene.statistics().passes.ui,
        DrawCount {
            calls: 1,
            instances: 1
        }
    );
}
