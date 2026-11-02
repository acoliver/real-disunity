//! Copying GPU results back to the CPU.

use std::sync::mpsc;

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

impl ProgramRuntime {
    /// The contents of a buffer, after the submitted work that writes it has finished.
    ///
    /// # Errors
    /// Fails on an unknown handle or when the GPU cannot map the copy.
    pub fn read_buffer(&self, handle: BufferHandle) -> Result<Vec<u8>, ProgramError> {
        let entry = self.store.buffer(handle)?;
        let size = entry.buffer.size();
        let mut bytes = self.read_staged(size, |encoder, staging| {
            encoder.copy_buffer_to_buffer(&entry.buffer, 0, staging, 0, size);
        })?;
        bytes.truncate(usize::try_from(entry.len).unwrap_or(usize::MAX));
        Ok(bytes)
    }

    /// The color texture of a render target, after the submitted work that draws it has
    /// finished.
    ///
    /// # Errors
    /// Fails on an unknown handle or when the GPU cannot map the copy.
    pub fn read_target(&self, handle: TargetHandle) -> Result<TargetImage, ProgramError> {
        let target = self.store.target(handle)?;
        let texture = &target.color.texture;
        let (width, height) = (texture.width(), texture.height());
        let texel = texture.format().block_copy_size(None).ok_or_else(|| {
            ProgramError::Readback(format!("{:?} cannot be copied", texture.format()))
        })?;
        let row = width * texel;
        let padded_row = row.next_multiple_of(ROW_ALIGNMENT);
        let staged = self.read_staged(
            u64::from(padded_row) * u64::from(height),
            |encoder, staging| {
                encoder.copy_texture_to_buffer(
                    texture.as_image_copy(),
                    wgpu::TexelCopyBufferInfo {
                        buffer: staging,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(padded_row),
                            rows_per_image: Some(height),
                        },
                    },
                    texture.size(),
                );
            },
        )?;
        let bytes = staged
            .chunks_exact(padded_row as usize)
            .flat_map(|line| &line[..row as usize])
            .copied()
            .collect();
        Ok(TargetImage {
            size: [width, height],
            format: texture.format(),
            bytes,
        })
    }

    /// Runs `copy` into a mappable staging buffer of `size` bytes and returns its contents.
    fn read_staged(
        &self,
        size: u64,
        copy: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::Buffer),
    ) -> Result<Vec<u8>, ProgramError> {
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("render-program readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render-program readback"),
            });
        copy(&mut encoder, &staging);
        self.guarded(|| self.queue.submit([encoder.finish()]))?;
        let (sender, receiver) = mpsc::channel();
        staging
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                // The receiver outlives the poll below; a send can only fail after a timeout.
                let _ = sender.send(result);
            });
        self.poll_device()?;
        receiver
            .recv()
            .map_err(|error| ProgramError::Readback(error.to_string()))?
            .map_err(|error| ProgramError::Readback(error.to_string()))?;
        let bytes = staging
            .slice(..)
            .get_mapped_range()
            .map_err(|error| ProgramError::Readback(error.to_string()))?
            .to_vec();
        staging.unmap();
        Ok(bytes)
    }
}
