//! The material mip bias that `prepare_uniform` puts in the frame block, through a configured
//! renderer on lavapipe: the shipped policy is exactly positive zero under temporal reconstruction
//! at every tested scale and in every other configuration, with or without a projection jitter
//! installed.
use super::taa_end_to_end_tests::{OUTPUT, SIZE, context, frame};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_plan::post::{RenderScale, UpscaleMode, UpscaleSettings};
use worldrunner_render_post::{FrameConfig, HdrPostSettings, PostResolve, ToneMap};

const OUTPUT_EXTENT: [u32; 2] = [SIZE, SIZE];
const SCALES: [u32; 3] = [500_000, 670_000, 770_000];

fn bias_of(scale_micro: Option<u32>, resolve: PostResolve, mode: Option<UpscaleMode>) -> f32 {
    let context = context();
    let mut frame_config = FrameConfig::new(OUTPUT_EXTENT, wgpu::TextureFormat::Rgba16Float, 1);
    if let Some(micro) = scale_micro {
        let render = RenderScale::new(micro)
            .unwrap()
            .extent(OUTPUT_EXTENT)
            .unwrap();
        frame_config = frame_config.with_render_extent(render);
    }
    let mut settings = RendererConfiguration::legacy(OUTPUT_EXTENT, OUTPUT);
    settings.frame = frame_config;
    settings.hdr = Some((1.0, ToneMap::Reinhard));
    settings.post = HdrPostSettings {
        resolve,
        upscale: mode.map(|mode| UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    };
    let mut renderer =
        ConfiguredFrameRenderer::new(&context, settings, AllocationBudget(1 << 28)).unwrap();
    let render = renderer.targets().config().size;
    renderer
        .renderer_mut()
        .prepare_uniform(frame(0, render[0]))
        .unwrap()
        .material_mip_bias
}

#[test]
fn temporal_reconstruction_carries_positive_zero_at_each_scale() {
    for scale in SCALES {
        let bias = bias_of(
            Some(scale),
            PostResolve::taa(0.9),
            Some(UpscaleMode::Temporal),
        );
        assert_eq!(bias.to_bits(), 0, "{scale}: bias {bias}");
    }
}

#[test]
fn every_other_configuration_carries_positive_zero() {
    let cases = [
        ("native TAA", None, PostResolve::taa(0.9), None),
        (
            "temporal at equal extents",
            None,
            PostResolve::taa(0.9),
            Some(UpscaleMode::Temporal),
        ),
        (
            "easu",
            Some(500_000),
            PostResolve::None,
            Some(UpscaleMode::Easu),
        ),
        (
            "bilinear",
            Some(670_000),
            PostResolve::None,
            Some(UpscaleMode::Bilinear),
        ),
        ("no upscale", None, PostResolve::None, None),
    ];
    for (name, scale, resolve, mode) in cases {
        let bias = bias_of(scale, resolve, mode);
        assert_eq!(bias.to_bits(), 0, "{name}: bias {bias}");
    }
}
