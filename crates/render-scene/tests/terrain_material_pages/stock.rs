use super::*;
use worldrunner_render_scene::{
    compositor::{FrameRenderer, encode_resident},
    executor::{PassId, PassSlot, TimingMode},
};

#[test]
fn terrain_pages_disabled_preserves_exact_pixels_and_stock_pass_ids() {
    let context = pollster::block_on(GpuContext::request_with_features(
        wgpu::Features::TIMESTAMP_QUERY,
    ))
    .unwrap();
    let view = page_view();
    let pipelines = ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT);
    let gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &fixture::scene(false),
        &pipelines,
    )
    .unwrap();
    gpu.set_frame(
        context.queue(),
        worldrunner_render_scene::frame::FrameUniform::unlit(&view),
    );
    let mut renderer = FrameRenderer::new(context.device());
    renderer
        .configure_timing(context.device(), context.queue(), TimingMode::Required)
        .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    for frame in 0..2 {
        let mut encoder = context.submission().begin(context.device()).unwrap();
        renderer.begin_timing(frame).unwrap();
        encode_resident(
            &mut renderer,
            context.device(),
            &mut encoder,
            (&gpu, &pipelines),
            (&target.color_attachment(), &target.depth_attachment()),
        )
        .unwrap();
        renderer.finish_timing(&mut encoder).unwrap();
        target.encode_readback(&mut encoder).unwrap();
        let submission = encoder.finish().submit().unwrap();
        let image = target.read_encoded(&context, submission).unwrap();
        assert_eq!(image.pixels, vec![255; 9 * 9 * 4]);
        let timings = renderer.take_timings().unwrap();
        assert_eq!(timings.len(), 1);
        assert_eq!(
            timings[0]
                .passes
                .iter()
                .map(|pass| pass.pass_id)
                .collect::<Vec<_>>(),
            vec![PassId::Geometry(PassSlot::Opaque)]
        );
    }
}
