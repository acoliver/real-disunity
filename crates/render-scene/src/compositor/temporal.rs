//! Projection jitter and previous-frame view state of the stock compositor.
//!
//! History follows the ownership of the exposure and shadow-fit history: it belongs to the
//! [`FrameRenderer`], advances only when a frame's transaction is submitted, and is never
//! published from a cancelled or failed preparation. A reset bumps an epoch so a frame that was
//! prepared before the reset cannot republish a stale matrix when it is submitted afterwards.

use super::FrameRenderer;
mod block;
mod taa;
use crate::frame::{FrameUniform, temporal::TemporalView};
use glam::{Mat4, Vec3};
use worldrunner_render_post::{FrameError, PostError, ProjectionJitter};

/// Unjittered camera of the last published frame.
#[derive(Clone, Copy, Debug)]
struct Published {
    view_projection: Mat4,
    jitter_pixels: [f32; 2],
    /// Render and output extents, see [`block::extents_key`].
    extents: [[u32; 2]; 2],
    animation_time_ms: u32,
    camera_position: Vec3,
}

/// Frame counter and previous-frame camera, shared with the publication callbacks.
#[derive(Debug, Default)]
pub(crate) struct TemporalHistory {
    jitter: Option<ProjectionJitter>,
    frame_index: u64,
    epoch: u64,
    previous: Option<Published>,
    /// TAA block of the last encoded frame, uploaded by the next output. Never published.
    pending_taa: Option<[u8; worldrunner_render_post::TAA_UNIFORM_BYTES]>,
    /// Temporal block of the last encoded frame under reduced temporal reconstruction.
    pending_temporal: Option<worldrunner_render_post::TemporalBlock>,
    /// Camera term of the last encoded frame, the same reprojection as `pending_taa`.
    pending_camera: Option<worldrunner_render_post::TemporalCamera>,
    /// Test harnesses compare a configuration against the same one with the bias forced.
    #[cfg(test)]
    mip_bias_override: Option<f32>,
}

impl TemporalHistory {
    fn reset(&mut self) {
        self.previous = None;
        self.pending_temporal = None;
        self.pending_camera = None;
        self.frame_index = 0;
        self.epoch += 1;
    }

    /// Whether a frame at `time_ms` precedes the last published frame. Equal times (a paused
    /// clock) keep the history; a wrapped 32-bit clock counts as moving backwards.
    fn rewound_before(&self, time_ms: u32) -> bool {
        self.previous
            .is_some_and(|previous| time_ms < previous.animation_time_ms)
    }
}

impl FrameRenderer {
    /// Selects sub-pixel projection jitter for the scene passes, or turns it off with `None`.
    ///
    /// Off is the default, and then every output is byte-identical to a renderer that never
    /// had the setting. A change discards the previous-frame camera and restarts the sequence.
    /// Jitter alone makes the image shimmer; it is meant for the temporal resolve that
    /// accumulates the frames.
    ///
    /// # Errors
    ///
    /// [`PostError::UnsupportedProfile`] when `jitter` is `None` while a TAA resolve is
    /// resident: TAA cannot accumulate unjittered frames. The same error for a sequence shorter
    /// than reduced temporal reconstruction derives from both extents
    /// ([`HdrPostSettings::validate_jitter`](worldrunner_render_post::HdrPostSettings::validate_jitter)).
    /// The selection is unchanged.
    /// This returns a `Result` since the milestone 1 signature, which returned nothing.
    pub fn set_projection_jitter(
        &mut self,
        jitter: Option<ProjectionJitter>,
    ) -> Result<(), PostError> {
        if jitter.is_none() && self.post_requested.resolve.active_taa().is_some() {
            return Err(PostError::UnsupportedProfile(
                "TAA requires projection jitter",
            ));
        }
        self.post_requested
            .validate_jitter(self.targets().config(), jitter)?;
        let mut history = self.temporal.borrow_mut();
        if history.jitter != jitter {
            history.jitter = jitter;
            history.reset();
        }
        Ok(())
    }

    /// The selected projection jitter, if any. Multisampled targets apply no offset even when a
    /// sequence is selected ([`ProjectionJitter::offset_pixels_for`]).
    pub fn projection_jitter(&self) -> Option<ProjectionJitter> {
        self.temporal.borrow().jitter
    }

    #[cfg(test)]
    pub(crate) fn force_material_mip_bias(&self, bias: Option<f32>) {
        self.temporal.borrow_mut().mip_bias_override = bias;
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
        let extents = block::extents_key(self.targets().config());
        self.temporal
            .borrow()
            .previous
            .is_some_and(|previous| previous.extents == extents)
    }

    /// Applies the jitter of the next frame to the camera matrix and attaches the history.
    ///
    /// The jitter always starts from the unjittered matrix, so preparing a frame twice does not
    /// offset it twice. Without jitter the camera is unchanged. The material mip bias follows the
    /// configuration, not the jitter, so it is set on every frame.
    pub(crate) fn jitter_frame(&self, mut frame: FrameUniform) -> FrameUniform {
        frame.material_mip_bias = self
            .post_requested
            .material_mip_bias(self.targets().config());
        #[cfg(test)]
        if let Some(forced) = self.temporal.borrow().mip_bias_override {
            frame.material_mip_bias = forced;
        }
        if self.temporal.borrow().jitter.is_none() {
            return frame;
        }
        let rewound = self
            .temporal
            .borrow()
            .rewound_before(frame.animation_time_milliseconds);
        if rewound {
            self.temporal.borrow_mut().reset();
        }
        let history = self.temporal.borrow();
        let Some(jitter) = history.jitter else {
            return frame;
        };
        let config = self.targets().config();
        let (size, samples) = (config.size, config.samples);
        let extents = block::extents_key(config);
        let unjittered = frame.selection_view_projection();
        let jitter_pixels = jitter.offset_pixels_for(history.frame_index, samples);
        let [x, y] = jitter.clip_offset_for(history.frame_index, size, samples);
        let previous = history
            .previous
            .filter(|previous| previous.extents == extents);
        frame.view_projection = Mat4::from_translation(Vec3::new(x, y, 0.0)) * unjittered;
        frame.temporal = Some(TemporalView {
            unjittered_view_projection: unjittered,
            previous_view_projection: previous.map_or(unjittered, |p| p.view_projection),
            jitter_pixels,
            previous_jitter_pixels: previous.map_or(jitter_pixels, |p| p.jitter_pixels),
            history_valid: previous.is_some(),
            previous_animation_time_ms: previous
                .map_or(frame.animation_time_milliseconds, |p| p.animation_time_ms),
            previous_camera_position: previous.map_or(frame.camera_position, |p| p.camera_position),
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
        self.remember_taa_block(Some(frame));
        let Some(temporal) = frame.temporal else {
            return Ok(());
        };
        let published = Published {
            view_projection: temporal.unjittered_view_projection,
            jitter_pixels: temporal.jitter_pixels,
            extents: block::extents_key(self.targets().config()),
            animation_time_ms: frame.animation_time_milliseconds,
            camera_position: frame.camera_position,
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

#[cfg(test)]
#[path = "taa_end_to_end_tests.rs"]
mod taa_end_to_end_tests;

#[cfg(test)]
#[path = "temporal_upscale_end_to_end_tests.rs"]
mod temporal_upscale_end_to_end_tests;

#[cfg(test)]
#[path = "temporal_mip_bias_tests.rs"]
mod temporal_mip_bias_tests;

#[cfg(test)]
#[path = "temporal_configuration_tests.rs"]
mod temporal_configuration_tests;
