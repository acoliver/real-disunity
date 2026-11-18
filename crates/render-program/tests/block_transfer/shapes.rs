//! Short final mips, mips that are not a multiple of the block size, and region addressing.

use wgpu::TextureFormat;

use crate::common::{Gpu, read_blocks, submit, texture, transfer};
use crate::encoded::{Block, Place, assert_landed, bc4_blocks, bc5_blocks, run};

#[test]
fn a_mip_two_texels_across_takes_one_block_that_covers_it() {
    let Some(gpu) = Gpu::new() else { return };
    let place = Place {
        base: [8, 8],
        layers: 2,
        mips: 4,
        layer: 1,
        mip: 2,
        extent: [2, 2],
    };
    let blocks = &bc4_blocks()[1..2];
    assert_landed(&run(&gpu, blocks, &place), blocks, [2, 2]);
}

#[test]
fn a_mip_six_texels_across_takes_two_blocks_a_side() {
    let Some(gpu) = Gpu::new() else { return };
    let place = Place {
        base: [12, 12],
        layers: 2,
        mips: 2,
        layer: 1,
        mip: 1,
        extent: [6, 6],
    };
    let blocks = bc5_blocks();
    assert_landed(&run(&gpu, &blocks, &place), &blocks, [6, 6]);
}

#[test]
fn a_source_region_is_addressed_by_mip_layer_and_origin() {
    let Some(gpu) = Gpu::new() else { return };
    let mut runtime = gpu.runtime();
    let source = texture(&mut runtime, TextureFormat::Rg32Uint, [8, 8], 2, 2);
    let destination = texture(&mut runtime, TextureFormat::Bc4RUnorm, [8, 8], 1, 1);
    // Mip 1 of layer 1 is 4 by 4 blocks; the bytes of block (x, y) are its own coordinates.
    let level: Vec<Vec<u8>> = (0..16_u8)
        .map(|at| Block::Bc4(at % 4 + 1, at / 4 + 1).bytes())
        .collect();
    let texels: Vec<u8> = level.concat();
    runtime.authority().queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &runtime.gpu_texture(source).unwrap(),
            mip_level: 1,
            origin: wgpu::Origin3d { x: 0, y: 0, z: 1 },
            aspect: wgpu::TextureAspect::All,
        },
        &texels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(32),
            rows_per_image: Some(4),
        },
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
    );
    let request = transfer((source, 1, 1, [1, 2], [2, 2]), (destination, 0, 0, [8, 8]));
    let (read, index) = submit(&mut runtime, |runtime, encoder| {
        runtime.transfer_block_image(encoder, &request)?;
        Ok(read_blocks(runtime, encoder, destination, (0, 0), [2, 2]))
    })
    .unwrap();
    // The region starts at column 1 and row 2 and covers two blocks each way.
    let expected: Vec<u8> = [9, 10, 13, 14]
        .iter()
        .flat_map(|at| level[*at].clone())
        .collect();
    assert_eq!(read.resolve(&gpu.device, index), expected);
}
