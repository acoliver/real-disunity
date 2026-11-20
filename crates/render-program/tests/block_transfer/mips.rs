//! Prepared transfers into the mips of a lent block-compressed texture with a chain: each mip
//! takes its own blocks, a mip smaller than a block takes one rounded-up block, and a mip past
//! the chain or an extent that is not the mip's blocks is refused by name before anything is
//! recorded or counted.

use wgpu::TextureFormat;
use worldrunner_render_post::{PreparedCommands, PreparedWrite};
use worldrunner_render_program::error::{BlockTransferError, ProgramError, TransferSide};
use worldrunner_render_program::runtime::{BlockSource, LentBlockDestination, TextureHandle};

use crate::common::{Gpu, dispatch_writer, read_texture_blocks, submit, texture};
use crate::encoded::bc4_blocks;

const GRID: [u32; 2] = [2, 2];
const EXTENT: [u32; 2] = [8, 8];
const MIPS: u32 = 3;

/// A one-layer BC4 texture of three mips (8, 4 and 2 texels a side) the runtime does not hold.
fn chain(gpu: &Gpu) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("lent chain"),
        size: wgpu::Extent3d {
            width: EXTENT[0],
            height: EXTENT[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: MIPS,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TextureFormat::Bc4RUnorm,
        usage: wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn region(texture: TextureHandle, origin: [u32; 2], extent: [u32; 2]) -> BlockSource {
    BlockSource {
        texture,
        mip: 0,
        layer: 0,
        origin,
        extent,
    }
}

fn into(texture: &wgpu::Texture, mip: u32, extent: [u32; 2]) -> LentBlockDestination {
    LentBlockDestination {
        texture: texture.clone(),
        mip,
        layer: 0,
        extent,
    }
}

#[test]
fn prepared_lent_transfers_fill_each_mip_of_a_chain_with_its_own_blocks() {
    let Some(gpu) = Gpu::new() else { return };
    let blocks: Vec<Vec<u8>> = bc4_blocks().iter().map(|block| block.bytes()).collect();
    let mut runtime = gpu.runtime();
    let scratch = texture(&mut runtime, TextureFormat::Rg32Uint, GRID, 1, 1);
    let destination = chain(&gpu);
    // Mip 0 takes all four blocks, mip 1 (one block) the second and mip 2 (two texels a side,
    // one rounded-up block) the third.
    let transfers = [
        (region(scratch, [0, 0], GRID), into(&destination, 0, EXTENT)),
        (
            region(scratch, [1, 0], [1, 1]),
            into(&destination, 1, [4, 4]),
        ),
        (
            region(scratch, [0, 1], [1, 1]),
            into(&destination, 2, [2, 2]),
        ),
    ];
    let (readbacks, index) = submit(&mut runtime, |runtime, encoder| {
        dispatch_writer(runtime, encoder, scratch, GRID, &blocks)?;
        for (mip, (source, target)) in (0..).zip(&transfers) {
            let moved = runtime.prepare_lent_block_transfer(source, target, &[])?;
            assert_eq!(
                PreparedCommands::writes(&moved),
                [PreparedWrite::layer(&destination, mip, 0)],
                "a transfer into mip {mip} writes that mip"
            );
            moved.retain(encoder)?;
            moved.record(encoder);
        }
        Ok([GRID, [1, 1], [1, 1]]
            .into_iter()
            .zip(0..)
            .map(|(grid, mip)| {
                read_texture_blocks(&gpu.device, encoder, &destination, (mip, 0), grid)
            })
            .collect::<Vec<_>>())
    })
    .unwrap();
    let expected = [blocks.concat(), blocks[1].clone(), blocks[2].clone()];
    for (mip, (readback, expected)) in readbacks.into_iter().zip(expected).enumerate() {
        assert_eq!(
            readback.resolve(&gpu.device, index.clone()),
            expected,
            "mip {mip} holds its own blocks"
        );
    }
}

#[test]
fn a_prepared_lent_transfer_into_a_mip_past_the_chain_or_off_its_blocks_is_refused_by_name() {
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let scratch = texture(&mut runtime, TextureFormat::Rg32Uint, GRID, 1, 1);
    let destination = chain(&gpu);
    let before = runtime.live_bytes();
    let refusal = |source: BlockSource, target: LentBlockDestination| match runtime
        .prepare_lent_block_transfer(&source, &target, &[])
    {
        Err(ProgramError::BlockTransfer(error)) => error,
        Err(other) => panic!("expected a block transfer refusal, found {other}"),
        Ok(_) => panic!("the transfer was prepared"),
    };
    assert_eq!(
        refusal(
            region(scratch, [0, 0], [1, 1]),
            into(&destination, MIPS, [1, 1])
        ),
        BlockTransferError::MipOutOfRange {
            side: TransferSide::Destination,
            mip: MIPS,
            mips: MIPS,
        }
    );
    assert_eq!(
        refusal(region(scratch, [0, 0], GRID), into(&destination, 1, EXTENT)),
        BlockTransferError::ExtentPastMip {
            extent: EXTENT,
            mip: [4, 4],
        },
        "the base extent does not fit mip 1"
    );
    assert_eq!(
        refusal(region(scratch, [0, 0], GRID), into(&destination, 1, [4, 4])),
        BlockTransferError::BlockCountMismatch {
            blocks: [1, 1],
            region: GRID,
        },
        "mip 1 is one block"
    );
    assert_eq!(
        refusal(region(scratch, [0, 0], GRID), into(&destination, 2, [2, 2])),
        BlockTransferError::BlockCountMismatch {
            blocks: [1, 1],
            region: GRID,
        },
        "mip 2 is one rounded-up block"
    );
    assert_eq!(runtime.stats().transfer_buffers_built, 0);
    assert_eq!(runtime.live_bytes(), before);
}
