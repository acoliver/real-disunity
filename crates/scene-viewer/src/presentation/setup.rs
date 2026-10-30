use super::{COLOR, FrameConfig, RendererConfiguration, ViewerError, Viewport};

pub(super) fn configuration(
    features: wgpu::Features,
    format: wgpu::TextureFormat,
    size: Viewport,
    profile: crate::FrameProfile,
    effects: Option<&worldrunner_render_plan::SceneEffects>,
) -> Result<
    (
        RendererConfiguration,
        Option<worldrunner_render_post::RenderScale>,
    ),
    ViewerError,
> {
    let mut settings = RendererConfiguration::legacy([size.width, size.height], format);
    settings.frame.format = COLOR;
    if features.contains(crate::renderer::timing::features()) {
        settings.timing = worldrunner_render_post::TimingMode::Required;
    }
    configure_hdr(&mut settings, size, profile, effects)?;

    let scale = if let crate::FrameProfile::HdrUpscaled { scale, upscale, .. } = profile {
        settings.post.upscale = Some(upscale);
        settings.frame = settings
            .frame
            .with_render_extent(scale.extent([size.width, size.height])?);
        Some(scale)
    } else {
        None
    };

    if let crate::FrameProfile::HdrNativeRcas { rcas, .. } = profile {
        rcas.validate()?;
        settings.post.native_rcas = Some(rcas);
    }
    Ok((settings, scale))
}

fn configure_hdr(
    settings: &mut RendererConfiguration,
    size: Viewport,
    profile: crate::FrameProfile,
    effects: Option<&worldrunner_render_plan::SceneEffects>,
) -> Result<(), ViewerError> {
    let hdr = match profile {
        crate::FrameProfile::Legacy => None,
        crate::FrameProfile::Hdr { samples, tone_map }
        | crate::FrameProfile::HdrNativeRcas {
            samples, tone_map, ..
        }
        | crate::FrameProfile::HdrUpscaled {
            samples, tone_map, ..
        } => Some((samples, tone_map)),
    };
    if let Some((samples, tone_map)) = hdr {
        settings.frame = FrameConfig::new(
            [size.width, size.height],
            wgpu::TextureFormat::Rgba16Float,
            samples,
        );
        let exposure = effects.map_or(1.0, |effects| {
            f64::from(effects.grading().exposure_micro) / 1_000_000.0
        });
        settings.hdr = Some((exposure, tone_map));
        settings.post = effects
            .map(worldrunner_render_scene::post_settings)
            .transpose()?
            .unwrap_or_default();
        settings.ssao = effects.and_then(|effects| effects.ssao()).copied();
        settings.ssr = effects.and_then(|effects| effects.ssr()).copied();
        settings.water = effects.map_or_else(Vec::new, |effects| effects.water_bodies().to_vec());
    }
    Ok(())
}
