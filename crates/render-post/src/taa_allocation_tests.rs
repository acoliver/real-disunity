//! CPU-only byte accounting of the TAA history banks.
use crate::{
    ColorGrading, FrameConfig, HdrPostSettings, PostResolve, UpscaleMode, UpscaleSettings,
    allocation::post_allocations,
};

const RENDER: [u32; 2] = [32, 16];
const OUTPUT: [u32; 2] = [64, 32];
const HALF_RGBA: u64 = 8;
const FLOAT_R: u64 = 4;
const SLOTS: u64 = 3;

fn texels(extent: [u32; 2]) -> u64 {
    u64::from(extent[0]) * u64::from(extent[1])
}

fn settings(resolve: PostResolve, upscale: Option<UpscaleMode>) -> HdrPostSettings {
    HdrPostSettings {
        grading: ColorGrading {
            linear_scale: 1.5,
            ..ColorGrading::default()
        },
        resolve,
        upscale: upscale.map(|mode| UpscaleSettings {
            mode,
            sharpness: None,
        }),
        ..HdrPostSettings::default()
    }
}

fn config(samples: u32) -> FrameConfig {
    FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, samples).with_render_extent(RENDER)
}

/// Bytes the TAA resolve adds to a configuration, found by removing only the resolve.
fn taa_bytes(config: FrameConfig, upscale: Option<UpscaleMode>) -> u64 {
    let limits = wgpu::Limits::default();
    let bytes = |resolve| {
        post_allocations(config, settings(resolve, upscale))
            .validate(&limits)
            .unwrap()
    };
    bytes(PostResolve::taa(0.9)) - bytes(PostResolve::None)
}

#[test]
fn native_taa_counts_shared_history_once_and_slot_outputs_three_times() {
    let shared = texels(RENDER) * (HALF_RGBA + FLOAT_R);
    let per_slot = texels(RENDER) * (HALF_RGBA + FLOAT_R) * SLOTS;
    assert_eq!(taa_bytes(config(1), None), shared + per_slot);
}

#[test]
fn native_taa_history_does_not_grow_with_the_output_extent() {
    let same = FrameConfig::new(RENDER, wgpu::TextureFormat::Rgba16Float, 1);
    assert_eq!(taa_bytes(config(1), None), taa_bytes(same, None));
}

#[test]
fn native_taa_history_is_single_sampled_with_msaa_scene_targets() {
    assert_eq!(taa_bytes(config(4), None), taa_bytes(config(1), None));
}

#[test]
fn temporal_taa_counts_output_sized_history_once_without_depth() {
    let shared = texels(OUTPUT) * HALF_RGBA;
    let per_slot = texels(OUTPUT) * HALF_RGBA * SLOTS;
    assert_eq!(
        taa_bytes(config(1), Some(UpscaleMode::Temporal)),
        shared + per_slot
    );
}
