//! Blocks that shaders encode and the compressed texture layer they must end up in.

use wgpu::TextureFormat;
use worldrunner_render_program::runtime::TargetImage;

use crate::common::{
    Gpu, dispatch_writer, pixel, read_blocks, sample, source_format, submit, texture, transfer,
};

const BLACK: [u8; 3] = [0, 0, 0];
const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [255, 0, 0];
const GREEN: [u8; 3] = [0, 255, 0];
const BLUE: [u8; 3] = [0, 0, 255];
const YELLOW: [u8; 3] = [255, 255, 0];
const CYAN: [u8; 3] = [0, 255, 255];

/// A block of one of the formats the transfers are observed to fill. Each uses only the two
/// endpoint values, alternating in a checkerboard, so every texel decodes to an endpoint exactly.
#[derive(Clone, Copy, Debug)]
pub enum Block {
    /// Two colours whose channels are 0 or 255, which five and six bit endpoints hold exactly.
    Bc1([u8; 3], [u8; 3]),
    /// Two red values.
    Bc4(u8, u8),
    /// Two red values and two green values.
    Bc5([u8; 2], [u8; 2]),
}

fn rgb565(rgb: [u8; 3]) -> u16 {
    (u16::from(rgb[0] >> 3) << 11) | (u16::from(rgb[1] >> 2) << 5) | u16::from(rgb[2] >> 3)
}

/// A BC4 channel block: both endpoints and 3-bit indices of 0 and 1 in a checkerboard.
fn channel_block(ends: [u8; 2]) -> Vec<u8> {
    let mut indices = 0_u64;
    for i in 0..16_u64 {
        indices |= (((i % 4) + (i / 4)) % 2) << (3 * i);
    }
    let mut bytes = ends.to_vec();
    bytes.extend_from_slice(&indices.to_le_bytes()[..6]);
    bytes
}

impl Block {
    pub fn format(self) -> TextureFormat {
        match self {
            Self::Bc1(..) => TextureFormat::Bc1RgbaUnormSrgb,
            Self::Bc4(..) => TextureFormat::Bc4RUnorm,
            Self::Bc5(..) => TextureFormat::Bc5RgUnorm,
        }
    }

    /// The block's bytes as a BC encoder writes them.
    pub fn bytes(self) -> Vec<u8> {
        match self {
            Self::Bc1(a, b) => {
                let mut bytes = rgb565(a).to_le_bytes().to_vec();
                bytes.extend_from_slice(&rgb565(b).to_le_bytes());
                // Two bits a texel, row 0 first: index 0 where x + y is even, 1 where it is odd.
                bytes.extend_from_slice(&[0x44, 0x11, 0x44, 0x11]);
                bytes
            }
            Self::Bc4(a, b) => channel_block([a, b]),
            Self::Bc5(r, g) => {
                let mut bytes = channel_block(r);
                bytes.extend(channel_block(g));
                bytes
            }
        }
    }

    /// The texel at `column` and `row` of the block, as a `Rgba8Unorm` draw reads it. Sampling
    /// decodes the sRGB endpoints to the same linear 0 and 1 the encoder meant.
    pub fn texel(self, column: u32, row: u32) -> [u8; 4] {
        let which = ((column + row) % 2) as usize;
        match self {
            Self::Bc1(even, odd) => {
                let [red, green, blue] = [even, odd][which];
                [red, green, blue, 255]
            }
            Self::Bc4(even, odd) => [[even, odd][which], 0, 0, 255],
            Self::Bc5(red, green) => [red[which], green[which], 0, 255],
        }
    }
}

/// The set of blocks used by the formats' tests, four to a format.
pub fn bc1_blocks() -> Vec<Block> {
    vec![
        Block::Bc1(RED, BLACK),
        Block::Bc1(GREEN, WHITE),
        Block::Bc1(BLUE, YELLOW),
        Block::Bc1(CYAN, BLACK),
    ]
}

pub fn bc4_blocks() -> Vec<Block> {
    vec![
        Block::Bc4(255, 0),
        Block::Bc4(128, 64),
        Block::Bc4(10, 250),
        Block::Bc4(77, 200),
    ]
}

pub fn bc5_blocks() -> Vec<Block> {
    vec![
        Block::Bc5([255, 0], [0, 255]),
        Block::Bc5([128, 64], [200, 100]),
        Block::Bc5([10, 250], [30, 90]),
        Block::Bc5([77, 200], [255, 1]),
    ]
}

/// Where a transfer goes: the destination array, the layer and mip and the logical extent.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub base: [u32; 2],
    pub layers: u32,
    pub mips: u32,
    pub layer: u32,
    pub mip: u32,
    pub extent: [u32; 2],
}

/// What the destination holds after the transfer.
pub struct Outcome {
    /// The blocks of the destination region, rows of blocks one after the other.
    pub bytes: Vec<u8>,
    /// The same region of the layer after the one transferred into.
    pub neighbour: Vec<u8>,
    /// The destination mip sampled over its logical extent.
    pub image: TargetImage,
}

/// Writes `blocks` into a source image from a kernel, transfers them into `place` and reads the
/// destination back as bytes and as sampled colours, all in one frame.
pub fn run(gpu: &Gpu, blocks: &[Block], place: &Place) -> Outcome {
    let columns = place.extent[0].div_ceil(4);
    let rows = place.extent[1].div_ceil(4);
    assert_eq!(blocks.len(), (columns * rows) as usize);
    let encoded: Vec<Vec<u8>> = blocks.iter().map(|block| block.bytes()).collect();
    let mut runtime = gpu.runtime();
    let source = texture(
        &mut runtime,
        source_format(encoded[0].len()),
        [columns, rows],
        1,
        1,
    );
    let destination = texture(
        &mut runtime,
        blocks[0].format(),
        place.base,
        place.layers,
        place.mips,
    );
    let request = transfer(
        (source, 0, 0, [0, 0], [columns, rows]),
        (destination, place.mip, place.layer, place.extent),
    );
    let neighbour = (place.layer + 1) % place.layers;
    let ((bytes, other, image), index) = submit(&mut runtime, |runtime, encoder| {
        dispatch_writer(runtime, encoder, source, [columns, rows], &encoded)?;
        runtime.transfer_block_image(encoder, &request)?;
        let bytes = read_blocks(
            runtime,
            encoder,
            destination,
            (place.mip, place.layer),
            [columns, rows],
        );
        let other = read_blocks(
            runtime,
            encoder,
            destination,
            (place.mip, neighbour),
            [columns, rows],
        );
        let image = sample(
            runtime,
            encoder,
            destination,
            (place.layer, place.mip),
            place.extent,
        )?;
        Ok((bytes, other, image))
    })
    .unwrap();
    Outcome {
        bytes: bytes.resolve(&gpu.device, index.clone()),
        neighbour: other.resolve(&gpu.device, index.clone()),
        image: image.resolve(index).unwrap(),
    }
}

/// Checks the bytes are the blocks', the layer beside is untouched and every sampled texel is
/// the colour its block encodes.
pub fn assert_landed(outcome: &Outcome, blocks: &[Block], extent: [u32; 2]) {
    let expected: Vec<u8> = blocks.iter().flat_map(|block| block.bytes()).collect();
    assert_eq!(outcome.bytes, expected, "block bytes");
    assert!(
        outcome.neighbour.iter().all(|byte| *byte == 0),
        "another layer changed"
    );
    assert_eq!(outcome.image.size, extent);
    let columns = extent[0].div_ceil(4);
    for y in 0..extent[1] {
        for x in 0..extent[0] {
            let block = blocks[(y / 4 * columns + x / 4) as usize];
            assert_eq!(
                pixel(&outcome.image, x, y),
                block.texel(x % 4, y % 4),
                "texel ({x}, {y})"
            );
        }
    }
}

const LAYER_TWO: Place = Place {
    base: [8, 8],
    layers: 4,
    mips: 1,
    layer: 2,
    mip: 0,
    extent: [8, 8],
};

#[test]
fn bc1_blocks_land_byte_exact_in_array_layer_two_and_sample_as_their_endpoints() {
    let Some(gpu) = Gpu::new() else { return };
    let blocks = bc1_blocks();
    assert_landed(&run(&gpu, &blocks, &LAYER_TWO), &blocks, [8, 8]);
}

#[test]
fn bc4_blocks_land_byte_exact_in_array_layer_two_and_sample_as_their_endpoints() {
    let Some(gpu) = Gpu::new() else { return };
    let blocks = bc4_blocks();
    assert_landed(&run(&gpu, &blocks, &LAYER_TWO), &blocks, [8, 8]);
}

#[test]
fn bc5_blocks_from_rgba32uint_land_byte_exact_in_array_layer_two_and_sample_as_their_endpoints() {
    let Some(gpu) = Gpu::new() else { return };
    let blocks = bc5_blocks();
    assert_landed(&run(&gpu, &blocks, &LAYER_TWO), &blocks, [8, 8]);
}
