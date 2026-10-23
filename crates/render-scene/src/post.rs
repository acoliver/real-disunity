//! Translation of authored post controls into the HDR executor's unitful settings.
use worldrunner_render_plan::{SceneAntiAlias, SceneEffects, ScenePostStage};
use worldrunner_render_post::{AdaptiveExposure, Bloom, HdrPostSettings, PostError};

pub(crate) fn settings(effects: &SceneEffects) -> Result<HdrPostSettings, PostError> {
    let mut out = HdrPostSettings::default();
    if let Some(post) = effects.hdr_post() {
        post.validate()
            .map_err(|_| PostError::UnsupportedProfile("HDR post quantities"))?;
        out.adaptive = post.adaptive_exposure.map(|a| AdaptiveExposure {
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
        });
        out.bloom = post.bloom.map(|b| Bloom {
            threshold: micro(b.threshold_micro),
            knee: micro(b.soft_knee_micro),
            intensity: micro(b.intensity_micro),
            levels: b.levels,
        });
        out.fxaa = post.fxaa_subpixel_micro.map(micro);
    }
    for stage in effects.post() {
        match *stage {
            ScenePostStage::Bloom {
                threshold_micro,
                intensity_micro,
            } if out.bloom.is_none() => {
                out.bloom = Some(Bloom {
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
fn micro(value: u32) -> f32 {
    milli(value) / 1000.0
}
fn milli(value: u32) -> f32 {
    f32::from(u16::try_from(value / 1000).expect("validated post quantity"))
        + f32::from(u16::try_from(value % 1000).unwrap()) / 1000.0
}
fn signed(value: i32) -> f32 {
    f32::from(i16::try_from(value).expect("validated log range")) / 1000.0
}
