//! Recording copies of GPU results into staging buffers, and resolving them once submitted.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_post::contracts::AsyncReadback;

use crate::budget::{Note, Reservation};
use crate::error::ProgramError;
use crate::resources::{BufferHandle, TargetHandle, TextureHandle};
use crate::runtime::ProgramRuntime;

/// One level of a texture or a render target's color texture, rows tightly packed: rows of
/// texels, or rows of blocks for a block-compressed format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetImage {
    /// The level's extent in texels; a block-compressed level's rows cover whole blocks.
    pub size: [u32; 2],
    pub format: wgpu::TextureFormat,
    pub bytes: Vec<u8>,
}

impl TargetImage {
    /// The bytes of the pixel at column `x` and row `y`, counted from the top, of an
    /// uncompressed format.
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

/// One mip level of one array layer (or, for a 3D texture, one slice of the level) of a
/// texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextureLevel {
    pub layer: u32,
    pub mip: u32,
}

impl TextureLevel {
    /// The staging bytes a readback of this level of `texture` admits.
    ///
    /// # Errors
    /// As [`ProgramRuntime::read_lent_texture`] refuses the level or the texture.
    pub fn readback_bytes(self, texture: &wgpu::Texture) -> Result<u64, ProgramError> {
        Ok(LevelCopy::of(texture, self)?.staging_bytes())
    }
}

/// The copy of one level of a texture into a staging buffer, in rows of blocks.
#[derive(Clone, Copy, Debug)]
struct LevelCopy {
    /// The level's logical extent in texels.
    size: [u32; 2],
    /// The level's extent in blocks (texels for an uncompressed format).
    blocks: [u32; 2],
    block_bytes: u32,
    /// The texels copied: the level's physical extent, whole blocks.
    extent: wgpu::Extent3d,
}

impl LevelCopy {
    fn of(texture: &wgpu::Texture, level: TextureLevel) -> Result<Self, ProgramError> {
        let format = texture.format();
        let refuse = |why: String| Err(ProgramError::Readback(why));
        if level.mip >= texture.mip_level_count() {
            return refuse(format!(
                "mip {} is outside the {} mips of the texture",
                level.mip,
                texture.mip_level_count()
            ));
        }
        let size = texture
            .size()
            .mip_level_size(level.mip, texture.dimension());
        if level.layer >= size.depth_or_array_layers {
            return refuse(format!(
                "layer {} is outside the {} layers of mip {}",
                level.layer, size.depth_or_array_layers, level.mip
            ));
        }
        let block_bytes = format
            .block_copy_size(None)
            .ok_or_else(|| ProgramError::Readback(format!("{format:?} cannot be copied")))?;
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return refuse("the texture was not made with COPY_SRC".to_owned());
        }
        if texture.sample_count() != 1 {
            return refuse("a multisampled texture cannot be copied".to_owned());
        }
        let physical = size.physical_size(format);
        let (block_width, block_height) = format.block_dimensions();
        Ok(Self {
            size: [size.width, size.height],
            blocks: [physical.width / block_width, physical.height / block_height],
            block_bytes,
            extent: wgpu::Extent3d {
                width: physical.width,
                height: physical.height,
                depth_or_array_layers: 1,
            },
        })
    }

    fn row(self) -> u32 {
        self.blocks[0] * self.block_bytes
    }

    fn padded_row(self) -> u32 {
        self.row().next_multiple_of(ROW_ALIGNMENT)
    }

    fn staging_bytes(self) -> u64 {
        u64::from(self.padded_row()) * u64::from(self.blocks[1])
    }

    fn layout(self, format: wgpu::TextureFormat) -> ImageLayout {
        ImageLayout {
            size: self.size,
            format,
            row: self.row() as usize,
            padded_row: self.padded_row() as usize,
        }
    }
}

/// How a level's rows sit in the staging buffer and in the image made from it.
#[derive(Clone, Copy, Debug)]
struct ImageLayout {
    size: [u32; 2],
    format: wgpu::TextureFormat,
    /// Bytes of one row of texels, or of blocks for a block-compressed format.
    row: usize,
    padded_row: usize,
}

impl ImageLayout {
    fn image(self, staged: &[u8]) -> TargetImage {
        let bytes = staged
            .chunks_exact(self.padded_row)
            .flat_map(|line| &line[..self.row])
            .copied()
            .collect();
        TargetImage {
            size: self.size,
            format: self.format,
            bytes,
        }
    }
}

/// A texture copy recorded into a frame, to be resolved once the frame has been submitted.
#[must_use = "resolve the readback after submitting the frame it was recorded in"]
pub struct PendingTargetRead {
    staged: Staged,
    layout: ImageLayout,
}

impl PendingTargetRead {
    /// The copied level as of the recorded copy, rows tightly packed.
    ///
    /// # Errors
    /// Fails on device loss, a timeout or a mapping failure.
    pub fn resolve(self, submission: wgpu::SubmissionIndex) -> Result<TargetImage, ProgramError> {
        let layout = self.layout;
        let staged = self.staged.resolve(submission)?;
        Ok(layout.image(&staged))
    }

    /// Starts reading the copy without waiting, through render-post's [`AsyncReadback`]: call
    /// it once the frame that recorded the copy has been submitted.
    /// [`MappingTargetRead::try_finish`] gives the image once the GPU has completed that frame
    /// and the host has polled the device.
    ///
    /// # Errors
    /// Fails when the map cannot be asked for.
    pub fn begin(self) -> Result<MappingTargetRead, ProgramError> {
        let mapping = AsyncReadback::begin(&self.staged.buffer)
            .map_err(|error| ProgramError::Readback(error.to_string()))?;
        Ok(MappingTargetRead {
            staged: self.staged,
            layout: self.layout,
            mapping: Some(mapping),
        })
    }
}

/// A texture copy whose staging buffer is being mapped. Dropping it gives the staging buffer
/// and its reservation back.
#[must_use = "finish the readback or drop it"]
pub struct MappingTargetRead {
    staged: Staged,
    layout: ImageLayout,
    /// The map, until the image is delivered or the map fails.
    mapping: Option<AsyncReadback>,
}

impl MappingTargetRead {
    /// The copied level, once the buffer is mapped; `None` while the map is pending. It is
    /// delivered once.
    ///
    /// # Errors
    /// Fails on device loss, a mapping failure and when the image was already delivered.
    pub fn try_finish(&mut self) -> Result<Option<TargetImage>, ProgramError> {
        self.staged.authority.check_device()?;
        let mapping = self
            .mapping
            .as_mut()
            .ok_or_else(|| ProgramError::Readback("the read was already delivered".to_owned()))?;
        let read = mapping.try_read();
        if !matches!(read, Ok(None)) {
            self.mapping = None;
        }
        let staged = read.map_err(|error| ProgramError::Readback(error.to_string()))?;
        Ok(staged.map(|staged| self.layout.image(&staged)))
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
        let staged = self.staging(size, &allocations)?;
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
        self.copy_texture(
            encoder,
            &target.color.texture,
            TextureLevel::default(),
            vec![self.store.target_reservation(handle)?],
        )
    }

    /// Records a copy of the first mip and layer of a texture into `encoder`, as `read_target`
    /// does for a render target. The texture needs to allow copies from it.
    ///
    /// # Errors
    /// Fails on an unknown handle, a format that cannot be copied, an encoder that is not the
    /// authority's or a lost device.
    pub fn read_texture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        handle: TextureHandle,
    ) -> Result<PendingTargetRead, ProgramError> {
        self.recording(encoder)?;
        let entry = self.store.texture(handle)?;
        self.copy_texture(
            encoder,
            &entry.texture,
            TextureLevel::default(),
            vec![self.store.texture_reservation(handle)?],
        )
    }

    /// Records a copy of one level of a texture another owner holds and keeps counted, as
    /// `read_texture` does for a texture of the runtime: the level's rows of texels, or of
    /// blocks for a block-compressed format. The staging buffer is admitted against this
    /// runtime's budget and held by the frame until it is cancelled, dropped or completed on the
    /// GPU; wgpu keeps the texture alive for the copy.
    ///
    /// # Errors
    /// Fails on a level outside the texture, a texture without `COPY_SRC`, a format that cannot
    /// be copied, an encoder that is not the authority's or a lost device.
    pub fn read_lent_texture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        level: TextureLevel,
    ) -> Result<PendingTargetRead, ProgramError> {
        self.recording(encoder)?;
        self.copy_texture(encoder, texture, level, Vec::new())
    }

    fn copy_texture(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        level: TextureLevel,
        mut shares: Vec<Arc<Reservation>>,
    ) -> Result<PendingTargetRead, ProgramError> {
        let copy = LevelCopy::of(texture, level)?;
        let mut allocations = AllocationSet::default();
        allocations.readback("render-program readback", copy.blocks, copy.block_bytes, 1);
        let staged = self.staging(copy.staging_bytes(), &allocations)?;
        shares.push(staged.reservation.clone());
        self.hold_for_frame(encoder, shares)?;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level.mip,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: level.layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staged.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(copy.padded_row()),
                    rows_per_image: Some(copy.blocks[1]),
                },
            },
            copy.extent,
        );
        Ok(PendingTargetRead {
            staged,
            layout: copy.layout(texture.format()),
        })
    }

    fn staging(&self, size: u64, allocations: &AllocationSet) -> Result<Staged, ProgramError> {
        let reservation = self.admit(allocations, Note::new("readback staging", ""))?;
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
