use super::{FrameOutput, FrameRenderer};
use worldrunner_render_post::{FrameError, PostError};

impl FrameRenderer {
    /// A fixed-exposure profile is immutable. Rebuild it when scene exposure changes.
    /// Geometry stays unexposed only when this executor owns the post multiplier.
    pub fn prepare_uniform(
        &self,
        mut frame: crate::frame::FrameUniform,
    ) -> Result<crate::frame::FrameUniform, FrameError> {
        if let Some(time) = self.animation_time {
            frame.animation_time_milliseconds = time;
        }
        if let FrameOutput::Hdr(factory) = self.chain.factory() {
            if !factory.matches_exposure(frame.exposure) {
                return Err(FrameError::FixedExposureMismatch);
            }
            frame.exposure = 1.0;
        }
        Ok(self.jitter_frame(frame))
    }

    /// Supplies animation time for the next prepared frame, in seconds.
    /// Adaptation uses time since the last encoded output, capped at ten seconds.
    pub fn set_frame_time(&mut self, seconds: f32) -> Result<(), FrameError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(FrameError::WaterSettings(
                "frame time must be finite and nonnegative",
            ));
        }
        self.selected_time = Some(
            std::time::Duration::try_from_secs_f32(seconds)
                .map_err(|_| FrameError::WaterSettings("frame time exceeds duration range"))?,
        );
        self.frame_delta = (seconds - self.post_time.get()).clamp(0.0, 10.0);
        self.elapsed_seconds = seconds;
        let period = f64::from(u32::MAX) + 1.0;
        let time =
            std::time::Duration::from_secs_f64(((f64::from(seconds) * 1000.0) % period) / 1000.0);
        self.animation_time = Some(
            u32::try_from(time.as_millis() % (u128::from(u32::MAX) + 1))
                .expect("wrapped animation milliseconds"),
        );
        Ok(())
    }
    /// Supplies the admitted clock sample without rounding timed shadow adoption.
    /// Moving time backwards resets depth-fitting evidence and contraction history.
    pub fn set_frame_time_duration(&mut self, time: std::time::Duration) -> Result<(), FrameError> {
        self.set_frame_time(time.as_secs_f32())?;
        self.selected_time = Some(time);
        Ok(())
    }
    /// Explicit simulation delta in seconds. Fixed capture steps in the scene override it.
    pub fn set_frame_delta(&mut self, delta: f32) -> Result<(), PostError> {
        if !delta.is_finite() || !(0.0..=10.0).contains(&delta) {
            return Err(PostError::FrameDelta);
        }
        self.frame_delta = delta;
        Ok(())
    }
    /// Reset the ordered history at the next output, for camera cuts or capture restart.
    pub fn reset_exposure(&self) {
        self.exposure_reset.set(true);
    }
}
