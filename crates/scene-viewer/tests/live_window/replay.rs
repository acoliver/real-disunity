use worldrunner_render_scene::{
    device::GpuContext, draw::SceneGpu, frame::FrameUniform, offscreen::OffscreenTarget,
    renderer::ScenePipelines,
};
use worldrunner_scene_viewer::LiveDriver;

use super::driver::Driver;

/// Executes the same caller-owned simulation against the recorded desktop inputs.
pub(super) fn verify(desktop: &Driver) {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let mut scene = SceneGpu::empty(context.device(), context.submission(), &pipelines).unwrap();
    let mut replay = Driver::default();
    replay.initialize(desktop.ticks[0].viewport).unwrap();
    let mut target = OffscreenTarget::new(context.device(), desktop.ticks[0].viewport).unwrap();
    for (tick, expected) in desktop.ticks.iter().zip(&desktop.images) {
        let frame = replay.update(tick).unwrap();
        if let Some(textures) = &frame.textures {
            scene
                .set_dynamic_textures(context.device(), context.submission(), textures)
                .unwrap();
        }
        scene
            .set_dynamic_quads(
                context.device(),
                context.submission(),
                &pipelines,
                &frame.quads,
            )
            .unwrap();
        scene.set_frame(context.queue(), FrameUniform::unlit(&frame.view));
        if target.viewport() != tick.viewport {
            target = OffscreenTarget::new(context.device(), tick.viewport).unwrap();
        }
        let actual = target.render(&context, &scene, &pipelines).unwrap();
        assert_eq!(
            &actual, expected,
            "desktop and headless driver replay frame {}",
            tick.frame
        );
    }
    assert_eq!(replay.ticks.len(), desktop.ticks.len());
}
