//! Nonblocking observation of the latest completed GPU timestamp frame.
pub use worldrunner_render_post::{FrameTiming, PassId, PassSlot, PassTiming, StageId};

/// A timing request never prevents rendering on an unsupported adapter.
#[derive(Clone, Debug, PartialEq)]
pub enum GpuTiming {
    Disabled,
    /// Requested, but no submitted timestamp frame has completed yet.
    Pending,
    Unavailable(GpuTimingUnavailable),
    /// Replaced only by a newer completed sample. Reading does not consume it.
    Complete(FrameTiming),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpuTimingUnavailable {
    MissingFeatures(wgpu::Features),
    /// The backend returned all-zero or reversed counters, with no usable duration.
    InvalidTimestampPair {
        frame_id: u64,
        pass_id: PassId,
    },
}

impl super::RetainedHost {
    /// Durations are nanoseconds, scaled by the queue timestamp period.
    /// Unusable counter pairs produce typed unavailable observations, never fake zeros.
    pub fn gpu_timing(&self) -> &GpuTiming {
        &self.gpu_timing
    }
}
impl super::RetainedEventHost<'_> {
    pub fn gpu_timing(&self) -> &GpuTiming {
        self.host.gpu_timing()
    }
}
