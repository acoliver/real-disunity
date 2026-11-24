//! Reading a texture of the frame back, as the contract's readback work.

use std::sync::mpsc;

use worldrunner_render_post::StageId;
use worldrunner_render_post::contracts::{
    AllocationKey, RasterId, ResourceLifetime, TextureCopy, TypedOperation, TypedWork,
};

use crate::error::ProgramError;
use crate::ids::TextureId;

use super::OffscreenOperations;
use super::error::OffscreenError;
use super::tracker::Tracker;

/// A copy of one mip and layer of a texture, in a staging buffer a submitted frame fills.
pub struct PendingImage {
    buffer: wgpu::Buffer,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    bytes_per_row: u32,
    texel: u32,
}

impl PendingImage {
    /// The width and height in texels.
    #[must_use]
    pub fn size(&self) -> [u32; 2] {
        self.size
    }

    /// The format of the texels.
    #[must_use]
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Waits for the submitted frame and returns the rows, without the padding the copy needs.
    ///
    /// # Panics
    /// Panics when the device is lost or the frame that fills the buffer was never submitted.
    #[must_use]
    pub fn bytes(self, device: &wgpu::Device) -> Vec<u8> {
        self.resolve(device).expect("the staging buffer maps")
    }

    /// As [`bytes`](Self::bytes), naming the failure instead of panicking.
    ///
    /// # Errors
    /// Fails when the device is lost or the buffer cannot be mapped.
    pub fn resolve(self, device: &wgpu::Device) -> Result<Vec<u8>, ProgramError> {
        let failed = |error: String| ProgramError::Readback(error);
        let (sender, receiver) = mpsc::channel();
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // The receiver waits below, so a send can only fail once it is gone.
                let _ = sender.send(result);
            });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| failed(error.to_string()))?;
        receiver
            .recv()
            .map_err(|error| failed(error.to_string()))?
            .map_err(|error| failed(error.to_string()))?;
        let mapped = self
            .buffer
            .slice(..)
            .get_mapped_range()
            .map_err(|error| failed(error.to_string()))?;
        let row = (self.size[0] * self.texel) as usize;
        let rows = mapped
            .chunks(self.bytes_per_row as usize)
            .take(self.size[1] as usize);
        let bytes = rows
            .flat_map(|chunk| chunk[..row].iter().copied())
            .collect();
        drop(mapped);
        self.buffer.unmap();
        Ok(bytes)
    }
}

/// One mip and layer of one allocation of the frame, as a readback copies it.
pub(super) struct ReadSource {
    pub(super) texture: TextureId,
    pub(super) key: AllocationKey,
    pub(super) format: wgpu::TextureFormat,
    /// The extent of the mip.
    pub(super) size: [u32; 2],
    pub(super) aspect: wgpu::TextureAspect,
    pub(super) mip: u32,
    pub(super) layer: u32,
}

/// The operation that copies `source`, as the frame has it written so far, into a new staging
/// buffer, and the handle the submitted frame fills. `id` names the operation.
pub(super) fn readback_operation(
    device: &wgpu::Device,
    tracker: &mut Tracker,
    source: &ReadSource,
    id: RasterId,
) -> Result<(TypedOperation, PendingImage), OffscreenError> {
    let ReadSource {
        texture,
        key,
        format,
        size,
        aspect,
        mip,
        layer,
    } = *source;
    let texel = format
        .block_copy_size(Some(aspect))
        .ok_or(OffscreenError::UnreadableFormat { texture, format })?;
    let bytes_per_row = (size[0] * texel).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("offscreen readback"),
        size: u64::from(bytes_per_row) * u64::from(size[1]),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let buffer_key = tracker.import_buffer(&buffer, ResourceLifetime::FrameLocal, None)?;
    let destination = tracker.buffer_view(buffer_key, 0..buffer.size(), 1)?;
    let version = tracker.cell(key, mip, layer);
    let view = tracker.cell_view(key, (mip, layer), aspect, version)?;
    let operation = TypedOperation::new(
        id,
        TypedWork::ReadbackTexture {
            source: TextureCopy::whole(view),
            destination,
            bytes_per_row,
            rows_per_image: size[1],
        },
    );
    let pending = PendingImage {
        buffer,
        size,
        format,
        bytes_per_row,
        texel,
    };
    Ok((operation, pending))
}

impl OffscreenOperations {
    /// Appends a readback of one mip and layer of a texture of the frame, after every operation so
    /// far, and returns the staging buffer the submitted frame fills.
    ///
    /// # Errors
    /// Fails when the texture is not one the frame uses or has no such mip and layer.
    pub fn read_back(
        &mut self,
        texture: TextureId,
        mip: u32,
        layer: u32,
    ) -> Result<PendingImage, OffscreenError> {
        let imported = self
            .tracker
            .imported(texture)
            .ok_or(OffscreenError::UnknownTexture(texture))?;
        if mip >= imported.mips || layer >= imported.layers {
            return Err(OffscreenError::NoSuchSubresource {
                texture,
                mip,
                element: layer,
            });
        }
        let source = ReadSource {
            texture,
            key: imported.primary,
            format: imported.format,
            size: imported.size.map(|side| (side >> mip).max(1)),
            aspect: wgpu::TextureAspect::All,
            mip,
            layer,
        };
        let id = RasterId::new(
            StageId::new("offscreen readback", 0),
            u32::try_from(self.operations.len()).unwrap_or(u32::MAX),
        );
        let (operation, pending) =
            readback_operation(&self.device, &mut self.tracker, &source, id)?;
        self.operations.push(operation);
        Ok(pending)
    }
}

/// What a read after one record of the frame holds: the record's target in colour and in depth,
/// each as a staging buffer the submitted frame fills, or the reason it cannot be read.
pub struct TapRead {
    /// The index of the record in the frame.
    pub record: usize,
    pub colour: Result<PendingImage, String>,
    pub depth: Result<PendingImage, String>,
}

impl OffscreenOperations {
    /// The reads the frame's `taps` asked for, one for each, which the submitted frame fills.
    /// Take them before the operations are admitted.
    pub fn take_taps(&mut self) -> Vec<TapRead> {
        std::mem::take(&mut self.taps)
    }
}
