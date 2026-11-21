//! Where the bytes a flush produced go: the sink the flush's compute buffer writes and pixel
//! answers are applied to, for the running world's flush and for a world held directly.

use worldrunner_disunity_runtime::GpuFlush;
use worldrunner_world_simulation::World;
use worldrunner_world_simulation::graphics::ComputeBufferId;

/// Where the bytes a flush produced go.
pub trait BufferSink {
    /// Writes `bytes` into compute buffer `id` at `offset`.
    ///
    /// # Errors
    /// Fails with the world's reason when the range is not inside the buffer.
    fn write(&mut self, id: ComputeBufferId, offset: u64, bytes: &[u8]) -> Result<(), String>;

    /// Answers pixel request `request` with `rgba`, four floats a texel, bottom row first, as
    /// the render texture stores them.
    ///
    /// # Errors
    /// Fails with the world's reason when the request is unknown or the count is wrong.
    fn write_pixels(&mut self, request: u64, rgba: &[f32]) -> Result<(), String>;
}

impl BufferSink for GpuFlush<'_> {
    fn write(&mut self, id: ComputeBufferId, offset: u64, bytes: &[u8]) -> Result<(), String> {
        self.write_compute_buffer(id.to_bits(), offset, bytes)
            .map_err(|error| error.to_string())
    }

    fn write_pixels(&mut self, request: u64, rgba: &[f32]) -> Result<(), String> {
        self.write_texture_pixels(request, rgba)
            .map_err(|error| error.to_string())
    }
}

impl BufferSink for World {
    fn write(&mut self, id: ComputeBufferId, offset: u64, bytes: &[u8]) -> Result<(), String> {
        let offset = usize::try_from(offset).map_err(|error| error.to_string())?;
        self.set_compute_buffer_data(id, offset, bytes)
            .map_err(|error| error.to_string())
    }

    fn write_pixels(&mut self, request: u64, rgba: &[f32]) -> Result<(), String> {
        self.write_texture_pixels(request, rgba)
            .map_err(|error| error.to_string())
    }
}
