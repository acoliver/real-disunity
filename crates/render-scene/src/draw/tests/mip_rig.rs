//! Shared rig for the material mip bias tests that go through a whole renderer. The renderer
//! derives the bias from its configuration, so the biased frame comes from a renderer that
//! reconstructs 32x32 into 64x64 temporally with its bias forced to exactly -2 (the derived bias
//! there is -1.5, which would put the colour between two levels) and the reference from a plain
//! 32x32 renderer (bias +0.0), both drawing the same scene at the same camera. The textures have a constant colour per mip level.
use super::mip_bias::LEVELS;
use super::*;
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::post::{UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{
    AttachmentView, FrameConfig, HdrPostSettings, PostResolve, Readback, ResourceRef, ToneMap,
};

pub(super) const RENDER: u32 = 32;
const HDR: TextureFormat = TextureFormat::Rgba16Float;
/// Largest linear step between adjacent level colours is 190/255 = 0.745. The hardware resolves
/// the LOD to 1/256 (0.0029 of that step), and the half float scene target rounds a value below 1
/// by at most 2^-11 = 0.0005. The sum, 0.0034, is rounded up to 0.005.
const BOUND: f32 = 0.005;

pub(super) fn plain(context: &GpuContext) -> ConfiguredFrameRenderer {
    let mut settings = RendererConfiguration::legacy([RENDER; 2], COLOR_FORMAT);
    settings.frame = FrameConfig::new([RENDER; 2], HDR, 1);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap()
}

/// Two whole levels, so every constant colour level is hit exactly.
const FORCED_BIAS: f32 = -2.0;

pub(super) fn temporal(context: &GpuContext) -> ConfiguredFrameRenderer {
    let output = [2 * RENDER; 2];
    let mut settings = RendererConfiguration::legacy(output, COLOR_FORMAT);
    settings.frame = FrameConfig::new(output, HDR, 1).with_render_extent([RENDER; 2]);
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.post = HdrPostSettings {
        resolve: PostResolve::taa(0.9),
        upscale: Some(UpscaleSettings {
            mode: UpscaleMode::Temporal,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    };
    let renderer =
        ConfiguredFrameRenderer::new(context, settings, AllocationBudget(1 << 28)).unwrap();
    renderer.force_material_mip_bias(Some(FORCED_BIAS));
    renderer
}

/// Draws one frame through the public encoding boundary and reads the resolved render target.
pub(super) fn center(
    context: &GpuContext,
    renderer: &mut ConfiguredFrameRenderer,
    scene: &RenderScene,
    frame: &FrameUniform,
) -> [f32; 3] {
    let pipelines = ScenePipelines::new(context.device(), HDR);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), scene, &pipelines).unwrap();
    // The resident scene has no post effects of its own; it adopts the renderer's requested ones.
    gpu.post = renderer.configuration().post;
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
            (&mut gpu, &pipelines),
            (&color, &depth),
            *frame,
        )
        .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    let image = Readback::new(
        context.device(),
        FrameConfig::new([RENDER; 2], HDR, 1),
        1_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        renderer.targets().texture(),
    )
    .unwrap();
    let at = usize::try_from((RENDER / 2 * RENDER + RENDER / 2) * 8).unwrap();
    let half = |channel: usize| {
        let bits = u16::from_le_bytes([image[at + 2 * channel], image[at + 2 * channel + 1]]);
        half::f16::from_bits(bits).to_f32()
    };
    [half(0), half(1), half(2)]
}

pub(super) fn assert_level(actual: [f32; 3], level: usize, what: &str) {
    for (got, wanted) in actual.iter().zip(LEVELS[level]) {
        assert!(
            (got - f32::from(wanted) / 255.0).abs() <= BOUND,
            "{what}: {actual:?} is not level {level} {:?}",
            LEVELS[level]
        );
    }
}
