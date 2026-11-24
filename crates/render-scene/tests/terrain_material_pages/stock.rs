use super::*;
use worldrunner_render_post::PassEntry;
use worldrunner_render_post::PassId;
use worldrunner_render_post::PassSlot;
use worldrunner_render_post::TimingMode;
use worldrunner_render_scene::compositor::FrameRenderer;
use worldrunner_render_scene::device::DeviceRequest;
use worldrunner_render_scene::draw::PreparedScene;

#[test]
fn terrain_pages_disabled_preserves_exact_pixels_and_stock_pass_ids() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let context = pollster::block_on(GpuContext::request(&DeviceRequest {
        features: wgpu::Features::TIMESTAMP_QUERY,
        ..DeviceRequest::default()
    }))
    .unwrap();
    let view = page_view();
    let pipelines =
        ScenePipelines::new(context.device(), OffscreenTarget::COLOR_FORMAT, 1).unwrap();
    let mut gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        PreparedScene::new(&fixture::scene(false)).unwrap(),
        &pipelines,
        None,
    )
    .unwrap();
    let uniform = worldrunner_render_scene::frame::FrameUniform::unlit(&view);
    gpu.set_frame(context.queue(), uniform);
    let mut renderer = FrameRenderer::new(context.device());
    renderer
        .configure_timing(context.device(), context.queue(), TimingMode::Required)
        .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    for frame in 0..2 {
        let mut encoder = context.submission().begin(context.device()).unwrap();
        renderer.begin_timing(frame).unwrap();
        renderer
            .encoding()
            .encode_scene(
                context.device(),
                &mut encoder,
                (&mut gpu, &pipelines),
                (&target.color_attachment(), &target.depth_attachment()),
                uniform,
            )
            .unwrap();
        renderer.finish_timing(&mut encoder).unwrap();
        target.encode_readback(&mut encoder).unwrap();
        let submission = encoder.finish().submit().unwrap();
        let image = target.read_encoded(&context, submission).unwrap();
        assert_eq!(image.pixels, vec![255; 9 * 9 * 4]);
        let timings =
            worldrunner_render_scene::test_support::take_timings(context.device(), 1, || {
                renderer.take_timings()
            });
        assert_eq!(timings.len(), 1);
        assert_eq!(
            timings[0]
                .passes
                .iter()
                .map(|pass| pass.pass_id)
                .collect::<Vec<_>>(),
            vec![PassId::Geometry(PassEntry::new(PassSlot::Opaque))]
        );
    }
}
