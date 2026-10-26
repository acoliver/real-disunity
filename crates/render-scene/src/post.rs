//! Translation of authored post controls into the HDR executor's unitful settings.
use worldrunner_render_plan::{SceneAntiAlias, SceneEffects, ScenePostStage};
use worldrunner_render_post::{AdaptiveExposure, Bloom, HdrPostSettings, PostError};

pub fn settings(effects: &SceneEffects) -> Result<HdrPostSettings, PostError> {
    let mut out = HdrPostSettings::default();
    if let Some(post) = effects.hdr_post() {
        post.validate()
            .map_err(|_| PostError::UnsupportedProfile("HDR post quantities"))?;
        apply_grading(post, &mut out);
        apply_fxaa(post, &mut out);
        out.adaptive = post.adaptive_exposure.map(adaptive);
        if let Some(b) = post.bloom {
            let pyramid = crate::post_bloom::pyramid(b);
            out.bloom = Some(pyramid.bloom);
            if b.recipe.is_some() {
                out.bloom_kernel =
                    worldrunner_render_post::BloomKernel::SeparablePyramid(pyramid.levels);
            }
        }
        out.fxaa = post.fxaa_subpixel_micro.map(micro);
    }
    for stage in effects.post() {
        match *stage {
            ScenePostStage::Bloom {
                threshold_micro,
                intensity_micro,
            } if out.bloom.is_none() => {
                out.bloom = Some(Bloom {
                    policies: worldrunner_render_post::BloomPolicies::default(),
                    threshold: micro(threshold_micro),
                    intensity: micro(intensity_micro),
                    knee: 0.5,
                    levels: 5,
                });
            }
            _ => {
                return Err(PostError::UnsupportedProfile(
                    "post order or duplicate bloom",
                ));
            }
        }
    }
    match *effects.anti_alias() {
        SceneAntiAlias::Disabled => {}
        SceneAntiAlias::Fxaa { subpixel_micro } if out.fxaa.is_none() => {
            out.fxaa = Some(micro(subpixel_micro));
        }
        _ => return Err(PostError::UnsupportedProfile("TAA or duplicate FXAA")),
    }
    out.validate()?;
    // Zero-strength requests take the unchanged fixed-output path.
    out.bloom = out.bloom.filter(|b| b.intensity > 0.0);
    out.fxaa = out.fxaa.filter(|weight| *weight > 0.0);
    Ok(out)
}
pub(crate) fn requested(effects: &SceneEffects) -> bool {
    effects.hdr_post().is_some()
        || !effects.post().is_empty()
        || *effects.anti_alias() != SceneAntiAlias::Disabled
}
pub(crate) fn micro(value: u32) -> f32 {
    milli(value) / 1000.0
}
fn milli(value: u32) -> f32 {
    f32::from(u16::try_from(value / 1000).expect("validated post quantity"))
        + f32::from(u16::try_from(value % 1000).unwrap()) / 1000.0
}
fn signed(value: i32) -> f32 {
    f32::from(i16::try_from(value).expect("validated log range")) / 1000.0
}

fn apply_grading(post: &worldrunner_render_plan::SceneHdrPost, out: &mut HdrPostSettings) {
    out.tone_map = post.tone_map.map(|t| match t {
        worldrunner_render_plan::SceneToneMap::Reinhard => {
            worldrunner_render_post::ToneMap::Reinhard
        }
        worldrunner_render_plan::SceneToneMap::Filmic => worldrunner_render_post::ToneMap::Filmic,
        worldrunner_render_plan::SceneToneMap::AcesFitted => {
            worldrunner_render_post::ToneMap::AcesFitted
        }
    });
    if let Some(g) = post.grading {
        out.grading = worldrunner_render_post::ColorGrading {
            linear_scale: micro(g.linear_scale_micro),
            display_power: g.display_power_micro.map(micro),
            vignette: g.vignette.map(|v| worldrunner_render_post::Vignette {
                axis_scale: v.axis_scale_micro.map(micro),
                tint_range: v.tint_range_micro.map(micro),
                tint: v.tint_micro.map(micro),
                darken_range: v.darken_range_micro.map(micro),
                darken_opacity: micro(v.darken_opacity_micro),
            }),
        };
    }
}

fn apply_fxaa(post: &worldrunner_render_plan::SceneHdrPost, out: &mut HdrPostSettings) {
    use worldrunner_render_plan::{SceneFxaaKernel, SceneFxaaLuminance};
    use worldrunner_render_post::{FxaaKernel, FxaaLuminance, FxaaQuality};
    if let Some(q) = post.fxaa_quality {
        out.fxaa_quality = FxaaQuality {
            edge_threshold: micro(q.edge_threshold_micro),
            minimum_threshold: micro(q.minimum_threshold_micro),
            subpixel_blend: micro(q.subpixel_blend_micro),
            luminance: match q.luminance {
                SceneFxaaLuminance::Linear => FxaaLuminance::Linear,
                SceneFxaaLuminance::Srgb => FxaaLuminance::Srgb,
                SceneFxaaLuminance::Green => FxaaLuminance::Green,
            },
            kernel: match q.kernel {
                SceneFxaaKernel::Sobel => FxaaKernel::Sobel,
                SceneFxaaKernel::SecondDifference => FxaaKernel::SecondDifference,
            },
        };
    }
}

fn metering(
    p: Option<worldrunner_render_plan::ScenePercentileExposure>,
) -> worldrunner_render_post::ExposureMetering {
    use worldrunner_render_plan::SceneExposureAdaptation as S;
    use worldrunner_render_post::{ExposureAdaptation as E, ExposureMetering, PercentileExposure};
    p.map_or(ExposureMetering::TrimmedMean, |p| {
        ExposureMetering::InterpolatedPercentile(PercentileExposure {
            black_threshold: micro(p.black_threshold_micro),
            percentile: micro(p.percentile_micro),
            key_bias: micro(p.key_bias_micro),
            output_scale: micro(p.output_scale_micro),
            adaptation: match p.adaptation {
                S::Exposure => E::Exposure,
                S::Luminance => E::Luminance,
                S::Instantaneous => E::Instantaneous,
            },
        })
    })
}

fn adaptive(a: worldrunner_render_plan::SceneAdaptiveExposure) -> AdaptiveExposure {
    AdaptiveExposure {
        metering: metering(a.metering),
        log_range: [signed(a.log_min_milli), signed(a.log_max_milli)],
        percentiles: [
            micro(a.low_percentile_micro),
            micro(a.high_percentile_micro),
        ],
        key: micro(a.key_micro),
        exposure_range: [micro(a.min_exposure_micro), micro(a.max_exposure_micro)],
        speeds: [milli(a.speed_up_milli), milli(a.speed_down_milli)],
        fixed_delta: a.fixed_delta_microseconds.map(micro),
        fixed_exposure: a.fixed_exposure_micro.map(micro),
    }
}

#[cfg(test)]
mod exposure_tests {
    #[test]
    fn authored_metering_preserves_units_and_adaptation() {
        use worldrunner_render_plan::{SceneExposureAdaptation, ScenePercentileExposure};
        use worldrunner_render_post::{ExposureAdaptation, ExposureMetering};
        let p = ScenePercentileExposure {
            black_threshold_micro: 5000,
            percentile_micro: 575_000,
            key_bias_micro: 575_608,
            output_scale_micro: 4_000_000,
            adaptation: SceneExposureAdaptation::Luminance,
        };
        p.validate().unwrap();
        let ExposureMetering::InterpolatedPercentile(actual) = super::metering(Some(p)) else {
            unreachable!()
        };
        assert!((actual.black_threshold - 0.005).abs() < 1e-8);
        assert!((actual.percentile - 0.575).abs() < 1e-7);
        assert!((actual.key_bias - 0.575_608).abs() < 1e-7);
        assert!((actual.output_scale - 4.0).abs() < 1e-7);
        assert_eq!(actual.adaptation, ExposureAdaptation::Luminance);
        assert_eq!(super::metering(None), ExposureMetering::TrimmedMean);
    }
}
