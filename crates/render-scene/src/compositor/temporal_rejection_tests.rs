//! Unsupported temporal combinations through the public `ConfiguredFrameRenderer` surface: each is
//! a typed `UnsupportedProfile` from `new` and from `reconfigure`, and a rejected replacement
//! leaves the previous renderer selecting the same jitter and rendering the same first frame as a
//! fresh renderer of its configuration. Frames compare bytes of one code path on one device only.
use super::temporal_configuration_tests::{build, configuration, context, temporal};
use crate::{
    compositor::{ConfiguredFrameRenderer, RendererConfiguration},
    device::GpuContext,
};
use worldrunner_render_gpu::configuration::{AllocationBudget, ConfigurationError};
use worldrunner_render_plan::post::UpscaleMode;
use worldrunner_render_post::{
    Cmaa2Settings, FrameConfig, PostError, PostResolve, Readback, SmaaSettings,
};

const HALF: u32 = 500_000;

fn rejected(settings: &RendererConfiguration, error: Option<ConfigurationError<PostError>>) {
    assert!(
        matches!(
            error,
            Some(ConfigurationError::Allocation(
                PostError::UnsupportedProfile(_)
            ))
        ),
        "{:?} {:?}",
        settings.post.resolve,
        error.map(|error| error.to_string())
    );
}

fn without_taa(resolve: PostResolve) -> RendererConfiguration {
    configuration(resolve, Some(UpscaleMode::Temporal), HALF)
}

fn multisampled(samples: u32) -> RendererConfiguration {
    let mut settings = temporal(HALF);
    settings.frame.samples = samples;
    settings
}

fn below_half() -> RendererConfiguration {
    let mut settings = temporal(HALF);
    settings.frame = settings.frame.with_render_extent([31, 32]);
    settings
}

/// Every combination the design lists as unsupported for reduced temporal reconstruction that a
/// host can express in a `RendererConfiguration`. A standalone `SpatialUpscale` stage and the
/// legacy profile are not expressible there; the stage-level tests cover the first.
fn unsupported() -> Vec<RendererConfiguration> {
    vec![
        without_taa(PostResolve::None),
        without_taa(PostResolve::fxaa(1.0)),
        without_taa(PostResolve::Smaa(SmaaSettings::default())),
        without_taa(PostResolve::Cmaa2(Cmaa2Settings::default())),
        multisampled(4),
        below_half(),
    ]
}

fn first_frame(context: &GpuContext, renderer: &mut ConfiguredFrameRenderer) -> Vec<u8> {
    let device = context.device();
    let size = renderer.configuration().frame.size;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let output = worldrunner_render_post::color_texture(device, "rejection output", size, format);
    let mut image = Readback::new(device, FrameConfig::new(size, format, 1), 100_000).unwrap();
    renderer.set_frame_delta(0.25).unwrap();
    let prepared = context
        .submission()
        .prepare(device, |encoder, _| {
            clear(encoder, renderer);
            let view = output.create_view(&wgpu::TextureViewDescriptor::default());
            renderer.encode_output(encoder, &view).unwrap();
            image.encode(encoder, &output).unwrap();
            Ok::<_, ()>(())
        })
        .unwrap();
    let submission = prepared.submit().unwrap();
    device.poll(wgpu::PollType::Poll).unwrap();
    image.read_encoded(device, Some(submission)).unwrap()
}

fn clear(encoder: &mut wgpu::CommandEncoder, renderer: &ConfiguredFrameRenderer) {
    let color = wgpu::Color {
        r: 0.25,
        g: 0.5,
        b: 0.125,
        a: 1.0,
    };
    let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: renderer.targets().scene(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });
}

#[test]
fn construction_rejects_every_unsupported_temporal_combination_with_a_typed_error() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for settings in unsupported() {
        let error =
            ConfiguredFrameRenderer::new(&context, settings.clone(), AllocationBudget(1 << 28))
                .err();
        rejected(&settings, error);
    }
}

#[test]
fn a_rejected_reconfiguration_leaves_a_renderer_that_still_renders_like_a_fresh_one() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut control = build(&context, temporal(HALF));
    let expected = first_frame(&context, &mut control);
    for settings in unsupported() {
        let mut renderer = build(&context, temporal(HALF));
        let (jitter, generation) = (renderer.projection_jitter(), renderer.generation());
        rejected(
            &settings,
            renderer.reconfigure(&context, settings.clone()).err(),
        );
        assert_eq!(renderer.projection_jitter(), jitter);
        assert_eq!(renderer.generation(), generation);
        assert_eq!(first_frame(&context, &mut renderer), expected);
    }
}
