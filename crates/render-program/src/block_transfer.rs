//! Block-image transfers: encoded blocks that shaders wrote into an uncompressed storage image,
//! one block per texel, moved into a block-compressed texture.
//!
//! wgpu copies only between textures of one format, so the bytes go texture to buffer to
//! texture through a staging buffer the runtime owns and reuses. Within one encoder the two
//! copies of a transfer run before the next transfer's, and queue submissions run in order, so
//! one buffer serves every transfer.

mod plan;
#[cfg(test)]
mod tests;

use std::sync::{Arc, PoisonError};

use worldrunner_render_gpu::configuration::AllocationSet;

use crate::budget::{Note, Reservation};
use crate::error::ProgramError;
use crate::resources::TextureHandle;
use crate::runtime::ProgramRuntime;
use plan::{Facts, Plan, Region, plan};

/// The region of the source image a transfer reads: one texel for each block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockSource {
    pub texture: TextureHandle,
    pub mip: u32,
    pub layer: u32,
    /// The first texel read, in texels of the source mip.
    pub origin: [u32; 2],
    /// The texels read: the block columns and rows of the destination extent.
    pub extent: [u32; 2],
}

impl BlockSource {
    fn region(&self) -> Region {
        Region {
            mip: self.mip,
            layer: self.layer,
            origin: self.origin,
            extent: self.extent,
        }
    }
}

/// The block-compressed image a transfer fills, from the first block of its mip and layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockDestination {
    pub texture: TextureHandle,
    pub mip: u32,
    pub layer: u32,
    /// The logical texels the blocks describe, which may end inside the last block.
    pub extent: [u32; 2],
}

impl BlockDestination {
    fn region(&self) -> Region {
        Region {
            mip: self.mip,
            layer: self.layer,
            origin: [0, 0],
            extent: self.extent,
        }
    }
}

/// Block bytes moved from an `Rg32Uint` or `Rgba32Uint` image into a block-compressed texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockImageTransfer {
    pub source: BlockSource,
    pub destination: BlockDestination,
}

/// The runtime's one staging buffer for transfers, and the reservation that counts it.
pub(crate) struct Staging {
    buffer: wgpu::Buffer,
    size: u64,
    reservation: Arc<Reservation>,
}

impl ProgramRuntime {
    /// Records a block-image transfer into `encoder`: a copy of the source region into the
    /// staging buffer and a copy of the staging buffer into the destination mip and layer.
    /// The bytes are not converted. The frame retains the reservations of both textures and of
    /// the staging buffer, so their bytes stay counted until the frame is cancelled, dropped or
    /// completed on the GPU.
    ///
    /// The staging buffer is made by the first transfer, and again only when a transfer needs
    /// more room than it has, so repeated transfers allocate nothing.
    ///
    /// # Errors
    /// Fails with `ProgramError::BlockTransfer` naming the rule a transfer breaks, with
    /// `UnknownHandle` for a texture the runtime does not hold, with `OverBudget` when the
    /// staging buffer does not fit, and when the encoder is not the authority's. Nothing is
    /// recorded in each case.
    pub fn transfer_block_image(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        request: &BlockImageTransfer,
    ) -> Result<(), ProgramError> {
        self.recording(encoder)?;
        let source = self.store.texture(request.source.texture)?.texture.clone();
        let destination = self
            .store
            .texture(request.destination.texture)?
            .texture
            .clone();
        let plan = plan(
            &Facts::of(&source),
            &request.source.region(),
            &Facts::of(&destination),
            &request.destination.region(),
            self.device.features(),
        )?;
        let (staging, counted) = self.transfer_staging(plan.staging_bytes)?;
        self.hold_for_frame(
            encoder,
            vec![
                self.store.texture_reservation(request.source.texture)?,
                self.store
                    .texture_reservation(request.destination.texture)?,
                counted,
            ],
        )?;
        record_copies(encoder, &source, &destination, &staging, request, &plan);
        Ok(())
    }

    /// The staging buffer with room for `size` bytes, made when there is none or it is too
    /// small, and the reservation that counts it.
    fn transfer_staging(
        &self,
        size: u64,
    ) -> Result<(wgpu::Buffer, Arc<Reservation>), ProgramError> {
        let mut held = self
            .block_staging
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(staging) = held.as_ref().filter(|staging| staging.size >= size) {
            return Ok((staging.buffer.clone(), staging.reservation.clone()));
        }
        // Only the runtime holds an idle buffer's reservation: no frame that recorded a copy
        // through it is alive, so it can give its bytes back before the larger one is admitted.
        if held
            .as_ref()
            .is_some_and(|staging| Arc::strong_count(&staging.reservation) == 1)
        {
            *held = None;
        }
        let mut allocations = AllocationSet::default();
        allocations.buffer("render-program block transfer", size, 1);
        let reservation =
            Arc::new(self.admit(allocations, Note::new("block transfer staging", ""))?);
        let buffer = self.guarded(|| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("render-program block transfer"),
                size,
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        })?;
        self.counters.transfer_buffer_built();
        *held = Some(Staging {
            buffer: buffer.clone(),
            size,
            reservation: reservation.clone(),
        });
        Ok((buffer, reservation))
    }
}

fn record_copies(
    encoder: &mut wgpu::CommandEncoder,
    source: &wgpu::Texture,
    destination: &wgpu::Texture,
    staging: &wgpu::Buffer,
    request: &BlockImageTransfer,
    plan: &Plan,
) {
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(plan.bytes_per_row),
        rows_per_image: Some(plan.rows_per_image),
    };
    let from = &request.source;
    let to = &request.destination;
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: source,
            mip_level: from.mip,
            origin: wgpu::Origin3d {
                x: from.origin[0],
                y: from.origin[1],
                z: from.layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: staging,
            layout,
        },
        plan.read,
    );
    encoder.copy_buffer_to_texture(
        wgpu::TexelCopyBufferInfo {
            buffer: staging,
            layout,
        },
        wgpu::TexelCopyTextureInfo {
            texture: destination,
            mip_level: to.mip,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: to.layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        plan.write,
    );
}
