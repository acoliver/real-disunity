//! Block-compressed player textures as DX10 DDS containers with their rows turned top first, or
//! decoded to RGBA8 for a device that cannot upload block formats.
//!
//! The managed engine stores a texture's rows bottom first; the presentation's sprite UVs count
//! rows from the top. Turning a block-compressed image over reverses its block rows and the four
//! texel rows inside each block.

const BLOCK_TEXELS: usize = 4;
const DDS_MAGIC: &[u8; 4] = b"DDS ";
const HEADER_BYTES: u32 = 124;
/// Caps, height, width, pixel format and linear size are all present.
const HEADER_FLAGS: u32 = 0x1 | 0x2 | 0x4 | 0x1000 | 0x8_0000;
const PIXEL_FORMAT_BYTES: u32 = 32;
const PIXEL_FORMAT_FOURCC: u32 = 0x4;
const CAPS_TEXTURE: u32 = 0x1000;
const RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;

/// The block layouts a player texture can have that the renderer draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockLayout {
    /// Eight bytes: two colour endpoints and sixteen 2-bit indices.
    Bc1,
    /// Sixteen bytes: an alpha block, then a `Bc1` colour block.
    Bc3,
    /// Eight bytes: two endpoints and sixteen 3-bit indices.
    Bc4,
    /// Sixteen bytes in one of eight modes; only decoded, never carried as a container.
    Bc7,
}

impl BlockLayout {
    /// The bytes of one 4 by 4 block.
    #[must_use]
    pub const fn block_bytes(self) -> usize {
        match self {
            Self::Bc1 | Self::Bc4 => 8,
            Self::Bc3 | Self::Bc7 => 16,
        }
    }

    /// The DXGI format code of the layout, in its sRGB form when `srgb` is set. `Bc4` holds data
    /// and has no sRGB form.
    #[must_use]
    pub const fn dxgi(self, srgb: bool) -> u32 {
        match (self, srgb) {
            (Self::Bc1, false) => 71,
            (Self::Bc1, true) => 72,
            (Self::Bc3, false) => 77,
            (Self::Bc3, true) => 78,
            (Self::Bc4, _) => 80,
            (Self::Bc7, false) => 98,
            (Self::Bc7, true) => 99,
        }
    }

    fn flip_block(self, block: &mut [u8]) {
        match self {
            Self::Bc1 => flip_colour_indices(block),
            Self::Bc4 => flip_alpha_indices(block),
            // Turning a BC7 block over would re-encode its indices per mode and partition; BC7 is
            // only drawn decoded.
            Self::Bc7 => {}
            Self::Bc3 => {
                let (alpha, colour) = block.split_at_mut(8);
                flip_alpha_indices(alpha);
                flip_colour_indices(colour);
            }
        }
    }
}

/// The bytes of a `width` by `height` base level.
#[must_use]
pub fn level_bytes(layout: BlockLayout, width: u32, height: u32) -> usize {
    let blocks = |texels: u32| (texels as usize).div_ceil(BLOCK_TEXELS);
    blocks(width) * blocks(height) * layout.block_bytes()
}

/// Reverses the rows of a base level whose height is a multiple of four.
#[must_use]
pub fn flip_rows(layout: BlockLayout, width: u32, level: &[u8]) -> Vec<u8> {
    let row_bytes = (width as usize).div_ceil(BLOCK_TEXELS) * layout.block_bytes();
    let mut flipped = Vec::with_capacity(level.len());
    for row in level.chunks_exact(row_bytes).rev() {
        flipped.extend_from_slice(row);
    }
    for block in flipped.chunks_exact_mut(layout.block_bytes()) {
        layout.flip_block(block);
    }
    flipped
}

/// The four index bytes after the two endpoints each hold one texel row.
fn flip_colour_indices(block: &mut [u8]) {
    block[4..8].reverse();
}

/// After two endpoint bytes, sixteen 3-bit indices fill 48 bits, twelve bits a texel row.
fn flip_alpha_indices(block: &mut [u8]) {
    let mut bits = 0_u64;
    for (index, byte) in block[2..8].iter().enumerate() {
        bits |= u64::from(*byte) << (8 * index);
    }
    let mut flipped = 0_u64;
    for row in 0..4 {
        flipped |= ((bits >> (12 * row)) & 0xFFF) << (12 * (3 - row));
    }
    for (index, byte) in block[2..8].iter_mut().enumerate() {
        *byte = (flipped >> (8 * index)) as u8;
    }
}

/// A one-level DX10 DDS container around `payload`.
#[must_use]
pub fn dx10_dds(width: u32, height: u32, dxgi: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(148 + payload.len());
    bytes.extend_from_slice(DDS_MAGIC);
    let linear_size = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    // Offsets 4 to 127: size, flags, height, width, linear size, depth, mips, 11 reserved words.
    let mut header = [0_u32; 31];
    header[0] = HEADER_BYTES;
    header[1] = HEADER_FLAGS;
    header[2] = height;
    header[3] = width;
    header[4] = linear_size;
    header[6] = 1;
    header[18] = PIXEL_FORMAT_BYTES;
    header[19] = PIXEL_FORMAT_FOURCC;
    header[20] = u32::from_le_bytes(*b"DX10");
    header[26] = CAPS_TEXTURE;
    for word in header {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    // The DX10 extension: format, 2D resource, no misc flags, one array slice, unknown alpha mode.
    for word in [dxgi, RESOURCE_DIMENSION_TEXTURE2D, 0, 1, 0] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(payload);
    bytes
}

/// Decodes a base level to RGBA8 rows in the order the level stores them. `Bc4` fills red and
/// leaves green and blue zero, as sampling a one-channel format does.
#[must_use]
pub fn decode_level(layout: BlockLayout, width: u32, height: u32, level: &[u8]) -> Vec<u8> {
    let (width, height) = (width as usize, height as usize);
    let blocks_wide = width.div_ceil(BLOCK_TEXELS);
    let mut rgba = vec![0_u8; width * height * 4];
    for (index, block) in level.chunks_exact(layout.block_bytes()).enumerate() {
        let origin = [
            (index % blocks_wide) * BLOCK_TEXELS,
            (index / blocks_wide) * BLOCK_TEXELS,
        ];
        let texels = decode_block(layout, block);
        for (texel, value) in texels.iter().enumerate() {
            let (x, y) = (origin[0] + texel % 4, origin[1] + texel / 4);
            if x < width && y < height {
                rgba[(y * width + x) * 4..][..4].copy_from_slice(value);
            }
        }
    }
    rgba
}

fn decode_block(layout: BlockLayout, block: &[u8]) -> [[u8; 4]; 16] {
    match layout {
        BlockLayout::Bc1 => decode_colour(block, true),
        BlockLayout::Bc3 => {
            let mut texels = decode_colour(&block[8..], false);
            for (texel, alpha) in texels.iter_mut().zip(decode_alpha(&block[..8])) {
                texel[3] = alpha;
            }
            texels
        }
        BlockLayout::Bc4 => decode_alpha(block).map(|red| [red, 0, 0, 255]),
        BlockLayout::Bc7 => crate::bc7::decode_block(block),
    }
}

fn expand_565(value: u16) -> [u8; 3] {
    let (r, g, b) = ((value >> 11) & 31, (value >> 5) & 63, value & 31);
    [
        ((r << 3) | (r >> 2)) as u8,
        ((g << 2) | (g >> 4)) as u8,
        ((b << 3) | (b >> 2)) as u8,
    ]
}

/// A colour block: two 5:6:5 endpoints and sixteen 2-bit indices. With `punch_through` set, an
/// endpoint order of `c0 <= c1` selects the three-colour mode whose fourth entry is transparent.
fn decode_colour(block: &[u8], punch_through: bool) -> [[u8; 4]; 16] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let (a, b) = (expand_565(c0), expand_565(c1));
    let mix = |wa: u16, wb: u16, total: u16| {
        [0, 1, 2].map(|i| ((wa * u16::from(a[i]) + wb * u16::from(b[i])) / total) as u8)
    };
    let opaque = |[r, g, bl]: [u8; 3]| [r, g, bl, 255];
    let palette = if c0 > c1 || !punch_through {
        [
            opaque(a),
            opaque(b),
            opaque(mix(2, 1, 3)),
            opaque(mix(1, 2, 3)),
        ]
    } else {
        [opaque(a), opaque(b), opaque(mix(1, 1, 2)), [0, 0, 0, 0]]
    };
    let indices = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    std::array::from_fn(|texel| palette[((indices >> (2 * texel)) & 3) as usize])
}

/// An alpha block: two endpoints and sixteen 3-bit indices.
fn decode_alpha(block: &[u8]) -> [u8; 16] {
    let (a0, a1) = (u16::from(block[0]), u16::from(block[1]));
    let palette: [u8; 8] = if a0 > a1 {
        std::array::from_fn(|i| match i {
            0 => a0 as u8,
            1 => a1 as u8,
            _ => (((8 - i as u16) * a0 + (i as u16 - 1) * a1) / 7) as u8,
        })
    } else {
        std::array::from_fn(|i| match i {
            0 => a0 as u8,
            1 => a1 as u8,
            6 => 0,
            7 => 255,
            _ => (((6 - i as u16) * a0 + (i as u16 - 1) * a1) / 5) as u8,
        })
    };
    let mut bits = 0_u64;
    for (index, byte) in block[2..8].iter().enumerate() {
        bits |= u64::from(*byte) << (8 * index);
    }
    std::array::from_fn(|texel| palette[((bits >> (3 * texel)) & 7) as usize])
}
