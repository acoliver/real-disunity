//! Reading a texture of the frame back, as the contract's readback work.

use worldrunner_render_post::StageId;
use worldrunner_render_post::contracts::{
    RasterId, ResourceLifetime, TextureCopy, TypedOperation, TypedWork,
};

use crate::ids::TextureId;

use super::OffscreenOperations;
use super::error::OffscreenError;

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
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |result| {
                result.expect("the staging buffer maps");
            });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("the device is polled");
        let mapped = self
            .buffer
            .slice(..)
            .get_mapped_range()
            .expect("the staging buffer is mapped");
        let row = (self.size[0] * self.texel) as usize;
        let rows = mapped
            .chunks(self.bytes_per_row as usize)
            .take(self.size[1] as usize);
        let bytes = rows
            .flat_map(|chunk| chunk[..row].iter().copied())
            .collect();
        drop(mapped);
        self.buffer.unmap();
        bytes
    }
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
        let size = imported.size.map(|side| (side >> mip).max(1));
        let texel =
            imported
                .format
                .block_copy_size(None)
                .ok_or(OffscreenError::UnreadableFormat {
                    texture,
                    format: imported.format,
                })?;
        let bytes_per_row = (size[0] * texel).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen readback"),
            size: u64::from(bytes_per_row) * u64::from(size[1]),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let key = self
            .tracker
            .import_buffer(&buffer, ResourceLifetime::FrameLocal, None)?;
        let destination = self.tracker.buffer_view(key, 0..buffer.size(), 1)?;
        let version = self.tracker.cell(imported.primary, mip, layer);
        let source = self.tracker.cell_view(
            imported.primary,
            (mip, layer),
            wgpu::TextureAspect::All,
            version,
        )?;
        let id = RasterId::new(
            StageId::new("offscreen readback", 0),
            u32::try_from(self.operations.len()).unwrap_or(u32::MAX),
        );
        self.operations.push(TypedOperation::new(
            id,
            TypedWork::ReadbackTexture {
                source: TextureCopy::whole(source),
                destination,
                bytes_per_row,
                rows_per_image: size[1],
            },
        ));
        Ok(PendingImage {
            buffer,
            size,
            format: imported.format,
            bytes_per_row,
            texel,
        })
    }
}
