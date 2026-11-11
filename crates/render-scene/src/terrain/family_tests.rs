//! One scene drawn by pipelines that were built independently of each other.
//!
//! Every `ScenePipelines` made on its own, and every renderer replacement that builds the
//! forward reflection pipelines again, has its own program runtime. A scene's terrain keeps what
//! it prepared for the runtime that drew it first, so each draw has to find or make what its own
//! runtime needs, whatever drew the scene before it.

use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::{RenderScene, SceneSsr};
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef, ToneMap};

use super::capture_tests::{Case, Rig, Source, Target, capture, drawable, scenes};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use crate::device::GpuContext;
use crate::draw::SceneGpu;
use crate::dynamic::capture_tests::{Built, CONFIGS, SIZE, build, context, supported};
use crate::frame::FrameUniform;
use crate::renderer::ScenePipelines;

/// Plain, ambient occlusion and reflection target sets: the three kinds of screen group.
const SETS: [usize; 3] = [0, 6, 8];

fn case<'a>(built: &'a Built, name: &'a str, source: Source, samples: u32) -> Case<'a> {
    Case {
        name,
        source: source.name(),
        target: Target {
            formats: &built.formats,
            samples,
        },
        pipelines: &built.pipelines,
        screen: built.screen.as_ref(),
        shadow: None,
    }
}

/// Draws one uploaded scene through two independently built sets of pipelines, then through the
/// first again. Each draw is compared with the fixed path by [`capture`], and the three agree.
fn through_independent_pipelines(source: Source) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut compared = 0;
    for &index in &SETS {
        let config = &CONFIGS[index];
        if !supported(&context, config) || !drawable(&context, source) {
            continue;
        }
        let (first, second) = (
            build(context.device(), config),
            build(context.device(), config),
        );
        let rig = Rig::new(
            &context,
            &first.pipelines,
            &source.scene(),
            (source, scenes::frame(scenes::pose())),
        );
        let drew =
            |built: &Built| capture(&rig, &case(built, config.name, source, config.samples)).images;
        let (a, b, again) = (drew(&first), drew(&second), drew(&first));
        assert_eq!(
            a,
            b,
            "{} {}: the second pipelines",
            config.name,
            source.name()
        );
        assert_eq!(
            a,
            again,
            "{} {}: back to the first",
            config.name,
            source.name()
        );
        compared += 1;
    }
    assert!(compared > 0, "no target set could run");
}

#[test]
fn palette_terrain_draws_through_independently_built_pipelines() {
    through_independent_pipelines(Source::Plain);
}

#[test]
fn paged_terrain_draws_through_independently_built_pipelines() {
    through_independent_pipelines(Source::Pages);
}

const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

fn request() -> SceneSsr {
    SceneSsr {
        enabled: true,
        max_steps: 64,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 1_000_000,
        ..SceneSsr::default()
    }
}

fn configuration() -> RendererConfiguration {
    let mut settings =
        RendererConfiguration::legacy([SIZE; 2], wgpu::TextureFormat::Rgba8UnormSrgb);
    settings.frame = FrameConfig::new([SIZE; 2], HDR, 1);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.ssr = Some(request());
    settings
}

/// Encodes `frames` frames of the scene through `renderer` and reads back the last.
fn render(
    context: &GpuContext,
    renderer: &mut ConfiguredFrameRenderer,
    (gpu, pipelines): (&mut SceneGpu, &ScenePipelines),
    (frame, frames): (FrameUniform, usize),
) -> Vec<u8> {
    for _ in 0..frames {
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut encoder = context.submission().begin(context.device()).unwrap();
        renderer
            .encoding()
            .encode_scene(
                context.device(),
                &mut encoder,
                (&mut *gpu, pipelines),
                (&color, &depth),
                frame,
            )
            .unwrap();
        renderer.targets().encode_resolve(&mut encoder);
        encoder.finish().submit().unwrap();
    }
    Readback::new(
        context.device(),
        FrameConfig::new([SIZE; 2], HDR, 1),
        1_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        renderer.targets().texture(),
    )
    .unwrap()
}

fn renderer(context: &GpuContext) -> ConfiguredFrameRenderer {
    ConfiguredFrameRenderer::new(context, configuration(), AllocationBudget(1 << 28)).unwrap()
}

/// The terrain of `source` is drawn by the forward pipelines of a reflecting renderer. Replacing
/// the renderer's configuration builds those pipelines again, and a second renderer builds its
/// own, so the same uploaded scene meets three program runtimes in turn.
fn through_reconfiguration(source: Source) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let scene: RenderScene = scenes::reflecting(request());
    let frame = scenes::frame_with(
        scenes::pose(),
        scene.effects().expect("the scene has effects"),
    );
    let pipelines = ScenePipelines::new(context.device(), HDR);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &scene, &pipelines).unwrap();
    gpu.set_frame(context.queue(), frame);
    if source.pages() {
        gpu.enable_terrain_pages(
            context.device(),
            context.submission(),
            scenes::page_source(false),
        )
        .unwrap();
    }
    let mut first = renderer(&context);
    let before = render(&context, &mut first, (&mut gpu, &pipelines), (frame, 4));
    assert!(
        !source.pages() || gpu.terrain_pages_ready(),
        "pages did not become resident"
    );
    let drawn = |image: &[u8]| image.chunks(8).any(|texel| texel != &image[..8]);
    assert!(
        drawn(&before),
        "{}: the terrain drew nothing",
        source.name()
    );

    first
        .reconfigure(&context, configuration())
        .expect("the replacement is valid");
    let replaced = render(&context, &mut first, (&mut gpu, &pipelines), (frame, 1));
    let mut fresh = renderer(&context);
    let control = render(&context, &mut fresh, (&mut gpu, &pipelines), (frame, 1));
    assert!(drawn(&replaced));
    assert_eq!(
        replaced,
        control,
        "{}: the replaced renderer draws what a new one does",
        source.name()
    );
}

#[test]
fn palette_terrain_survives_a_reflecting_renderer_replacement() {
    through_reconfiguration(Source::Plain);
}

#[test]
fn paged_terrain_survives_a_reflecting_renderer_replacement() {
    through_reconfiguration(Source::Pages);
}
