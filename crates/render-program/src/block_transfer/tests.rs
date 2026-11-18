//! The rules and geometry a transfer applies to textures it cannot have here: a device without
//! block compression, textures that are not flat and sources that cannot be copied from.

use wgpu::{Extent3d, Features, TextureDimension, TextureFormat, TextureUsages};

use super::plan::{Facts, Plan, Region, plan};
use crate::error::{BlockTransferError, TransferSide};

fn facts(format: TextureFormat, size: [u32; 2], layers: u32, mips: u32) -> Facts {
    Facts {
        format,
        usage: TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
        dimension: TextureDimension::D2,
        samples: 1,
        size: Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: layers,
        },
        mips,
    }
}

fn region(mip: u32, extent: [u32; 2]) -> Region {
    Region {
        mip,
        layer: 0,
        origin: [0, 0],
        extent,
    }
}

fn bc() -> Features {
    Features::TEXTURE_COMPRESSION_BC
}

fn source() -> Facts {
    facts(TextureFormat::Rg32Uint, [4, 4], 1, 1)
}

fn bc1(size: u32, mips: u32) -> Facts {
    facts(TextureFormat::Bc1RgbaUnormSrgb, [size, size], 4, mips)
}

#[test]
fn a_device_without_block_compression_is_refused_by_name() {
    let error = plan(
        &source(),
        &region(0, [2, 2]),
        &bc1(8, 1),
        &region(0, [8, 8]),
        Features::empty(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        BlockTransferError::DeviceLacksFormat {
            format: TextureFormat::Bc1RgbaUnormSrgb,
            missing: bc(),
        }
    );
}

#[test]
fn a_source_that_cannot_be_copied_from_is_refused_by_name() {
    let mut no_copy = source();
    no_copy.usage = TextureUsages::STORAGE_BINDING;
    let error = plan(
        &no_copy,
        &region(0, [2, 2]),
        &bc1(8, 1),
        &region(0, [8, 8]),
        bc(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        BlockTransferError::Usage {
            side: TransferSide::Source,
            usage: TextureUsages::STORAGE_BINDING,
            needed: TextureUsages::COPY_SRC,
        }
    );
}

#[test]
fn a_destination_that_cannot_be_copied_to_is_refused_by_name() {
    let mut no_copy = bc1(8, 1);
    no_copy.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    let error = plan(
        &source(),
        &region(0, [2, 2]),
        &no_copy,
        &region(0, [8, 8]),
        bc(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        BlockTransferError::Usage {
            side: TransferSide::Destination,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC,
            needed: TextureUsages::COPY_DST,
        }
    );
}

#[test]
fn a_texture_that_is_not_single_sample_2d_is_refused_by_name() {
    let mut volume = source();
    volume.dimension = TextureDimension::D3;
    let error = plan(
        &volume,
        &region(0, [2, 2]),
        &bc1(8, 1),
        &region(0, [8, 8]),
        bc(),
    )
    .unwrap_err();
    assert_eq!(
        error,
        BlockTransferError::NotFlat {
            side: TransferSide::Source,
            dimension: TextureDimension::D3,
            samples: 1,
        }
    );
    let mut multisampled = bc1(8, 1);
    multisampled.samples = 4;
    let error = plan(
        &source(),
        &region(0, [2, 2]),
        &multisampled,
        &region(0, [8, 8]),
        bc(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        BlockTransferError::NotFlat {
            side: TransferSide::Destination,
            samples: 4,
            ..
        }
    ));
}

#[test]
fn an_extent_with_a_zero_side_is_refused_by_name() {
    let error = plan(
        &source(),
        &region(0, [0, 2]),
        &bc1(8, 1),
        &region(0, [0, 8]),
        bc(),
    )
    .unwrap_err();
    assert_eq!(error, BlockTransferError::EmptyExtent);
}

#[test]
fn rows_are_padded_to_the_copy_alignment_and_counted_in_blocks() {
    // 2 blocks of 8 bytes across, 2 block rows: the 16-byte row is padded to 256.
    let plan = plan(
        &source(),
        &region(0, [2, 2]),
        &bc1(8, 1),
        &region(0, [8, 8]),
        bc(),
    )
    .unwrap();
    assert_eq!(
        plan,
        Plan {
            read: Extent3d {
                width: 2,
                height: 2,
                depth_or_array_layers: 1
            },
            write: Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1
            },
            bytes_per_row: 256,
            rows_per_image: 2,
            staging_bytes: 512,
        }
    );
}

#[test]
fn a_short_mip_is_written_through_the_blocks_that_cover_it() {
    // A 2 by 2 mip of an 8 by 8 texture is one block, written as 4 by 4 texels of its physical
    // size; a 6 by 6 mip is two blocks across and down, written as 8 by 8.
    let two = plan(
        &source(),
        &region(0, [1, 1]),
        &bc1(8, 4),
        &region(2, [2, 2]),
        bc(),
    )
    .unwrap();
    assert_eq!((two.read.width, two.read.height), (1, 1));
    assert_eq!((two.write.width, two.write.height), (4, 4));
    let six = plan(
        &source(),
        &region(0, [2, 2]),
        &bc1(12, 2),
        &region(1, [6, 6]),
        bc(),
    )
    .unwrap();
    assert_eq!((six.read.width, six.read.height), (2, 2));
    assert_eq!((six.write.width, six.write.height), (8, 8));
}

#[test]
fn a_sixteen_byte_block_row_of_sixteen_blocks_needs_no_padding() {
    let wide = facts(TextureFormat::Rgba32Uint, [16, 1], 1, 1);
    let bc5 = facts(TextureFormat::Bc5RgUnorm, [64, 4], 1, 1);
    let plan = plan(&wide, &region(0, [16, 1]), &bc5, &region(0, [64, 4]), bc()).unwrap();
    assert_eq!((plan.bytes_per_row, plan.staging_bytes), (256, 256));
}
