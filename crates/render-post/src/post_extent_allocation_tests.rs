//! Allocation accounting of the post chain by carrier extent: reconstruction onto the output grid
//! sizes every post intermediate at the output extent, and every other mode is unchanged.
use crate::{
    Bloom, BloomPolicies, CasSettings, FrameConfig, HdrPostSettings, NativeRcasSettings,
    PostResolve, RcasDomain, UpscaleMode, UpscaleSettings, allocation::post_allocations,
};

const RENDER: [u32; 2] = [32, 16];
const OUTPUT: [u32; 2] = [64, 32];

fn config(reduced: bool) -> FrameConfig {
    let config = FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1);
    if reduced {
        config.with_render_extent(RENDER)
    } else {
        config
    }
}

fn bloom() -> Bloom {
    Bloom {
        policies: BloomPolicies::default(),
        threshold: 1.0,
        knee: 0.5,
        intensity: 0.25,
        levels: 3,
    }
}

fn upscale(mode: UpscaleMode, sharpness: Option<f32>) -> UpscaleSettings {
    UpscaleSettings { mode, sharpness }
}

/// Every configuration that does not use `UpscaleMode::Temporal` at a reduced extent.
fn existing_modes() -> Vec<(FrameConfig, HdrPostSettings)> {
    let resolves = [
        PostResolve::None,
        PostResolve::fxaa(0.5, crate::FxaaQuality::default()),
        PostResolve::taa(0.9),
    ];
    let upscales = [
        None,
        Some(upscale(UpscaleMode::Bilinear, None)),
        Some(upscale(UpscaleMode::Easu, Some(1.0))),
        Some(upscale(UpscaleMode::Temporal, Some(0.5))),
    ];
    let mut all = Vec::new();
    for resolve in resolves {
        for upscale in upscales {
            for (bloom, cas, rcas) in [
                (false, false, false),
                (true, true, false),
                (true, false, true),
            ] {
                for reduced in [false, true] {
                    let temporal_reduced =
                        reduced && upscale.is_some_and(|u| u.mode == UpscaleMode::Temporal);
                    if (reduced && upscale.is_none()) || temporal_reduced {
                        continue;
                    }
                    all.push((
                        config(reduced),
                        HdrPostSettings {
                            resolve,
                            upscale,
                            bloom: bloom.then(self::bloom),
                            sharpen: cas.then_some(CasSettings { sharpness: 0.5 }),
                            native_rcas: rcas.then_some(NativeRcasSettings {
                                stops: 1.0,
                                domain: RcasDomain::DisplayLinear,
                            }),
                            ..HdrPostSettings::default()
                        },
                    ));
                }
            }
        }
    }
    all
}

#[test]
fn every_existing_mode_allocates_for_its_bloom_and_taa_stages() {
    let all = existing_modes();
    assert_eq!(all.len(), 54);
    for (index, (config, settings)) in all.into_iter().enumerate() {
        let bytes = whole(config, &settings);
        if settings.bloom.is_some() {
            assert!(
                bytes > without_bloom(config, &settings),
                "configuration {index} allocates nothing for its bloom chain"
            );
        }
        if settings.resolve.active_taa().is_some() {
            assert!(
                bytes > without_taa(config, &settings),
                "configuration {index} allocates nothing for its TAA bank"
            );
        }
    }
}

/// Bytes of everything except the bloom chain.
fn without_bloom(config: FrameConfig, settings: &HdrPostSettings) -> u64 {
    whole(
        config,
        &HdrPostSettings {
            bloom: None,
            ..*settings
        },
    )
}

/// Bytes of everything except the TAA bank.
fn without_taa(config: FrameConfig, settings: &HdrPostSettings) -> u64 {
    whole(
        config,
        &HdrPostSettings {
            resolve: PostResolve::None,
            ..*settings
        },
    )
}

fn temporal(sharpness: Option<f32>) -> HdrPostSettings {
    HdrPostSettings {
        resolve: PostResolve::taa(0.9),
        upscale: Some(upscale(UpscaleMode::Temporal, sharpness)),
        bloom: Some(bloom()),
        sharpen: Some(CasSettings { sharpness: 0.5 }),
        ..HdrPostSettings::default()
    }
}

fn whole(config: FrameConfig, settings: &HdrPostSettings) -> u64 {
    post_allocations(config, *settings)
        .validate(&wgpu::Limits::default())
        .unwrap()
}

#[test]
fn temporal_post_intermediates_are_counted_at_the_output_extent() {
    // At equal extents Temporal is the native chain on that grid, so the post intermediates
    // (bloom, CAS) of the reduced configuration must cost exactly what they cost on the grid O.
    let on_output = FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1);
    let on_render = FrameConfig::new(RENDER, wgpu::TextureFormat::Rgba16Float, 1);
    let settings = temporal(None);
    assert_eq!(
        without_taa(config(true), &settings),
        without_taa(on_output, &settings)
    );
    assert_ne!(
        without_taa(config(true), &settings),
        without_taa(on_render, &settings)
    );
}

#[test]
fn temporal_rcas_is_a_native_rcas_input_at_the_output_extent() {
    let on_output = FrameConfig::new(OUTPUT, wgpu::TextureFormat::Rgba16Float, 1);
    let native = HdrPostSettings {
        upscale: None,
        native_rcas: Some(NativeRcasSettings {
            stops: 0.5,
            domain: RcasDomain::DisplayLinear,
        }),
        ..temporal(None)
    };
    assert_eq!(
        without_taa(config(true), &temporal(Some(0.5))),
        without_taa(on_output, &native)
    );
}
