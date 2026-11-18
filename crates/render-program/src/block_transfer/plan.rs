//! What a block-image transfer checks and the geometry of its copies, from the textures' facts
//! alone, so every refusal is found before the device or the encoder is touched.

use wgpu::{Extent3d, Features, TextureDimension, TextureFormat, TextureUsages};

use crate::error::{BlockTransferError, TransferSide};

/// The part of a texture one side of a transfer reaches.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Region {
    pub(crate) mip: u32,
    pub(crate) layer: u32,
    pub(crate) origin: [u32; 2],
    pub(crate) extent: [u32; 2],
}

/// What a transfer needs to know about a texture.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Facts {
    pub(crate) format: TextureFormat,
    pub(crate) usage: TextureUsages,
    pub(crate) dimension: TextureDimension,
    pub(crate) samples: u32,
    pub(crate) size: Extent3d,
    pub(crate) mips: u32,
}

impl Facts {
    pub(crate) fn of(texture: &wgpu::Texture) -> Self {
        Self {
            format: texture.format(),
            usage: texture.usage(),
            dimension: texture.dimension(),
            samples: texture.sample_count(),
            size: texture.size(),
            mips: texture.mip_level_count(),
        }
    }

    /// The texels of mip `mip`, which a mip below the last never makes smaller than one.
    fn mip_size(&self, mip: u32) -> [u32; 2] {
        [
            (self.size.width >> mip).max(1),
            (self.size.height >> mip).max(1),
        ]
    }
}

/// The two copies of a transfer: texture to buffer in blocks, and buffer to texture in texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    /// The blocks read from the source, as texels of it.
    pub(crate) read: Extent3d,
    /// The destination's texels the blocks cover, which may reach past the logical extent to the
    /// end of the last block, inside the mip's physical size.
    pub(crate) write: Extent3d,
    /// The staging buffer's row pitch, padded to the alignment of buffer copies.
    pub(crate) bytes_per_row: u32,
    /// Block rows in each image of the buffer.
    pub(crate) rows_per_image: u32,
    /// The staging bytes the copies use.
    pub(crate) staging_bytes: u64,
}

/// Checks a transfer of `from` into `to` and lays out its copies.
///
/// # Errors
/// Fails with the first rule the transfer breaks, naming it.
pub(crate) fn plan(
    source: &Facts,
    from: &Region,
    destination: &Facts,
    to: &Region,
    features: Features,
) -> Result<Plan, BlockTransferError> {
    let texel_bytes = check_source(source)?;
    let block_bytes = check_destination(destination, features)?;
    if texel_bytes != block_bytes {
        return Err(BlockTransferError::BlockSizeMismatch {
            texel_bytes,
            block_bytes,
        });
    }
    check_subresource(TransferSide::Source, source, from.layer, from.mip)?;
    check_subresource(TransferSide::Destination, destination, to.layer, to.mip)?;
    let (block_width, block_height) = destination.format.block_dimensions();
    let blocks = [
        to.extent[0].div_ceil(block_width),
        to.extent[1].div_ceil(block_height),
    ];
    check_extents(source, from, destination, to, blocks)?;
    let bytes_per_row =
        (blocks[0] * block_bytes).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    Ok(Plan {
        read: Extent3d {
            width: blocks[0],
            height: blocks[1],
            depth_or_array_layers: 1,
        },
        write: Extent3d {
            width: blocks[0] * block_width,
            height: blocks[1] * block_height,
            depth_or_array_layers: 1,
        },
        bytes_per_row,
        rows_per_image: blocks[1],
        staging_bytes: u64::from(bytes_per_row) * u64::from(blocks[1]),
    })
}

/// The block bytes a source texel holds.
fn check_source(source: &Facts) -> Result<u32, BlockTransferError> {
    check_flat(TransferSide::Source, source)?;
    if !matches!(
        source.format,
        TextureFormat::Rg32Uint | TextureFormat::Rgba32Uint
    ) {
        return Err(BlockTransferError::SourceFormat {
            format: source.format,
        });
    }
    check_usage(TransferSide::Source, source, TextureUsages::COPY_SRC)?;
    Ok(source.format.block_copy_size(None).unwrap_or_default())
}

/// The block bytes the destination holds.
fn check_destination(destination: &Facts, features: Features) -> Result<u32, BlockTransferError> {
    check_flat(TransferSide::Destination, destination)?;
    let format = destination.format;
    if !format.is_compressed() {
        return Err(BlockTransferError::DestinationNotCompressed { format });
    }
    let missing = format.required_features().difference(features);
    if !missing.is_empty() {
        return Err(BlockTransferError::DeviceLacksFormat { format, missing });
    }
    check_usage(
        TransferSide::Destination,
        destination,
        TextureUsages::COPY_DST,
    )?;
    Ok(format.block_copy_size(None).unwrap_or_default())
}

fn check_flat(side: TransferSide, facts: &Facts) -> Result<(), BlockTransferError> {
    if facts.dimension != TextureDimension::D2 || facts.samples != 1 {
        return Err(BlockTransferError::NotFlat {
            side,
            dimension: facts.dimension,
            samples: facts.samples,
        });
    }
    Ok(())
}

fn check_usage(
    side: TransferSide,
    facts: &Facts,
    needed: TextureUsages,
) -> Result<(), BlockTransferError> {
    if !facts.usage.contains(needed) {
        return Err(BlockTransferError::Usage {
            side,
            usage: facts.usage,
            needed,
        });
    }
    Ok(())
}

fn check_subresource(
    side: TransferSide,
    facts: &Facts,
    layer: u32,
    mip: u32,
) -> Result<(), BlockTransferError> {
    let layers = facts.size.depth_or_array_layers;
    if layer >= layers {
        return Err(BlockTransferError::LayerOutOfRange {
            side,
            layer,
            layers,
        });
    }
    if mip >= facts.mips {
        return Err(BlockTransferError::MipOutOfRange {
            side,
            mip,
            mips: facts.mips,
        });
    }
    Ok(())
}

fn check_extents(
    source: &Facts,
    from: &Region,
    destination: &Facts,
    to: &Region,
    blocks: [u32; 2],
) -> Result<(), BlockTransferError> {
    if to.extent.contains(&0) || from.extent.contains(&0) {
        return Err(BlockTransferError::EmptyExtent);
    }
    let mip = destination.mip_size(to.mip);
    if to.extent[0] > mip[0] || to.extent[1] > mip[1] {
        return Err(BlockTransferError::ExtentPastMip {
            extent: to.extent,
            mip,
        });
    }
    if blocks != from.extent {
        return Err(BlockTransferError::BlockCountMismatch {
            blocks,
            region: from.extent,
        });
    }
    let mip = source.mip_size(from.mip);
    let end = [
        u64::from(from.origin[0]) + u64::from(from.extent[0]),
        u64::from(from.origin[1]) + u64::from(from.extent[1]),
    ];
    if end[0] > u64::from(mip[0]) || end[1] > u64::from(mip[1]) {
        return Err(BlockTransferError::SourceRegionPastMip {
            origin: from.origin,
            extent: from.extent,
            mip,
        });
    }
    Ok(())
}
