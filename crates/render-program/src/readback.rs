//! Recording copies of GPU results into staging buffers, and resolving them once submitted.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::submission::SubmissionAuthority;

use crate::budget::Reservation;
use crate::error::ProgramError;
use crate::resources::{BufferHandle, TargetHandle};
use crate::runtime::ProgramRuntime;

/// The pixels of a render target's color texture, rows tightly packed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetImage {
    pub size: [u32; 2],
    pub format: wgpu::TextureFormat,
    pub bytes: Vec<u8>,
}

impl TargetImage {
    /// The bytes of the pixel at column `x` and row `y`, counted from the top.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> &[u8] {
        let texel = self.bytes.len() / (self.size[0] as usize * self.size[1] as usize);
        let start = (y as usize * self.size[0] as usize + x as usize) * texel;
        &self.bytes[start..start + texel]
    }
}

/// Bytes per row of a texture-to-buffer copy must be a multiple of this.
const ROW_ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

/// How long a resolve waits for the GPU and the map callback together.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// A mappable staging buffer that a recorded copy fills. Its bytes stay in the runtime's
/// budget until it is resolved or dropped and the frame that recorded the copy is cancelled,
/// dropped or completed on the GPU.
struct Staged {
    buffer: wgpu::Buffer,
    device: wgpu::Device,
    authority: SubmissionAuthority,
    reservation: Arc<Reservation>,
}

impl Staged {
    /// Waits for `submission`, which must contain the recorded copy, then reads the buffer.
    fn resolve(self, submission: wgpu::SubmissionIndex) -> Result<Vec<u8>, ProgramError> {
        self.authority.check_device()?;
        let read = self.map(submission);
        self.buffer.unmap();
        read.map_err(|error| {
            self.authority
                .check_device()
                .err()
                .map_or(error, Into::into)
        })
    }

    fn map(&self, submission: wgpu::SubmissionIndex) -> Result<Vec<u8>, ProgramError> {
        let deadline = Instant::now() + READ_TIMEOUT;
        let (sender, receiver) = mpsc::channel();
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // The receiver outlives the wait below; a send can only fail after a timeout.
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(deadline.saturating_duration_since(Instant::now())),
            })
            .map_err(|error| ProgramError::Readback(error.to_string()))?;
        receiver
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|error| ProgramError::Readback(error.to_string()))?
            .map_err(|error| ProgramError::Readback(error.to_string()))?;
        let mapped = self
            .buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| ProgramError::Readback(error.to_string()))?;
        Ok(mapped.to_vec())
    }
}

/// A buffer copy recorded into a frame, to be resolved once the frame has been submitted.
#[must_use = "resolve the readback after submitting the frame it was recorded in"]
pub struct PendingBufferRead {
    staged: Staged,
    len: usize,
}

impl PendingBufferRead {
    /// The buffer's contents as of the recorded copy. `submission` is the index the frame's
    /// submit returned; the wait is bounded.
    ///
    /// # Errors
    /// Fails on device loss, a timeout or a mapping failure.
    pub fn resolve(self, submission: wgpu::SubmissionIndex) -> Result<Vec<u8>, ProgramError> {
        let len = self.len;
        let mut bytes = self.staged.resolve(submission)?;
        bytes.truncate(len);
        Ok(bytes)
    }
}

/// A render target copy recorded into a frame, to be resolved once the frame has been
/// submitted.
#[must_use = "resolve the readback after submitting the frame it was recorded in"]
pub struct PendingTargetRead {
    staged: Staged,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    row: usize,
    padded_row: usize,
}

impl PendingTargetRead {
    /// The target's color texture as of the recorded copy, rows tightly packed.
    ///
    /// # Errors
    /// Fails on device loss, a timeout or a mapping failure.
    pub fn resolve(self, submission: wgpu::SubmissionIndex) -> Result<TargetImage, ProgramError> {
        let (size, format) = (self.size, self.format);
        let (row, padded_row) = (self.row, self.padded_row);
        let staged = self.staged.resolve(submission)?;
        let bytes = staged
            .chunks_exact(padded_row)
            .flat_map(|line| &line[..row])
            .copied()
            .collect();
        Ok(TargetImage {
            size,
            format,
            bytes,
        })
    }
}

impl ProgramRuntime {
    /// Records a copy of a buffer into `encoder`. Submit the frame, then resolve the result
    /// with the submission index. The frame retains the reservations of the buffer and of the
    /// staging buffer, so their bytes stay counted until the frame is cancelled, dropped or
    /// completed on the GPU, even when the runtime or the pending read is dropped first.
    ///
    /// # Errors
    /// Fails on an unknown handle, an encoder that is not the authority's or a lost device.
    pub fn read_buffer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        handle: BufferHandle,
    ) -> Result<PendingBufferRead, ProgramError> {
        self.recording(encoder)?;
        let entry = self.store.buffer(handle)?;
        let size = entry.buffer.size();
        let mut allocations = AllocationSet::default();
        if size > 0 {
            allocations.buffer("render-program readback", size, 1);
        }
        let staged = self.staging(size, allocations)?;
        self.hold_for_frame(
            encoder,
            vec![
                self.store.buffer_reservation(handle)?,
                staged.reservation.clone(),
            ],
        )?;
        encoder.copy_buffer_to_buffer(&entry.buffer, 0, &staged.buffer, 0, size);
        Ok(PendingBufferRead {
            staged,
            len: usize::try_from(entry.len).unwrap_or(usize::MAX),
        })
    }

    /// Records a copy of a render target's color texture into `encoder`. Submit the frame,
    /// then resolve the result with the submission index. The frame retains the reservations of
    /// the target and of the staging buffer, as `read_buffer` does.
    ///
    /// # Errors
    /// Fails on an unknown handle, a format that cannot be copied, an encoder that is not the
    /// authority's or a lost device.
    pub fn read_target(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        handle: TargetHandle,
    ) -> Result<PendingTargetRead, ProgramError> {
        self.recording(encoder)?;
        let target = self.store.target(handle)?;
        let texture = &target.color.texture;
        let (width, height) = (texture.width(), texture.height());
        let texel = texture.format().block_copy_size(None).ok_or_else(|| {
            ProgramError::Readback(format!("{:?} cannot be copied", texture.format()))
        })?;
        let row = width * texel;
        let padded_row = row.next_multiple_of(ROW_ALIGNMENT);
        let mut allocations = AllocationSet::default();
        allocations.readback("render-program readback", [width, height], texel, 1);
        let staged = self.staging(u64::from(padded_row) * u64::from(height), allocations)?;
        self.hold_for_frame(
            encoder,
            vec![
                self.store.target_reservation(handle)?,
                staged.reservation.clone(),
            ],
        )?;
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &staged.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        Ok(PendingTargetRead {
            staged,
            size: [width, height],
            format: texture.format(),
            row: row as usize,
            padded_row: padded_row as usize,
        })
    }

    fn staging(&self, size: u64, allocations: AllocationSet) -> Result<Staged, ProgramError> {
        let reservation = self.admit(allocations)?;
        let buffer = self.guarded(|| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render-program readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        })?;
        Ok(Staged {
            buffer,
            device: self.device.clone(),
            authority: self.authority.clone(),
            reservation: Arc::new(reservation),
        })
    }
}
