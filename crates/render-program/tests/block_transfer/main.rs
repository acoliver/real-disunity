//! Block-image transfers: block-compressed data that a game's shaders encode into an
//! uncompressed storage image, one block for each texel, moves into a block-compressed texture
//! array. Every block is synthetic: its endpoints decode to exact colours, so the expected bytes
//! and the expected sampled texels follow from how the test builds the block.

mod command_buffers;
mod common;
mod encoded;
mod mips;
mod prepared;
mod refusals;
mod reuse;
mod shapes;
