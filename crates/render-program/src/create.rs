//! Creating, updating and releasing the buffers, textures and render targets a runtime owns.

use std::borrow::Cow;
use std::sync::Arc;

use worldrunner_render_gpu::configuration::AllocationSet;
use worldrunner_render_gpu::resource::upload_buffer;
use worldrunner_render_gpu::texture::{TextureKind, TextureLayer, TextureUpload, packed_mips};
use worldrunner_render_gpu::volume::{VolumeUpload, packed_volume_levels};

use crate::bind::Snapshot;
use crate::binding::BindingAddress;
use crate::budget::{Note, Reservation};
use crate::error::ProgramError;
use crate::import::default_dimension;
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
        let reservation = self.admit(&allocations, Note::new(label, ""))?;
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
        let staged = Arc::new(self.admit(&allocations, Note::new("write staging", ""))?);
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
        self.create_source_texture(
            label,
            SourceShape::flat(TextureKind::D2, size, 1),
            format,
            bytes,
        )
    }

    /// Uploads a cube of six `edge` by `edge` faces, each with `mips` levels, sampled through a
    /// cube view. `bytes` holds the faces one after the other in the order +X, -X, +Y, -Y, +Z, -Z,
    /// each holding its levels from the base level down, every level tightly packed as
    /// [`create_texture_2d`](Self::create_texture_2d) takes one. All levels are counted against
    /// the budget.
    ///
    /// # Errors
    /// Fails when `bytes` is not exactly the six faces of `mips` levels, `mips` is zero or more
    /// than the edge has, or the upload plan or device refuses the texture, as for a 2D texture.
    pub fn create_texture_cube(
        &mut self,
        label: &str,
        edge: u32,
        mips: u32,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<TextureHandle, ProgramError> {
        let shape = SourceShape {
            kind: TextureKind::Cube,
            size: [edge, edge],
            faces: 6,
            mips,
        };
        self.create_source_texture(label, shape, format, bytes)
    }

    /// Uploads a 2D array of `layers` layers of `size`, each with `mips` levels, sampled through an
    /// array view even when there is one layer. `bytes` holds the layers one after the other,
    /// each holding its levels from the base level down, every level tightly packed as
    /// [`create_texture_2d`](Self::create_texture_2d) takes one. All levels are counted against
    /// the budget.
    ///
    /// # Errors
    /// Fails when `bytes` is not exactly the `layers` layers of `mips` levels, `layers` is zero,
    /// `mips` is zero or more than the size has, or the upload plan or device refuses the
    /// texture, as for a 2D texture.
    pub fn create_texture_array(
        &mut self,
        label: &str,
        size: [u32; 2],
        layers: u32,
        mips: u32,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<TextureHandle, ProgramError> {
        let shape = SourceShape {
            kind: TextureKind::Array,
            size,
            faces: layers,
            mips,
        };
        self.create_source_texture(label, shape, format, bytes)
    }

    /// Uploads a 3D texture of `size` (width, height and depth) with `mips` levels, sampled
    /// through a 3D view. `bytes` holds the levels one after the other from the base level, each
    /// holding its slices one after the other and as wide, as high and as deep as the base level
    /// shifted down (at least one texel), every slice tightly packed as
    /// [`create_texture_2d`](Self::create_texture_2d) takes a level. All levels are counted
    /// against the budget. Block-compressed formats are not uploaded as volumes.
    ///
    /// # Errors
    /// Fails when `bytes` is not exactly the levels, an extent or `mips` is zero, `mips` is more
    /// than the size has, the format is not an uncompressed color format, or the device's 3D
    /// extent limit is below the size.
    pub fn create_texture_3d(
        &mut self,
        label: &str,
        size: [u32; 3],
        mips: u32,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<TextureHandle, ProgramError> {
        self.authority.check_device()?;
        let levels = packed_volume_levels(format, size, mips, bytes)?;
        let needed: usize = levels.iter().map(|level| level.bytes.len()).sum();
        if bytes.len() != needed {
            return Err(ProgramError::TextureData {
                size: [size[0], size[1]],
                format,
                needed,
                given: bytes.len(),
            });
        }
        let upload = VolumeUpload::new(format, &levels)?;
        let mut allocations = AllocationSet::default();
        allocations.volume("render-program texture", size, format, mips);
        let note = Note::new(
            label,
            format!("{}x{}x{} {format:?}", size[0], size[1], size[2]),
        );
        let reservation = self.admit(&allocations, note)?;
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

    /// The upload of a 2D texture (one layer), a cube (six) or an array (`faces`), each face
    /// holding `mips` levels, `size` being one face.
    fn create_source_texture(
        &mut self,
        label: &str,
        shape: SourceShape,
        format: wgpu::TextureFormat,
        bytes: &[u8],
    ) -> Result<TextureHandle, ProgramError> {
        let SourceShape {
            kind,
            size,
            faces,
            mips: levels,
        } = shape;
        self.authority.check_device()?;
        let block = format
            .block_copy_size(None)
            .filter(|_| !format.is_depth_stencil_format() && !format.is_multi_planar_format())
            .ok_or_else(|| ProgramError::Gpu(format!("{format:?} is not a color format")))?;
        let face_len = chain_len(format, shape, block);
        let needed = face_len * faces as usize;
        if bytes.len() != needed {
            return Err(ProgramError::TextureData {
                size,
                format,
                needed,
                given: bytes.len(),
            });
        }
        let mips = (0..faces as usize)
            .map(|face| {
                packed_mips(
                    format,
                    size[0],
                    size[1],
                    levels,
                    &bytes[face * face_len..(face + 1) * face_len],
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let layers: Vec<TextureLayer<'_>> = mips
            .iter()
            .map(|mips| TextureLayer {
                format,
                width: size[0],
                height: size[1],
                mips,
            })
            .collect();
        let upload = TextureUpload::new(&layers, 0, kind)?;
        let mut allocations = AllocationSet::default();
        allocations.texture(
            "render-program texture",
            [size[0], size[1], faces],
            format,
            [levels, 1],
            1,
        );
        let note = Note::new(label, format!("{}x{} {format:?}", size[0], size[1]));
        let reservation = self.admit(&allocations, note)?;
        let texture = self.guarded(|| {
            upload.upload(
                &self.device,
                self.authority.queue(),
                label,
                wgpu::TextureUsages::COPY_SRC,
            )
        })??;
        let entry = match kind {
            TextureKind::Cube => entry_viewed(texture, wgpu::TextureViewDimension::Cube),
            TextureKind::Array => entry_viewed(texture, wgpu::TextureViewDimension::D2Array),
            TextureKind::D2 => entry(texture),
        };
        Ok(self.store.add_texture(entry, reservation))
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
        let reservation = self.admit(&allocations, note)?;
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
    /// render attachment, as a sampled texture and as a copy source and destination, or a 3D
    /// texture of that depth, which is not a render attachment. A new texture holds zeros.
    ///
    /// # Errors
    /// Fails when the budget or the device refuses the texture.
    pub fn create_empty_texture(
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
        let reservation = self.admit(&allocations, note)?;
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
                dimension: if shape.volume() {
                    wgpu::TextureDimension::D3
                } else {
                    wgpu::TextureDimension::D2
                },
                format: shape.format,
                usage: shape.usage(),
                view_formats: &[],
            })
        })?;
        Ok(self
            .store
            .add_texture(entry_viewed(texture, shape.view), reservation))
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
        let reservation = self.admit(&allocations, note)?;
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

    /// The wgpu texture behind a handle, for work the runtime does not record itself, such as
    /// sampling it after the submission that writes it or reading a block-compressed texture
    /// back. The result shares wgpu's reference-counted texture; the runtime still counts its
    /// storage in the budget until the handle is released. Drop the clone when the allocation
    /// is replaced: the runtime cannot track references the caller retains.
    ///
    /// # Errors
    /// Returns an error for an unknown handle.
    pub fn gpu_texture(&self, handle: TextureHandle) -> Result<wgpu::Texture, ProgramError> {
        Ok(self.store.texture(handle)?.texture.clone())
    }

    /// The wgpu buffer behind a handle.
    #[cfg(feature = "unity")]
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
pub struct TextureShape {
    pub size: [u32; 2],
    pub layers: u32,
    pub mips: u32,
    pub format: wgpu::TextureFormat,
    /// Whether a kernel binds the texture as a storage image, which the format must allow.
    pub storage: bool,
    /// The dimension of the default view, which is the one a kernel or a draw samples the texture
    /// through. A 3D view makes a 3D texture with `layers` as its depth.
    pub view: wgpu::TextureViewDimension,
}

impl TextureShape {
    fn volume(self) -> bool {
        self.view == wgpu::TextureViewDimension::D3
    }

    fn usage(self) -> wgpu::TextureUsages {
        let attachment = if self.format.is_compressed() || self.volume() {
            wgpu::TextureUsages::empty()
        } else {
            wgpu::TextureUsages::RENDER_ATTACHMENT
        };
        let storage = if self.storage {
            wgpu::TextureUsages::STORAGE_BINDING
        } else {
            wgpu::TextureUsages::empty()
        };
        attachment
            | storage
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
    }
}

fn entry(texture: wgpu::Texture) -> TextureEntry {
    let dimension = default_dimension(&texture);
    entry_viewed(texture, dimension)
}

/// A texture with the default view made in `dimension`, which a one-layer array or a cube needs
/// said, since the default for a texture is a plain 2D view or an array view by its layers.
fn entry_viewed(texture: wgpu::Texture, dimension: wgpu::TextureViewDimension) -> TextureEntry {
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(dimension),
        ..wgpu::TextureViewDescriptor::default()
    });
    TextureEntry {
        texture,
        view,
        dimension,
    }
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

/// What an upload of a source texture holds: `faces` layers or cube faces, each of `size` with
/// `mips` levels.
#[derive(Clone, Copy)]
struct SourceShape {
    kind: TextureKind,
    size: [u32; 2],
    faces: u32,
    mips: u32,
}

impl SourceShape {
    /// A texture whose faces have the base level alone.
    fn flat(kind: TextureKind, size: [u32; 2], faces: u32) -> Self {
        Self {
            kind,
            size,
            faces,
            mips: 1,
        }
    }
}

/// The bytes of one face's levels, base level first, each tightly packed in blocks of `block`
/// bytes. Levels are as wide as the base level shifted down, and at least one texel.
fn chain_len(format: wgpu::TextureFormat, shape: SourceShape, block: u32) -> usize {
    let (block_width, block_height) = format.block_dimensions();
    let level = |side: u32, level: u32| side.checked_shr(level).unwrap_or(0).max(1);
    (0..shape.mips)
        .map(|mip| {
            level(shape.size[0], mip).div_ceil(block_width) as usize
                * level(shape.size[1], mip).div_ceil(block_height) as usize
                * block as usize
        })
        .sum()
}
