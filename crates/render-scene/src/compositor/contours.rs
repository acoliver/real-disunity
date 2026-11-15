//! Contour scheduling and capture at the retained stock geometry boundary.
use super::FrameRenderer;
use worldrunner_render_post::{AttachmentView, FeatureDeclaration, FrameError, PassDeclaration};

impl FrameRenderer {
    pub(crate) fn plan_contours(
        &mut self,
        gpu: (&wgpu::Device, &wgpu::CommandEncoder),
        scene: (&crate::draw::SceneGpu, &crate::renderer::ScenePipelines),
        frame: &crate::frame::FrameUniform,
        plan: (
            &mut Vec<PassDeclaration>,
            &mut Vec<AttachmentView>,
            &mut FeatureDeclaration,
        ),
    ) -> Result<Option<crate::draw::ContourDraw>, FrameError> {
        let (scene, pipelines) = scene;
        let count = scene.contour_member_count();
        self.contours.validate(count, self.contour_format, plan.0)?;
        let draw = if count == 0 {
            None
        } else {
            Some(scene.prepare_contours(pipelines, frame)?)
        };
        self.contours
            .schedule(gpu, (self.frame_slot, count), plan)?;
        Ok(draw)
    }

    /// Last submitted visible ID image of the selected frame slot, in `R8Uint` at the
    /// scene depth's sample count. A configured table without member objects returns None.
    pub fn contour_target(&self) -> Option<wgpu::Texture> {
        self.contours.target(self.frame_slot)
    }

    pub fn contour_statistics(&self) -> crate::ContourStatistics {
        self.contour_target()
            .map_or_else(Default::default, |texture| crate::ContourStatistics {
                passes: 1,
                targets: 1,
                samples: texture.sample_count(),
            })
    }

    /// Captures exact IDs, with MSAA samples in horizontal lanes: x = sample * width + x.
    /// This allocates and records a capture-only pass, never resolves or averages IDs.
    /// Call after scene encoding in the same transaction, or after its submission.
    pub fn encode_contour_capture(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<Option<wgpu::Texture>, FrameError> {
        self.contours
            .capture_source(encoder, self.frame_slot)?
            .map(|source| crate::contours::capture::encode(device, encoder, &source))
            .transpose()
    }
}
