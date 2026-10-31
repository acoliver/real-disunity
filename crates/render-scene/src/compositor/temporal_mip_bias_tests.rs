//! The material mip bias that `prepare_uniform` puts in the frame block, through a configured
//! renderer on lavapipe: log2 of the smaller render-to-output axis ratio minus one under temporal
//! reconstruction, and exactly positive zero in every other configuration, with or without a
//! projection jitter installed.
use super::taa_end_to_end_tests::{OUTPUT, SIZE, context, frame};
use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_post::{
    FrameConfig, HdrPostSettings, PostResolve, RenderScale, ToneMap, UpscaleMode, UpscaleSettings,
};

const OUTPUT_EXTENT: [u32; 2] = [SIZE, SIZE];
const SCALES: [u32; 3] = [500_000, 670_000, 770_000];
/// One f32 ulp for magnitudes in 1..2, which covers the f64 to f32 rounding of the stored value.
const BIAS_BOUND: f64 = 1.0 / 8_388_608.0;

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

fn expected(scale_micro: u32) -> f64 {
    let render = RenderScale::new(scale_micro)
        .unwrap()
        .extent(OUTPUT_EXTENT)
        .unwrap();
    let ratio = f64::from(render[0].min(render[1])) / f64::from(SIZE);
    ratio.log2() - 1.0
}

#[test]
fn temporal_reconstruction_carries_the_derived_bias_at_each_scale() {
    for scale in SCALES {
        let bias = bias_of(
            Some(scale),
            PostResolve::taa(0.9),
            Some(UpscaleMode::Temporal),
        );
        let wanted = expected(scale);
        assert!(wanted < -1.3, "{scale}: a sharpening bias, got {wanted}");
        assert!(
            (f64::from(bias) - wanted).abs() <= BIAS_BOUND,
            "{scale}: bias {bias} wanted {wanted}"
        );
    }
}

/// The documented table is for large outputs. At 64 pixels the render extent rounds to a whole
/// pixel, so the ratio is off by at most half a pixel in `scale * SIZE` pixels, and the bias moves
/// by at most `-log2(1 - 0.5 / (scale * SIZE))`, the larger of the two directions.
#[test]
fn the_table_values_hold_at_the_documented_scales_within_the_extent_rounding() {
    let table = [(500_000, -2.0), (670_000, -1.578), (770_000, -1.377)];
    for (scale, listed) in table {
        let bias = bias_of(
            Some(scale),
            PostResolve::taa(0.9),
            Some(UpscaleMode::Temporal),
        );
        let pixels = f64::from(scale) / 1e6 * f64::from(SIZE);
        let rounding = -(1.0 - 0.5 / pixels).log2();
        assert!(
            (f64::from(bias) - listed).abs() <= rounding + 2e-3,
            "{scale}: bias {bias} table {listed} rounding bound {rounding}"
        );
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
