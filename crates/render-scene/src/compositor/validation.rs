use super::FrameRenderer;
use worldrunner_render_post::FrameError;
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
