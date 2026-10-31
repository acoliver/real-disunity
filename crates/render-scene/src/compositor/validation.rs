use super::FrameRenderer;
use worldrunner_render_post::{FrameConfig, FrameError, PostError};
impl FrameRenderer {
    pub(crate) fn validate_ssao(
        &self,
        pipelines: &crate::renderer::ScenePipelines,
        requested: Option<&worldrunner_render_plan::SceneSsao>,
    ) -> Result<(), FrameError> {
        if pipelines.ssao != self.ssao_requested.is_some() {
            return Err(FrameError::UnsupportedEffect(
                "SSAO pipeline/profile mismatch",
            ));
        }
        if self.ssao_requested.as_ref() != requested {
            return Err(FrameError::SsaoSettings(
                "SSAO resident/profile settings mismatch",
            ));
        }
        Ok(())
    }

    pub(crate) fn validate_post(
        &self,
        mut requested: worldrunner_render_post::HdrPostSettings,
    ) -> Result<(), FrameError> {
        // Reconstruction and native RCAS are owned by the frame output configuration.
        requested.upscale = self.post_requested.upscale;
        requested.native_rcas = self.post_requested.native_rcas;
        if self.post_requested != requested {
            return Err(FrameError::UnsupportedEffect(
                "post resident/profile settings mismatch",
            ));
        }
        Ok(())
    }
    pub(crate) fn validate_ssr(
        &self,
        requested: Option<&worldrunner_render_plan::SceneSsr>,
    ) -> Result<(), FrameError> {
        if self.ssr_requested.as_ref() != requested {
            return Err(FrameError::SsrSettings(
                "resident/profile settings mismatch",
            ));
        }
        Ok(())
    }
    pub(crate) fn validate_water(
        &self,
        requested: &[worldrunner_render_plan::SceneWaterBody],
    ) -> Result<(), FrameError> {
        if self
            .water
            .as_ref()
            .map_or(&[][..], |water| water.settings.as_ref())
            != requested
        {
            return Err(FrameError::WaterSettings(
                "resident/profile settings mismatch; water requires HDR",
            ));
        }
        Ok(())
    }
}

pub(super) fn validate_clear(color: wgpu::Color) -> Result<(), PostError> {
    if [color.r, color.g, color.b, color.a]
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(PostError::UnsupportedProfile(
            "clear color must be finite linear RGBA in 0..=1",
        ));
    }
    Ok(())
}

pub(super) fn validate_reflection_extent(
    device: &wgpu::Device,
    config: FrameConfig,
    settings: Option<&worldrunner_render_plan::SceneSsr>,
) -> Result<(), FrameError> {
    if let Some(settings) = settings.filter(|settings| settings.enabled) {
        crate::ssr::validate_extent(config, settings, &device.limits())?;
    }
    Ok(())
}

pub(super) fn validate_hdr_effects(
    effects: Option<&worldrunner_render_plan::SceneEffects>,
) -> Result<(), PostError> {
    if let Some(effects) = effects {
        if let Some(settings) = effects.ssao() {
            crate::ssao::validate_settings(settings)?;
        }
        crate::frame::validate_stock_effects(effects)
            .map_err(|_| PostError::UnsupportedProfile("scene effects"))?;
    }
    if let Some(settings) = effects.and_then(|effects| effects.ssr()) {
        crate::ssr::validate_settings(settings)?;
    }
    Ok(())
}
