//! The per-frame block the TAA resolve reads, and the contracts that keep it honest.
//!
//! [`FrameRenderer::encode_frame`] builds the block from the frame's camera data and keeps it
//! until the paired [`FrameRenderer::encode_output`] uploads it for the same frame slot. The
//! block is plain state, written while encoding and never at submission, so it needs no
//! publication key. A host that calls `encode_frame` without `encode_output` uploads nothing,
//! and the next `encode_frame` replaces the block; a frame without temporal data (or the
//! unplanned [`FrameRenderer::encode`]) leaves no block, which uploads as "no history".
use crate::compositor::FrameRenderer;
use crate::frame::{FrameUniform, temporal::TemporalView};
use worldrunner_render_post::{
    AttachmentView, FrameError, ResourceRef, TAA_UNIFORM_BYTES, TaaUniform, ToneMapOutput,
};

/// The TAA block for a frame's camera data.
///
/// The reprojection is `previous * inverse(current)` of the unjittered matrices, computed in
/// f64 because a long far plane makes the f32 inverse inaccurate. A non-finite product (a
/// singular camera) gives an identity matrix with no history.
pub(super) fn uniform(view: &TemporalView) -> TaaUniform {
    let current = view.unjittered_view_projection.as_dmat4();
    let previous = view.previous_view_projection.as_dmat4();
    let reprojection = previous * current.inverse();
    let finite = reprojection.is_finite();
    TaaUniform {
        reprojection: if finite {
            reprojection.to_cols_array_2d()
        } else {
            glam::DMat4::IDENTITY.to_cols_array_2d()
        },
        jitter_pixels: view.jitter_pixels,
        history_valid: view.history_valid && finite,
    }
}

impl FrameRenderer {
    /// Keeps the block of `frame`, or none when it has no temporal data.
    pub(crate) fn remember_taa_block(&self, frame: Option<&FrameUniform>) {
        self.temporal.borrow_mut().pending_taa = frame
            .and_then(|frame| frame.temporal.as_ref())
            .map(|view| uniform(view).to_bytes());
    }

    /// The block the next output uploads.
    pub(crate) fn taa_block(&self) -> Option<[u8; TAA_UNIFORM_BYTES]> {
        self.temporal.borrow().pending_taa
    }

    /// The TAA stage reads [`FrameTargets::depth`](worldrunner_render_post::FrameTargets::depth),
    /// so with TAA resident a scene must be drawn into that texture.
    pub(crate) fn validate_taa_depth(&self, depth: &AttachmentView) -> Result<(), FrameError> {
        if self.taa_resident() && depth.texture() != self.targets().depth().texture() {
            return Err(FrameError::UnsupportedEffect(
                "TAA requires the frame depth target",
            ));
        }
        Ok(())
    }

    /// The velocity attachment the object pass writes, when `pipelines` write one.
    ///
    /// Velocity pipelines need a renderer with TAA resident, which owns the attachment beside
    /// the frame depth. Plain pipelines draw without it and the resolve then reads zeros, the
    /// camera-only reprojection.
    pub(crate) fn velocity_attachment(
        &self,
        pipelines: &crate::renderer::ScenePipelines,
    ) -> Result<Option<AttachmentView>, FrameError> {
        let texture = self
            .taa_resident()
            .then(|| self.targets().velocity_texture())
            .flatten();
        match (texture, pipelines.writes_velocity()) {
            (Some(texture), true) => {
                AttachmentView::new(ResourceRef::new("scene velocity"), texture).map(Some)
            }
            (_, false) => Ok(None),
            (None, true) => Err(FrameError::UnsupportedEffect(
                "velocity pipelines require a renderer with TAA resident",
            )),
        }
    }

    /// Uploads the pending block for this frame slot. Renderers without TAA upload nothing.
    pub(crate) fn prepare_taa(&self, factory: &ToneMapOutput, encoder: &mut wgpu::CommandEncoder) {
        if self.taa_resident() {
            factory.prepare_taa(encoder, self.frame_slot, self.taa_block().as_ref());
        }
    }

    fn taa_resident(&self) -> bool {
        self.post_requested.resolve.active_taa().is_some()
    }
}

#[cfg(test)]
#[path = "taa_tests.rs"]
mod tests;
