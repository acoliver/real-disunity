//! Creating, updating and releasing the buffers, textures and render targets a runtime owns.

use std::borrow::Cow;
use std::sync::Arc;

use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::resource::upload_buffer;
use worldrunner_render_gpu::texture::{TextureKind, TextureLayer, TextureUpload, packed_mips};

use crate::bind::Snapshot;
use crate::binding::BindingAddress;
use crate::budget::{Note, Reservation};
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
        let (buffer, reservation) = self.upload(label, bytes)?;
        let entry = BufferEntry {
            buffer,
            len: bytes.len() as u64,
        };
        Ok(self.store.add_buffer(entry, reservation))
    }

    /// A buffer of constants that only a prepared draw uses, with the reservation that counts
    /// its bytes. The draw and the frames it is recorded into share the reservation, so the
    /// bytes are counted until the last of them is done and need no release.
    pub(crate) fn snapshot_buffer(
        &self,
        at: BindingAddress,
        bytes: &[u8],
    ) -> Result<Snapshot, ProgramError> {
        let (buffer, reservation) = self.upload("render-program constants", bytes)?;
        Ok(Snapshot {
            at,
            buffer,
            len: bytes.len() as u64,
            reservation: Arc::new(reservation),
        })
    }

    /// Admits and uploads `bytes` to a new buffer usable as uniform, storage, vertex and index
    /// data.
    fn upload(
        &self,
        label: &str,
        bytes: &[u8],
    ) -> Result<(wgpu::Buffer, Reservation), ProgramError> {
        self.authority.check_device()?;
        let contents = padded(bytes);
        let mut allocations = AllocationSet::default();
        if !contents.is_empty() {
            allocations.buffer("render-program buffer", contents.len() as u64, 1);
        }
        let reservation = self.admit(allocations, Note::new(label, ""))?;
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
        Ok((buffer, reservation))
    }

    /// Records an overwrite of part of a buffer into `encoder`; the offset and length must be
    /// multiples of four. The bytes are staged in a new buffer and copied when the frame runs,
    /// so the write lands at this point in the frame's order. The frame retains the reservations
    /// of the buffer and of the staging buffer, so their bytes stay counted until the frame is
    /// cancelled, dropped or completed on the GPU, even when the runtime is dropped first.
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
        // The staging buffer lives as long as the recorded copy may run, so its bytes are
        // admitted now and held by the frame.
        let mut allocations = AllocationSet::default();
        allocations.buffer("render-program write", len, 1);
        let staged = Arc::new(self.admit(allocations, Note::new("write staging", ""))?);
        let staging = self.guarded(|| {
            upload_buffer(
                &self.device,
                self.authority.queue(),
                "render-program write",
                wgpu::BufferUsages::COPY_SRC,
                bytes,
            )
        })?;
        self.hold_for_frame(
            encoder,
            vec![self.store.buffer_reservation(handle)?, staged],
        )?;
        encoder.copy_buffer_to_buffer(&staging, 0, &entry.buffer, offset, len);
        Ok(())
    }

    /// Uploads the base level of a 2D texture as tightly packed rows, or rows of blocks for a
    /// block-compressed format.
    ///
    /// # Errors
    /// Fails when `bytes` is not exactly the whole texels or blocks of the level in `format`, or
    /// the upload plan or device refuses the texture, as a block format the device cannot sample
    /// or an extent that is not a whole number of blocks.
    pub fn create_texture_2d(
        &mut self,
        label: &str,
        size: [u32; 2],
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<TextureHandle, ProgramError> {
        self.authority.check_device()?;
        let block = format
            .block_copy_size(None)
            .filter(|_| !format.is_depth_stencil_format() && !format.is_multi_planar_format())
            .ok_or_else(|| ProgramError::Gpu(format!("{format:?} is not a color format")))?;
        let (block_width, block_height) = format.block_dimensions();
        let needed = size[0].div_ceil(block_width) as usize
            * size[1].div_ceil(block_height) as usize
            * block as usize;
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
        let note = Note::new(label, format!("{}x{} {format:?}", size[0], size[1]));
        let reservation = self.admit(allocations, note)?;
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
        let note = Note::new(
            label,
            format!("{}x{} {color:?} with depth {depth:?}", size[0], size[1]),
        );
        let reservation = self.admit(allocations, note)?;
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

    /// Creates an empty 2D texture of `layers` array layers and `mips` mip levels, usable as a
    /// render attachment, as a sampled texture and as a copy source and destination. A new
    /// texture holds zeros.
    ///
    /// # Errors
    /// Fails when the budget or the device refuses the texture.
    pub(crate) fn create_empty_texture(
        &mut self,
        label: &str,
        shape: TextureShape,
    ) -> Result<TextureHandle, ProgramError> {
        self.authority.check_device()?;
        let mut allocations = AllocationSet::default();
        allocations.texture(
            "render-program render texture",
            [shape.size[0], shape.size[1], shape.layers],
            shape.format,
            [shape.mips, 1],
            1,
        );
        let note = Note::new(
            label,
            format!(
                "{}x{}x{} {:?} {} mips",
                shape.size[0], shape.size[1], shape.layers, shape.format, shape.mips
            ),
        );
        let reservation = self.admit(allocations, note)?;
        let texture = self.guarded(|| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: shape.size[0],
                    height: shape.size[1],
                    depth_or_array_layers: shape.layers,
                },
                mip_level_count: shape.mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: shape.format,
                usage: shape.usage(),
                view_formats: &[],
            })
        })?;
        Ok(self.store.add_texture(entry(texture), reservation))
    }

    /// Creates a texture that kernels bind as a storage image and draws sample, holding zeros.
    /// `size` is width, height and, for a 2D texture, the number of array layers or, for a 3D
    /// texture, the depth; `dimension` is 2D or 3D. It has one mip level, can be copied from and
    /// to, and is bound for storage through its first and only mip level.
    ///
    /// # Errors
    /// Fails with `UnsupportedStorageFormat` when the device cannot bind `format` as a storage
    /// image, and when the dimension is not 2D or 3D, the size has a zero, or the budget
    /// or the device refuses the texture.
    pub fn create_storage_texture(
        &mut self,
        label: &str,
        size: [u32; 3],
        dimension: wgpu::TextureDimension,
        format: wgpu::TextureFormat,
    ) -> Result<TextureHandle, ProgramError> {
        self.authority.check_device()?;
        if dimension == wgpu::TextureDimension::D1 || size.contains(&0) {
            return Err(ProgramError::InvalidAllocation(
                "a storage texture is 2D or 3D and has no empty side",
            ));
        }
        // A texture that one access can bind may be bound with any weaker one.
        let access = wgpu::StorageTextureAccess::WriteOnly;
        if !self.supports_storage_image(format, access) {
            return Err(ProgramError::UnsupportedStorageFormat { format, access });
        }
        let mut allocations = AllocationSet::default();
        allocations.texture("render-program storage texture", size, format, [1, 1], 1);
        let note = Note::new(
            label,
            format!(
                "{}x{}x{} {dimension:?} {format:?} storage",
                size[0], size[1], size[2]
            ),
        );
        let reservation = self.admit(allocations, note)?;
        let texture = self.guarded(|| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: size[2],
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension,
                format,
                usage: wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        })?;
        Ok(self.store.add_texture(entry(texture), reservation))
    }

    /// The wgpu texture behind a handle.
    pub(crate) fn gpu_texture(&self, handle: TextureHandle) -> Result<wgpu::Texture, ProgramError> {
        Ok(self.store.texture(handle)?.texture.clone())
    }

    /// The wgpu buffer behind a handle.
    pub(crate) fn gpu_buffer(&self, handle: BufferHandle) -> Result<wgpu::Buffer, ProgramError> {
        Ok(self.store.buffer(handle)?.buffer.clone())
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

/// The shape of a texture [`ProgramRuntime::create_empty_texture`] makes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TextureShape {
    pub(crate) size: [u32; 2],
    pub(crate) layers: u32,
    pub(crate) mips: u32,
    pub(crate) format: wgpu::TextureFormat,
}

impl TextureShape {
    fn usage(self) -> wgpu::TextureUsages {
        let attachment = if self.format.is_compressed() {
            wgpu::TextureUsages::empty()
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        attachment
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
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
