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
use worldrunner_render_post::PreparedWrite;

use crate::budget::{Note, Reservation};
use crate::error::ProgramError;
use crate::resources::{Keepalive, Pins, TextureHandle};
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

/// The block-compressed texture a transfer fills when another owner holds it, from the first
/// block of its mip and layer.
#[derive(Clone, Debug)]
pub struct LentBlockDestination {
    pub texture: wgpu::Texture,
    pub mip: u32,
    pub layer: u32,
    /// The logical texels the blocks describe, which may end inside the last block.
    pub extent: [u32; 2],
}

/// A transfer whose plan was checked and whose staging was reserved. Recording it touches no
/// runtime state.
///
/// It holds the staging buffer it was planned against and shares of the reservations of the
/// buffer and of the runtime's textures it reads or writes, for as long as it is held and then
/// for every frame that [`retain`](Self::retain)ed it, until that frame is cancelled, dropped or
/// completed on the GPU. A larger transfer prepared later may give the runtime a new staging
/// buffer; this one keeps the one it was planned with.
pub struct PreparedBlockTransfer {
    pins: Pins,
    source: (wgpu::Texture, Region),
    destination: (wgpu::Texture, Region),
    staging: wgpu::Buffer,
    plan: Plan,
}

impl std::fmt::Debug for PreparedBlockTransfer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedBlockTransfer")
            .field("source", &self.source.1)
            .field("destination", &self.destination.1)
            .finish_non_exhaustive()
    }
}

impl PreparedBlockTransfer {
    /// Makes the frame `encoder` records hold the transfer's reservations and owners until the
    /// frame is cancelled, dropped or completed on the GPU. Call it for every frame the transfer
    /// is recorded into.
    ///
    /// # Errors
    /// Fails with `ForeignAuthority` when the frame belongs to another submission authority than
    /// the runtime's and when `encoder` is not a frame being recorded.
    pub fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ProgramError> {
        self.pins.retain(encoder)
    }

    /// The destination mip and layer, the one subresource the transfer writes besides its
    /// staging buffer.
    pub fn writes(&self) -> PreparedWrite {
        let (texture, region) = &self.destination;
        PreparedWrite::layer(texture, region.mip, region.layer)
    }

    /// Records the copy of the source region into the staging buffer and the copy of the staging
    /// buffer into the destination mip and layer, one after the other.
    pub fn record(&self, encoder: &mut wgpu::CommandEncoder) {
        record_copies(
            encoder,
            (&self.source.0, &self.source.1),
            (&self.destination.0, &self.destination.1),
            &self.staging,
            &self.plan,
        );
    }
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
        let prepared = self.prepare_block_transfer(request)?;
        prepared.retain(encoder)?;
        prepared.record(encoder);
        Ok(())
    }

    /// Prepares a block-image transfer for a frame that records it later, as
    /// [`transfer_block_image`](Self::transfer_block_image) would record it now. Everything that
    /// can fail is checked here, and the staging buffer is made or reused here.
    ///
    /// # Errors
    /// As [`transfer_block_image`](Self::transfer_block_image), less the encoder's.
    pub fn prepare_block_transfer(
        &self,
        request: &BlockImageTransfer,
    ) -> Result<PreparedBlockTransfer, ProgramError> {
        self.authority.check_device()?;
        let source = self.store.texture(request.source.texture)?.texture.clone();
        let destination = self
            .store
            .texture(request.destination.texture)?
            .texture
            .clone();
        let shares = vec![
            self.store.texture_reservation(request.source.texture)?,
            self.store
                .texture_reservation(request.destination.texture)?,
        ];
        self.prepare_transfer(
            (source, request.source.region()),
            (destination, request.destination.region()),
            (shares, Vec::new()),
        )
    }

    /// Prepares a transfer from a runtime texture into a block-compressed texture another owner
    /// holds and keeps counted. `owners` are held by the transfer and by every frame that retains
    /// it.
    ///
    /// # Errors
    /// As [`prepare_block_transfer`](Self::prepare_block_transfer).
    pub fn prepare_lent_block_transfer(
        &self,
        source: &BlockSource,
        destination: &LentBlockDestination,
        owners: &[Keepalive],
    ) -> Result<PreparedBlockTransfer, ProgramError> {
        self.authority.check_device()?;
        let texture = self.store.texture(source.texture)?.texture.clone();
        let shares = vec![self.store.texture_reservation(source.texture)?];
        let region = Region {
            mip: destination.mip,
            layer: destination.layer,
            origin: [0, 0],
            extent: destination.extent,
        };
        self.prepare_transfer(
            (texture, source.region()),
            (destination.texture.clone(), region),
            (shares, owners.to_vec()),
        )
    }

    /// Plans the copies between the two ends and reserves the staging they need, holding
    /// `shares` and `owners` with the staging buffer's reservation.
    fn prepare_transfer(
        &self,
        source: (wgpu::Texture, Region),
        destination: (wgpu::Texture, Region),
        (mut shares, owners): (Vec<Arc<Reservation>>, Vec<Keepalive>),
    ) -> Result<PreparedBlockTransfer, ProgramError> {
        let plan = plan(
            &Facts::of(&source.0),
            &source.1,
            &Facts::of(&destination.0),
            &destination.1,
            self.device.features(),
        )?;
        let (staging, counted) = self.transfer_staging(plan.staging_bytes)?;
        shares.push(counted);
        Ok(PreparedBlockTransfer {
            pins: Pins::borrowing(&self.authority, shares, owners),
            source,
            destination,
            staging,
            plan,
        })
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
            Arc::new(self.admit(&allocations, Note::new("block transfer staging", ""))?);
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
    (source, from): (&wgpu::Texture, &Region),
    (destination, to): (&wgpu::Texture, &Region),
    staging: &wgpu::Buffer,
    plan: &Plan,
) {
    let layout = wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(plan.bytes_per_row),
        rows_per_image: Some(plan.rows_per_image),
    };
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
                x: to.origin[0],
                y: to.origin[1],
                z: to.layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        plan.write,
    );
}
