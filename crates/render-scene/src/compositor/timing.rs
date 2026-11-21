//! The pass timing of the frames a renderer encodes: configured once, begun before geometry and
//! resolved after output.
use super::FrameRenderer;

impl FrameRenderer {
    /// Timing is explicitly begun before geometry and resolved after output.
    pub fn configure_timing(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        mode: worldrunner_render_post::TimingMode,
    ) -> Result<worldrunner_render_post::TimingSupport, worldrunner_render_post::TimingError> {
        self.chain.configure_timing(device, queue, mode)
    }
    pub fn begin_timing(
        &self,
        frame_id: u64,
    ) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
        self.chain.begin_timing(frame_id)
    }
    pub fn take_timings(
        &self,
    ) -> Result<Vec<worldrunner_render_post::FrameTiming>, worldrunner_render_post::TimingError>
    {
        self.chain.take_timings()
    }
    pub fn unavailable_timing_passes(&self) -> u64 {
        self.chain.unavailable_timing_passes()
    }
    pub fn dropped_timing_frames(&self) -> u64 {
        self.chain.dropped_timing_frames()
    }
    /// Geometry-only custom hosts finish explicitly; output composition does this automatically.
    pub fn finish_timing(
        &self,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), worldrunner_render_gpu::submission::TransactionError> {
        self.chain.finish_timing(encoder)
    }
}
