use std::{sync::mpsc, time::Duration};

use crate::{FrameConfig, PostError, targets::extent};

#[derive(Debug, thiserror::Error)]
pub enum ReadbackError {
    #[error(transparent)]
    Write(#[from] worldrunner_render_gpu::submission::capture::WriteError),
    #[error(transparent)]
    Transaction(#[from] worldrunner_render_gpu::submission::TransactionError),
    #[error(transparent)]
    Submission(#[from] worldrunner_render_gpu::submission::SubmitError),
    #[error("readback slot already contains an unread capture")]
    Busy,
    #[error(transparent)]
    Config(#[from] PostError),
    #[error("unsupported readback format {0:?}")]
    Format(wgpu::TextureFormat),
    #[error("readback allocation exceeds its byte limit")]
    Limit,
    #[error("short readback: expected {expected} bytes, got {actual}")]
    Short { expected: usize, actual: usize },
    #[error("could not allocate readback image: {0}")]
    Allocation(#[source] std::collections::TryReserveError),
    #[error("GPU readback wait failed: {0}")]
    Poll(#[from] wgpu::PollError),
    #[error("GPU readback mapping failed: {0}")]
    Map(#[from] wgpu::BufferAsyncError),
    #[error("GPU readback completion was not delivered")]
    Callback,
    #[error("GPU readback mapped range unavailable: {0}")]
    Range(#[from] wgpu::MapRangeError),
}

/// Retained padded staging storage. Returns native texture bytes, with no color conversion.
/// A mutable read prevents overlapping maps. Hosts retain submission/window ownership.
#[derive(Debug)]
pub struct Readback {
    buffer: wgpu::Buffer,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    pitch: u32,
    row: usize,
    pending: std::rc::Rc<std::cell::Cell<bool>>,
    encoded_transaction: Option<worldrunner_render_gpu::submission::Publications>,
}

impl Readback {
    pub fn new(
        device: &wgpu::Device,
        config: FrameConfig,
        max_bytes: u64,
    ) -> Result<Self, ReadbackError> {
        FrameConfig {
            format: wgpu::TextureFormat::Rgba8Unorm,
            ..config
        }
        .validate(&device.limits())?;
        if config.samples != 1 {
            return Err(PostError::ReadbackSource.into());
        }
        let bytes = match config.format {
            wgpu::TextureFormat::Rgba8Unorm
            | wgpu::TextureFormat::Rgba8UnormSrgb
            | wgpu::TextureFormat::Bgra8Unorm
            | wgpu::TextureFormat::Bgra8UnormSrgb
            | wgpu::TextureFormat::R32Float
            | wgpu::TextureFormat::Rg16Float => 4,
            wgpu::TextureFormat::Rgba16Float => 8,
            wgpu::TextureFormat::Rgba32Float => 16,
            _ => return Err(ReadbackError::Format(config.format)),
        };
        let row = u64::from(config.size[0]) * bytes;
        let pitch = row.next_multiple_of(u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT));
        let length = pitch
            .checked_mul(u64::from(config.size[1]))
            .ok_or(ReadbackError::Limit)?;
        let buffer = staging_buffer(device, length, max_bytes)?;
        Ok(Self {
            buffer,
            size: config.size,
            format: config.format,
            pitch: u32::try_from(pitch).map_err(|_| ReadbackError::Limit)?,
            row: usize::try_from(row).map_err(|_| ReadbackError::Limit)?,
            pending: std::rc::Rc::default(),
            encoded_transaction: None,
        })
    }

    /// Copies and waits with a bounded deadline. The source must be this device's matching
    /// single-sample `COPY_SRC` texture. Prior rendering must already have been submitted.
    pub fn read(
        &mut self,
        device: &wgpu::Device,
        authority: &worldrunner_render_gpu::submission::SubmissionAuthority,
        source: &wgpu::Texture,
    ) -> Result<Vec<u8>, ReadbackError> {
        let prepared = authority
            .prepare(device, |encoder, _| self.encode(encoder, source))
            .map_err(worldrunner_render_gpu::submission::PrepareError::into_error)?;
        let submission = prepared.submit()?;
        self.read_encoded(device, Some(submission))
    }

    /// Encodes at the caller's capture point. Submit this encoder before `read_encoded`.
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
    ) -> Result<(), ReadbackError> {
        let current = Some(worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?);
        let retired = self
            .encoded_transaction
            .as_ref()
            .is_some_and(worldrunner_render_gpu::submission::Publications::capture_retired);
        if (self.pending.get() && !retired)
            || current.as_ref().is_some_and(|current| {
                self.encoded_transaction
                    .as_ref()
                    .is_some_and(|previous| previous.same_transaction(current))
            })
        {
            return Err(ReadbackError::Busy);
        }
        if source.width() != self.size[0]
            || source.height() != self.size[1]
            || source.format() != self.format
            || source.dimension() != wgpu::TextureDimension::D2
            || source.depth_or_array_layers() != 1
            || source.sample_count() != 1
            || !source.usage().contains(wgpu::TextureUsages::COPY_SRC)
        {
            return Err(PostError::ReadbackSource.into());
        }
        encoder.copy_texture_to_buffer(
            source.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.pitch),
                    rows_per_image: Some(self.size[1]),
                },
            },
            extent(self.size),
        );
        let pending = self.pending.clone();
        worldrunner_render_gpu::submission::Publications::for_encoder(encoder)?
            .stage_write(self.write_key(), move || pending.set(true))?;
        if retired {
            // Retire the old unread copy before replacing its transaction provenance.
            self.pending.set(false);
        }
        self.encoded_transaction = current;
        Ok(())
    }

    /// Reads a previously encoded and submitted copy, using a single bounded deadline.
    /// `None` waits for all submitted work. No transfer, swizzle or float decoding is applied.
    pub fn read_encoded(
        &mut self,
        device: &wgpu::Device,
        submission: Option<wgpu::SubmissionIndex>,
    ) -> Result<Vec<u8>, ReadbackError> {
        let bytes = self.read_encoded_padded(device, submission)?;
        unpad_rows(self.pitch, self.row, self.size[1], &bytes)
    }

    /// Padded native bytes for diagnostic exporters that retain the copy layout in receipts.
    pub fn read_encoded_padded(
        &mut self,
        device: &wgpu::Device,
        submission: Option<wgpu::SubmissionIndex>,
    ) -> Result<Vec<u8>, ReadbackError> {
        let bytes = read_buffer(device, &self.buffer, submission);
        self.pending.set(false);
        bytes
    }

    /// Starts a nonblocking map of this slot's submitted copy.
    pub fn begin_async(&self) -> Result<crate::contracts::AsyncReadback, ReadbackError> {
        if !self.pending.get() {
            return Err(ReadbackError::Callback);
        }
        crate::contracts::AsyncReadback::begin(&self.buffer)
    }

    /// Consumes a completed asynchronous map and releases the retained slot.
    pub fn try_read_async(
        &mut self,
        mapping: &mut crate::contracts::AsyncReadback,
    ) -> Result<Option<Vec<u8>>, ReadbackError> {
        let Some(bytes) = mapping.try_read()? else {
            return Ok(None);
        };
        self.pending.set(false);
        Ok(Some(unpad_rows(
            self.pitch,
            self.row,
            self.size[1],
            &bytes,
        )?))
    }

    /// Include this retained slot's pending-state key in its capture segment write set.
    pub fn write_key(&self) -> worldrunner_render_gpu::submission::capture::WriteKey {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.buffer.hash(&mut hash);
        worldrunner_render_gpu::submission::capture::WriteKey::Readback(hash.finish())
    }
    pub const fn padded_bytes_per_row(&self) -> u32 {
        self.pitch
    }
}

/// Opaque byte readback for caller-authored diagnostic buffers; no inferred ABI.
#[derive(Debug)]
pub struct BufferReadback {
    buffer: wgpu::Buffer,
}

impl BufferReadback {
    pub fn new(device: &wgpu::Device, size: u64, max_bytes: u64) -> Result<Self, ReadbackError> {
        Ok(Self {
            buffer: staging_buffer(device, size, max_bytes)?,
        })
    }

    /// Copies the exact configured byte count from offset zero at the caller's capture point.
    pub fn encode(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Buffer,
    ) -> Result<(), ReadbackError> {
        if source.size() < self.buffer.size()
            || !source.usage().contains(wgpu::BufferUsages::COPY_SRC)
        {
            return Err(PostError::ReadbackSource.into());
        }
        encoder.copy_buffer_to_buffer(source, 0, &self.buffer, 0, self.buffer.size());
        Ok(())
    }

    pub fn read_encoded(
        &mut self,
        device: &wgpu::Device,
        submission: Option<wgpu::SubmissionIndex>,
    ) -> Result<Vec<u8>, ReadbackError> {
        read_buffer(device, &self.buffer, submission)
    }
}

fn staging_buffer(
    device: &wgpu::Device,
    size: u64,
    max_bytes: u64,
) -> Result<wgpu::Buffer, ReadbackError> {
    if size == 0
        || !size.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
        || size > max_bytes.min(device.limits().max_buffer_size)
    {
        return Err(ReadbackError::Limit);
    }
    Ok(device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("post diagnostic readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    }))
}

fn read_buffer(
    device: &wgpu::Device,
    buffer: &wgpu::Buffer,
    submission: Option<wgpu::SubmissionIndex>,
) -> Result<Vec<u8>, ReadbackError> {
    let result = (|| {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let (send, receive) = mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        wait_for_map(
            deadline,
            |timeout| {
                device.poll(wgpu::PollType::Wait {
                    submission_index: submission,
                    timeout: Some(timeout),
                })
            },
            |timeout| receive.recv_timeout(timeout),
        )?;
        let mapped = buffer.slice(..).get_mapped_range()?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(mapped.len())
            .map_err(ReadbackError::Allocation)?;
        bytes.extend_from_slice(&mapped);
        Ok(bytes)
    })();
    buffer.unmap();
    result
}

/// Shares a single deadline between GPU polling and callback delivery, including concurrent pollers.
pub fn wait_for_map(
    deadline: std::time::Instant,
    poll: impl FnOnce(Duration) -> Result<wgpu::PollStatus, wgpu::PollError>,
    receive: impl FnOnce(Duration) -> Result<Result<(), wgpu::BufferAsyncError>, mpsc::RecvTimeoutError>,
) -> Result<(), ReadbackError> {
    poll(deadline.saturating_duration_since(std::time::Instant::now()))?;
    receive(deadline.saturating_duration_since(std::time::Instant::now())).map_err(|error| {
        match error {
            mpsc::RecvTimeoutError::Timeout => ReadbackError::Poll(wgpu::PollError::Timeout),
            mpsc::RecvTimeoutError::Disconnected => ReadbackError::Callback,
        }
    })??;
    Ok(())
}

/// Checks the complete padded extent, then strips rows without changing texture bytes.
pub fn unpad_rows(
    pitch: u32,
    row: usize,
    height: u32,
    bytes: &[u8],
) -> Result<Vec<u8>, ReadbackError> {
    let pitch = usize::try_from(pitch).map_err(|_| ReadbackError::Limit)?;
    let height = usize::try_from(height).map_err(|_| ReadbackError::Limit)?;
    let expected = pitch.checked_mul(height).ok_or(ReadbackError::Limit)?;
    if pitch == 0 || row > pitch {
        return Err(ReadbackError::Limit);
    }
    if bytes.len() < expected {
        return Err(ReadbackError::Short {
            expected,
            actual: bytes.len(),
        });
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(row.checked_mul(height).ok_or(ReadbackError::Limit)?)
        .map_err(ReadbackError::Allocation)?;
    for bytes in bytes[..expected].chunks_exact(pitch) {
        result.extend_from_slice(&bytes[..row]);
    }
    Ok(result)
}
