//! Projection jitter and previous-frame view state of the stock compositor.
//!
//! History follows the ownership of the exposure and shadow-fit history: it belongs to the
//! [`FrameRenderer`], advances only when a frame's transaction is submitted, and is never
//! published from a cancelled or failed preparation. A reset bumps an epoch so a frame that was
//! prepared before the reset cannot republish a stale matrix when it is submitted afterwards.

use super::FrameRenderer;
use crate::frame::{FrameUniform, temporal::TemporalView};
use glam::{Mat4, Vec3};
use worldrunner_render_post::{FrameError, ProjectionJitter};

/// Unjittered camera of the last published frame.
#[derive(Clone, Copy, Debug)]
struct Published {
    view_projection: Mat4,
    jitter_pixels: [f32; 2],
    size: [u32; 2],
}

/// Frame counter and previous-frame camera, shared with the publication callbacks.
#[derive(Debug, Default)]
pub(crate) struct TemporalHistory {
    jitter: Option<ProjectionJitter>,
    frame_index: u64,
    epoch: u64,
    previous: Option<Published>,
}

impl TemporalHistory {
    fn reset(&mut self) {
        self.previous = None;
        self.frame_index = 0;
        self.epoch += 1;
    }
}

impl FrameRenderer {
    /// Selects sub-pixel projection jitter for the scene passes, or turns it off with `None`.
    ///
    /// Off is the default, and then every output is byte-identical to a renderer that never
    /// had the setting. A change discards the previous-frame camera and restarts the sequence.
    /// Jitter alone makes the image shimmer; it is meant for the temporal resolve that
    /// accumulates the frames, which a later release adds.
    pub fn set_projection_jitter(&mut self, jitter: Option<ProjectionJitter>) {
        let mut history = self.temporal.borrow_mut();
        if history.jitter != jitter {
            history.jitter = jitter;
            history.reset();
        }
    }

    /// The selected projection jitter, if any.
    pub fn projection_jitter(&self) -> Option<ProjectionJitter> {
        self.temporal.borrow().jitter
    }

    /// Discards the previous-frame camera, for camera cuts and capture restarts.
    ///
    /// The next frame has no history and the jitter sequence restarts. Frames already prepared
    /// but not yet submitted do not republish the discarded camera.
    pub fn reset_temporal_history(&self) {
        self.temporal.borrow_mut().reset();
    }

    /// Whether the next prepared frame has a previous camera to reproject from.
    pub fn has_temporal_history(&self) -> bool {
        let size = self.targets().config().size;
        self.temporal
            .borrow()
            .previous
            .is_some_and(|previous| previous.size == size)
    }

    /// Applies the jitter of the next frame to the camera matrix and attaches the history.
    ///
    /// The jitter always starts from the unjittered matrix, so preparing a frame twice does not
    /// offset it twice. Without jitter the frame is returned unchanged.
    pub(crate) fn jitter_frame(&self, mut frame: FrameUniform) -> FrameUniform {
        let history = self.temporal.borrow();
        let Some(jitter) = history.jitter else {
            return frame;
        };
        let size = self.targets().config().size;
        let unjittered = frame.selection_view_projection();
        let jitter_pixels = jitter.offset_pixels(history.frame_index);
        let [x, y] = jitter.clip_offset(history.frame_index, size);
        let previous = history.previous.filter(|previous| previous.size == size);
        frame.view_projection = Mat4::from_translation(Vec3::new(x, y, 0.0)) * unjittered;
        frame.temporal = Some(TemporalView {
            unjittered_view_projection: unjittered,
            previous_view_projection: previous.map_or(unjittered, |p| p.view_projection),
            jitter_pixels,
            previous_jitter_pixels: previous.map_or(jitter_pixels, |p| p.jitter_pixels),
            history_valid: previous.is_some(),
        });
        frame
    }

    /// Stages the publication of this frame's camera for submission of the transaction.
    ///
    /// A cancelled or failed transaction drops the staged write, so the previous camera and the
    /// sequence position stay as they were. A frame without temporal data stages nothing.
    pub(crate) fn stage_temporal_history(
        &self,
        encoder: &wgpu::CommandEncoder,
        frame: &FrameUniform,
    ) -> Result<(), FrameError> {
        let Some(temporal) = frame.temporal else {
            return Ok(());
        };
        let published = Published {
            view_projection: temporal.unjittered_view_projection,
            jitter_pixels: temporal.jitter_pixels,
            size: self.targets().config().size,
        };
        let epoch = self.temporal.borrow().epoch;
        let state = self.temporal.clone();
        super::publication::encode(encoder, &self.temporal, move || {
            let mut history = state.borrow_mut();
            if history.epoch == epoch {
                history.previous = Some(published);
                history.frame_index += 1;
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "temporal_tests.rs"]
mod tests;
