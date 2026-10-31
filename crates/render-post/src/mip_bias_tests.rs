//! The shipped material mip bias is exactly positive zero in every mode. The sampling plumbing
//! stays (the frame lane and every biased site), so a later change can switch a policy on.
use crate::{FrameConfig, HdrPostSettings, PostResolve, RenderScale, UpscaleMode, UpscaleSettings};

const OUTPUTS: [[u32; 2]; 2] = [[1920, 1080], [1452, 921]];
const SCALES: [u32; 3] = [500_000, 670_000, 770_000];

fn scaled(output: [u32; 2], millionths: u32) -> [u32; 2] {
    RenderScale::new(millionths)
        .unwrap()
        .extent(output)
        .unwrap()
}

fn settings(mode: Option<UpscaleMode>, resolve: PostResolve) -> HdrPostSettings {
    HdrPostSettings {
        resolve,
        upscale: mode.map(|mode| UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    }
}

fn config(render: [u32; 2], output: [u32; 2]) -> FrameConfig {
    FrameConfig::new(output, wgpu::TextureFormat::Rgba16Float, 1).with_render_extent(render)
}

#[test]
fn temporal_reconstruction_ships_exactly_zero_bias_at_every_scale() {
    let taa = settings(Some(UpscaleMode::Temporal), PostResolve::taa(0.9));
    for output in OUTPUTS {
        for millionths in SCALES {
            let render = scaled(output, millionths);
            assert!(render != output, "{output:?} at {millionths} is reduced");
            let bias = taa.material_mip_bias(config(render, output));
            assert_eq!(bias.to_bits(), 0, "{output:?} at {millionths}: {bias}");
        }
    }
}

#[test]
fn every_other_mode_has_exactly_zero_bias() {
    let output = [1920, 1080];
    let reduced = config(scaled(output, 500_000), output);
    let native = config(output, output);
    let cases = [
        ("no upscale", settings(None, PostResolve::None), native),
        ("native TAA", settings(None, PostResolve::taa(0.9)), native),
        (
            "temporal at equal extents",
            settings(Some(UpscaleMode::Temporal), PostResolve::taa(0.9)),
            native,
        ),
        (
            "easu",
            settings(Some(UpscaleMode::Easu), PostResolve::taa(0.9)),
            reduced,
        ),
        (
            "bilinear",
            settings(Some(UpscaleMode::Bilinear), PostResolve::None),
            reduced,
        ),
        (
            "easu with taa",
            settings(Some(UpscaleMode::Easu), PostResolve::taa(0.9)),
            reduced,
        ),
    ];
    for (name, settings, config) in cases {
        assert_eq!(settings.material_mip_bias(config).to_bits(), 0, "{name}");
    }
}
