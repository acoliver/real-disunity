//! The projection jitter of a renderer built from a `RendererConfiguration`, observed through
//! the public `ConfiguredFrameRenderer` surface only: on construction and after a reconfiguration
//! that changes the render scale. No frame is rendered, so the tests need a device but no output.
use crate::{
    compositor::{ConfiguredFrameRenderer, RendererConfiguration},
    device::GpuContext,
};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{
    FrameConfig, HdrPostSettings, PostResolve, ProjectionJitter, ToneMap,
};

const OUTPUT_EXTENT: [u32; 2] = [64, 64];

fn context() -> GpuContext {
    pollster::block_on(GpuContext::request()).unwrap()
}

fn render_extent(scale_micro: u32) -> [u32; 2] {
    RenderScale::new(scale_micro)
        .unwrap()
        .extent(OUTPUT_EXTENT)
        .unwrap()
}

fn configuration(
    resolve: PostResolve,
    upscale: Option<UpscaleMode>,
    scale_micro: u32,
) -> RendererConfiguration {
    let render = render_extent(scale_micro);
    let mut settings =
        RendererConfiguration::legacy(OUTPUT_EXTENT, wgpu::TextureFormat::Rgba8UnormSrgb);
    settings.frame = FrameConfig::new(OUTPUT_EXTENT, wgpu::TextureFormat::Rgba16Float, 1)
        .with_render_extent(render);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.post = HdrPostSettings {
        resolve,
        upscale: upscale.map(|mode| UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    };
    settings
}

fn temporal(scale_micro: u32) -> RendererConfiguration {
    configuration(
        PostResolve::taa(0.9),
        Some(UpscaleMode::Temporal),
        scale_micro,
    )
}

fn build(context: &GpuContext, settings: RendererConfiguration) -> ConfiguredFrameRenderer {
    ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap()
}

fn cycle(scale_micro: u32) -> ProjectionJitter {
    ProjectionJitter::for_extents(render_extent(scale_micro), OUTPUT_EXTENT).unwrap()
}

#[test]
fn a_configured_temporal_renderer_selects_the_jitter_derived_from_both_extents() {
    let context = context();
    for scale in [500_000, 670_000, 770_000] {
        let renderer = build(&context, temporal(scale));
        let selected = renderer.projection_jitter();
        assert_eq!(selected, Some(cycle(scale)), "{scale}");
        assert!(selected.unwrap().length() > ProjectionJitter::DEFAULT_LENGTH);
    }
    assert_eq!(cycle(500_000).length(), 32);
}

#[test]
fn a_configured_native_taa_renderer_selects_the_default_sequence() {
    let context = context();
    let renderer = build(
        &context,
        configuration(PostResolve::taa(0.9), None, 1_000_000),
    );
    assert_eq!(
        renderer.projection_jitter(),
        Some(ProjectionJitter::default())
    );
}

#[test]
fn a_configured_renderer_without_taa_renders_unjittered() {
    let context = context();
    let renderer = build(&context, configuration(PostResolve::None, None, 1_000_000));
    assert_eq!(renderer.projection_jitter(), None);
}

#[test]
fn reconfiguring_the_render_scale_rederives_the_jitter_and_drops_the_history() {
    let context = context();
    let mut renderer = build(&context, temporal(500_000));
    assert_eq!(renderer.projection_jitter(), Some(cycle(500_000)));
    renderer.reconfigure(&context, temporal(670_000)).unwrap();
    assert_eq!(renderer.projection_jitter(), Some(cycle(670_000)));
    assert_ne!(cycle(500_000), cycle(670_000));
    assert!(!renderer.has_temporal_history());
    renderer.reconfigure(&context, temporal(500_000)).unwrap();
    assert_eq!(renderer.projection_jitter(), Some(cycle(500_000)));
}

#[test]
fn reconfiguring_between_temporal_native_taa_and_no_taa_follows_the_settings() {
    let context = context();
    let mut renderer = build(&context, temporal(500_000));
    renderer
        .reconfigure(
            &context,
            configuration(PostResolve::taa(0.9), None, 1_000_000),
        )
        .unwrap();
    assert_eq!(
        renderer.projection_jitter(),
        Some(ProjectionJitter::default())
    );
    renderer
        .reconfigure(&context, configuration(PostResolve::None, None, 1_000_000))
        .unwrap();
    assert_eq!(renderer.projection_jitter(), None);
    renderer.reconfigure(&context, temporal(770_000)).unwrap();
    assert_eq!(renderer.projection_jitter(), Some(cycle(770_000)));
}
