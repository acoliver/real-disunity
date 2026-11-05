//! Creating, updating and releasing the buffers, textures and render targets a runtime owns.

use std::borrow::Cow;

use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::resource::upload_buffer;
use worldrunner_render_gpu::texture::{TextureKind, TextureLayer, TextureUpload, packed_mips};

use crate::error::ProgramError;
use crate::resources::TextureHandle;
use crate::resources::{BufferEntry, BufferHandle, TargetEntry, TargetHandle, TextureEntry};
use crate::runtime::ProgramRuntime;

impl ProgramRuntime {
    /// Uploads `bytes` to a new buffer usable as uniform, storage, vertex and index data.
    ///
    /// # Errors
    /// Fails when the device is lost or rejects the buffer.
    pub fn create_buffer(
        &mut self,
        label: &str,
        bytes: &[u8],
    ) -> Result<BufferHandle, ProgramError> {
        self.authority.check_device()?;
        let contents = padded(bytes);
        let mut allocations = AllocationSet::default();
        if !contents.is_empty() {
            allocations.buffer("render-program buffer", contents.len() as u64, 1);
        }
        let reservation = self.admit(allocations)?;
        let usage = wgpu::BufferUsages::UNIFORM
            | wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::INDEX
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST;
        let buffer = self.guarded(|| {
            upload_buffer(
                &self.device,
                self.authority.queue(),
                label,
                usage,
                &contents,
            )
        })?;
        let entry = BufferEntry {
            buffer,
            len: bytes.len() as u64,
        };
        Ok(self.store.add_buffer(entry, reservation))
    }

    /// Records an overwrite of part of a buffer into `encoder`; the offset and length must be
    /// multiples of four. The bytes are staged in a new buffer and copied when the frame runs,
    /// so the write lands at this point in the frame's order.
    ///
    /// # Errors
    /// Fails on an unknown handle, a range outside the buffer, a misaligned range, or an
    /// encoder that is not the authority's.
    pub fn write_buffer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        handle: BufferHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), ProgramError> {
        self.recording(encoder)?;
        let entry = self.store.buffer(handle)?;
        let len = bytes.len() as u64;
        if offset.checked_add(len).is_none_or(|end| end > entry.len) {
            return Err(ProgramError::WriteRange {
                offset,
                len,
                size: entry.len,
            });
        }
        let alignment = wgpu::COPY_BUFFER_ALIGNMENT;
        if !offset.is_multiple_of(alignment) || !len.is_multiple_of(alignment) {
            return Err(ProgramError::WriteAlignment { offset, len });
        }
        if bytes.is_empty() {
            return Ok(());
        }
        // The staging buffer lives only as long as the recorded copy, so it is admitted for the
        // peak and released before this returns.
        let mut allocations = AllocationSet::default();
        allocations.buffer("render-program write", len, 1);
        let _transient = self.admit(allocations)?;
        let staging = self.guarded(|| {
            upload_buffer(
                &self.device,
                self.authority.queue(),
                "render-program write",
                wgpu::BufferUsages::COPY_SRC,
                bytes,
            )
        })?;
        encoder.copy_buffer_to_buffer(&staging, 0, &entry.buffer, offset, len);
        Ok(())
    }

    /// Uploads tightly packed rows of an uncompressed 2D texture.
    ///
    /// # Errors
    /// Fails when `bytes` is not exactly `width * height` texels of `format`, or the upload
    /// plan or device refuses the texture.
    pub fn create_texture_2d(
        &mut self,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<TextureHandle, ProgramError> {
        self.authority.check_device()?;
        let texel = format
            .block_copy_size(None)
            .filter(|_| format.block_dimensions() == (1, 1))
            .ok_or_else(|| {
                ProgramError::Gpu(format!("{format:?} is not an uncompressed format"))
            })?;
        let needed = size[0] as usize * size[1] as usize * texel as usize;
        if bytes.len() != needed {
            return Err(ProgramError::TextureData {
                size,
                format,
                needed,
                given: bytes.len(),
            });
        }
        let mips = packed_mips(format, size[0], size[1], 1, bytes)?;
        let layer = TextureLayer {
            format,
            width: size[0],
            height: size[1],
            mips: &mips,
        };
        let upload = TextureUpload::new(&[layer], 0, TextureKind::D2)?;
        let mut allocations = AllocationSet::default();
        allocations.texture(
            "render-program texture",
            [size[0], size[1], 1],
            format,
            [1, 1],
            1,
        );
        let reservation = self.admit(allocations)?;
        let texture = self.guarded(|| {
            upload.upload(
                &self.device,
                self.authority.queue(),
                label,
                wgpu::TextureUsages::COPY_SRC,
            )
        })??;
        Ok(self.store.add_texture(entry(texture), reservation))
    }

    /// Creates a render target with a color texture and, when `depth` is a depth format, a
    /// depth texture.
    ///
    /// # Errors
    /// Fails when `depth` is not a depth format or the device is lost.
    pub fn create_render_target(
        &mut self,
        label: &str,
        size: [u32; 2],
        color: wgpu::TextureFormat,
        depth: Option<wgpu::TextureFormat>,
    ) -> Result<TargetHandle, ProgramError> {
        self.authority.check_device()?;
        if let Some(format) = depth.filter(|format| !format.has_depth_aspect()) {
            return Err(ProgramError::Gpu(format!(
                "{format:?} is not a depth format"
            )));
        }
        let mut allocations = AllocationSet::default();
        allocations.texture(
            "render-program target color",
            [size[0], size[1], 1],
            color,
            [1, 1],
            1,
        );
        if let Some(format) = depth {
            allocations.texture(
                "render-program target depth",
                [size[0], size[1], 1],
                format,
                [1, 1],
                1,
            );
        }
        let reservation = self.admit(allocations)?;
        let attachment = wgpu::TextureUsages::RENDER_ATTACHMENT;
        let color = self.new_texture(
            label,
            size,
            color,
            attachment | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth = depth.map(|format| entry(self.new_texture(label, size, format, attachment)));
        let target = TargetEntry {
            color: entry(color),
            depth,
        };
        Ok(self.store.add_target(target, reservation))
    }

    /// Releases a buffer for new work. Its bytes stay in the budget until the work already
    /// submitted has completed on the GPU. Returns whether the handle was live.
    ///
    /// # Errors
    /// Fails while a frame is being recorded, since that frame may use the buffer, and when
    /// the device is lost.
    pub fn release_buffer(&mut self, handle: BufferHandle) -> Result<bool, ProgramError> {
        let _exclusive = self.authority.try_replacement()?;
        let last = self.authority.submitted_serial();
        Ok(self.store.remove_buffer(handle, last))
    }

    /// Releases a texture; see [`release_buffer`](Self::release_buffer).
    ///
    /// # Errors
    /// Fails while a frame is being recorded and when the device is lost.
    pub fn release_texture(&mut self, handle: TextureHandle) -> Result<bool, ProgramError> {
        let _exclusive = self.authority.try_replacement()?;
        let last = self.authority.submitted_serial();
        Ok(self.store.remove_texture(handle, last))
    }

    /// Releases a render target; see [`release_buffer`](Self::release_buffer).
    ///
    /// # Errors
    /// Fails while a frame is being recorded and when the device is lost.
    pub fn release_target(&mut self, handle: TargetHandle) -> Result<bool, ProgramError> {
        let _exclusive = self.authority.try_replacement()?;
        let last = self.authority.submitted_serial();
        Ok(self.store.remove_target(handle, last))
    }

    fn new_texture(
        &self,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    }
}

fn entry(texture: wgpu::Texture) -> TextureEntry {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    TextureEntry { texture, view }
}

/// The bytes, extended with zeros to a multiple of four as buffer copies require.
fn padded(bytes: &[u8]) -> Cow<'_, [u8]> {
    if bytes.len().is_multiple_of(4) {
        return Cow::Borrowed(bytes);
    }
    let mut owned = bytes.to_vec();
    owned.resize(bytes.len().next_multiple_of(4), 0);
    Cow::Owned(owned)
}
