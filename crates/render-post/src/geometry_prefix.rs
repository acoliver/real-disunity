use crate::{AttachmentView, FrameError, FramePlan, PassDeclaration, PassEntry};
use worldrunner_render_gpu::submission::Publications;

pub(crate) struct GeometryPrefix {
    frame: Publications,
    passes: Vec<PassDeclaration>,
    views: Vec<AttachmentView>,
}
impl GeometryPrefix {
    pub(crate) fn new(frame: Publications, plan: &FramePlan<'_>) -> Self {
        Self {
            frame,
            passes: plan.passes.to_vec(),
            views: plan.views.to_vec(),
        }
    }

    pub(crate) fn completed_entries(
        self,
        frame: &Publications,
        plan: &FramePlan<'_>,
    ) -> Result<Vec<PassEntry>, FrameError> {
        if !self.frame.same_frame(frame) {
            return Ok(Vec::new());
        }
        let mut entries = Vec::new();
        for produced in self.passes {
            // A depth-only consumer need not include metadata in its suffix plan.
            let Some(pass) = plan
                .passes
                .iter()
                .find(|pass| pass.entry() == produced.entry())
            else {
                continue;
            };
            if pass.color != produced.color
                || pass.additional_colors != produced.additional_colors
                || pass.depth != produced.depth
            {
                return Err(FrameError::AttachmentContract(pass.slot));
            }
            for attachment in pass
                .color
                .iter()
                .chain(&pass.additional_colors)
                .chain(&pass.depth)
            {
                let old = self
                    .views
                    .iter()
                    .find(|view| view.resource() == attachment.resource);
                let new = plan
                    .views
                    .iter()
                    .find(|view| view.resource() == attachment.resource);
                if !matches!((old, new), (Some(old), Some(new)) if old.texture() == new.texture()) {
                    return Err(FrameError::AttachmentView(attachment.resource));
                }
            }
            entries.push(pass.entry());
        }
        Ok(entries)
    }
}

impl<F: super::StageFactory> super::PostChain<F> {
    pub(super) fn completed_geometry_prefix(
        &mut self,
        encoder: &wgpu::CommandEncoder,
        plan: &FramePlan<'_>,
    ) -> Result<Vec<PassEntry>, FrameError> {
        let frame = Publications::for_encoder(encoder)?;
        self.geometry_prefix
            .take()
            .map(|prefix| prefix.completed_entries(&frame, plan))
            .transpose()
            .map(Option::unwrap_or_default)
    }
}
