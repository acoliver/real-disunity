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

/// The `(a, b)` of the linear view depth `b / (z - a)` of device depth `z`, read off the
/// unjittered view-projection `m`, or zeros when it is not a perspective projection.
///
/// A perspective projection with standard Z has `z_clip = a * w_clip + b * w_world`, so the
/// z row of `m` equals `a` times the w row plus `b` in the translation lane. `a` is read from
/// a lane where the w row is nonzero. An orthographic projection has w row (0, 0, 0, 1) and
/// returns zeros, which disables the disocclusion test.
fn depth_terms(m: &glam::DMat4) -> [f64; 2] {
    let (z_row, w_row) = (m.row(2), m.row(3));
    let Some(lane) = (0..3).max_by(|&i, &j| w_row[i].abs().total_cmp(&w_row[j].abs())) else {
        return [0.0; 2];
    };
    let scale = w_row.truncate().abs().max_element();
    if scale < f64::EPSILON * w_row[3].abs().max(1.0) {
        return [0.0; 2];
    }
    let a = z_row[lane] / w_row[lane];
    let b = z_row[3] - a * w_row[3];
    let consistent = (0..3).all(|i| (z_row[i] - a * w_row[i]).abs() <= 1e-6 * scale.max(a.abs()));
    if consistent && a.is_finite() && b.is_finite() {
        [a, b]
    } else {
        [0.0; 2]
    }
}

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
        depth_terms: depth_terms(&current),
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

    /// Whether the opaque pass draws with pipelines that write the velocity target: the
    /// reflection forward pipelines when the renderer owns them, otherwise `pipelines`.
    pub(crate) fn draws_velocity(&self, pipelines: &crate::renderer::ScenePipelines) -> bool {
        self.ssao
            .as_ref()
            .and_then(|screen| screen.geometry.forward.as_ref())
            .map_or_else(
                || pipelines.writes_velocity(),
                |(forward, _)| forward.writes_velocity(),
            )
    }

    /// The velocity attachment the object pass writes, when its pipelines (`writes`) write one.
    ///
    /// Velocity pipelines need a renderer with TAA resident, which owns the attachment beside
    /// the frame depth. Plain pipelines draw without it and the resolve then reads zeros, the
    /// camera-only reprojection.
    pub(crate) fn velocity_attachment(
        &self,
        writes: bool,
    ) -> Result<Option<AttachmentView>, FrameError> {
        let texture = self
            .taa_resident()
            .then(|| self.targets().velocity_texture())
            .flatten();
        match (texture, writes) {
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

    pub(crate) fn taa_resident(&self) -> bool {
        self.post_requested.resolve.active_taa().is_some()
    }
}

#[cfg(test)]
#[path = "taa_tests.rs"]
mod tests;
