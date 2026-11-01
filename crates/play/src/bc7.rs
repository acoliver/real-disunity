//! BC7 (BPTC) blocks decoded to RGBA8, following the Khronos Data Format specification's BPTC
//! section: eight modes, up to three subsets chosen by a partition pattern, endpoints with shared
//! or per-endpoint P-bits, a channel rotation and an optional swap of which index drives colour.
use crate::bc7_tables::{
    ANCHOR_THREE_SECOND, ANCHOR_THREE_THIRD, ANCHOR_TWO, THREE_SUBSETS, TWO_SUBSETS,
};
use crate::block_texture::low_byte;

const TEXELS: usize = 16;

/// How one mode lays out its block.
struct Mode {
    subsets: usize,
    partition_bits: u32,
    rotation_bits: u32,
    index_select: bool,
    color_bits: u32,
    alpha_bits: u32,
    endpoint_pbit: bool,
    shared_pbit: bool,
    index_bits: u32,
    /// The width of the second index set, or zero when the mode has one.
    alpha_index_bits: u32,
}

const fn mode(
    subsets: usize,
    partition_bits: u32,
    rotation_bits: u32,
    color_bits: u32,
    alpha_bits: u32,
    index_bits: u32,
) -> Mode {
    Mode {
        subsets,
        partition_bits,
        rotation_bits,
        index_select: false,
        color_bits,
        alpha_bits,
        endpoint_pbit: false,
        shared_pbit: false,
        index_bits,
        alpha_index_bits: 0,
    }
}

const MODES: [Mode; 8] = [
    Mode {
        endpoint_pbit: true,
        ..mode(3, 4, 0, 4, 0, 3)
    },
    Mode {
        shared_pbit: true,
        ..mode(2, 6, 0, 6, 0, 3)
    },
    mode(3, 6, 0, 5, 0, 2),
    Mode {
        endpoint_pbit: true,
        ..mode(2, 6, 0, 7, 0, 2)
    },
    Mode {
        index_select: true,
        alpha_index_bits: 3,
        ..mode(1, 0, 2, 5, 6, 2)
    },
    Mode {
        alpha_index_bits: 2,
        ..mode(1, 0, 2, 7, 8, 2)
    },
    Mode {
        endpoint_pbit: true,
        ..mode(1, 0, 0, 7, 7, 4)
    },
    Mode {
        endpoint_pbit: true,
        ..mode(2, 6, 0, 5, 5, 2)
    },
];

const WEIGHTS_2: [u32; 4] = [0, 21, 43, 64];
const WEIGHTS_3: [u32; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS_4: [u32; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];

/// Reads little-endian bit fields from a 128-bit block, lowest bit first.
struct Bits {
    value: u128,
    position: u32,
}

impl Bits {
    fn new(block: &[u8]) -> Self {
        let mut bytes = [0_u8; 16];
        bytes.copy_from_slice(&block[..16]);
        Self {
            value: u128::from_le_bytes(bytes),
            position: 0,
        }
    }

    fn take(&mut self, count: u32) -> u32 {
        let field = (self.value >> self.position) & ((1_u128 << count) - 1);
        self.position += count;
        let [a, b, c, d, ..] = field.to_le_bytes();
        u32::from_le_bytes([a, b, c, d])
    }
}

/// Decodes one 16-byte block to its sixteen texels in raster order. A block whose mode bits are
/// all zero is reserved and decodes to transparent black.
#[must_use]
pub fn decode_block(block: &[u8]) -> [[u8; 4]; TEXELS] {
    let mut bits = Bits::new(block);
    let number = (bits.value & 0xFF).trailing_zeros();
    let Some(mode) = MODES.get(number as usize) else {
        return [[0; 4]; TEXELS];
    };
    bits.position = number + 1;
    let partition = bits.take(mode.partition_bits) as usize;
    let rotation = bits.take(mode.rotation_bits);
    let select = mode.index_select && bits.take(1) == 1;
    let endpoints = read_endpoints(&mut bits, mode);
    let subsets = subset_map(mode.subsets, partition);
    let anchors = anchor_map(mode.subsets, partition);
    let primary = read_indices(&mut bits, mode.index_bits, &anchors);
    let secondary = (mode.alpha_index_bits > 0)
        .then(|| read_indices(&mut bits, mode.alpha_index_bits, &anchors));
    std::array::from_fn(|texel| {
        let [low, high] = [2 * subsets[texel], 2 * subsets[texel] + 1].map(|e| endpoints[e]);
        let primary_index = (primary[texel], mode.index_bits);
        let (color, alpha) = match &secondary {
            None => (primary_index, primary_index),
            Some(second) => {
                let second = (second[texel], mode.alpha_index_bits);
                if select {
                    (second, primary_index)
                } else {
                    (primary_index, second)
                }
            }
        };
        let mut rgba: [u8; 4] = std::array::from_fn(|channel| {
            let (index, width) = if channel == 3 { alpha } else { color };
            interpolate(low[channel], high[channel], weight(width, index))
        });
        if rotation > 0 {
            rgba.swap(3, rotation as usize - 1);
        }
        rgba
    })
}

/// The expanded 8-bit endpoints, two per subset.
fn read_endpoints(bits: &mut Bits, mode: &Mode) -> [[u8; 4]; 6] {
    let count = mode.subsets * 2;
    let mut raw = [[0_u32; 4]; 6];
    for channel in 0..3 {
        for endpoint in raw.iter_mut().take(count) {
            endpoint[channel] = bits.take(mode.color_bits);
        }
    }
    if mode.alpha_bits > 0 {
        for endpoint in raw.iter_mut().take(count) {
            endpoint[3] = bits.take(mode.alpha_bits);
        }
    }
    let mut pbits = [0_u32; 6];
    if mode.endpoint_pbit {
        for pbit in pbits.iter_mut().take(count) {
            *pbit = bits.take(1);
        }
    } else if mode.shared_pbit {
        for subset in 0..mode.subsets {
            let shared = bits.take(1);
            pbits[2 * subset..=2 * subset + 1].fill(shared);
        }
    }
    let with_pbit = mode.endpoint_pbit || mode.shared_pbit;
    std::array::from_fn(|endpoint| {
        std::array::from_fn(|channel| {
            let width = if channel == 3 {
                mode.alpha_bits
            } else {
                mode.color_bits
            };
            if width == 0 {
                return 255;
            }
            if with_pbit {
                expand((raw[endpoint][channel] << 1) | pbits[endpoint], width + 1)
            } else {
                expand(raw[endpoint][channel], width)
            }
        })
    })
}

/// Widens a `width`-bit value to 8 bits by repeating its high bits.
fn expand(value: u32, width: u32) -> u8 {
    low_byte((value << (8 - width)) | (value >> (2 * width - 8)))
}

fn subset_map(subsets: usize, partition: usize) -> [usize; TEXELS] {
    let pattern = match subsets {
        2 => TWO_SUBSETS[partition],
        3 => THREE_SUBSETS[partition],
        _ => 0,
    };
    std::array::from_fn(|texel| ((pattern >> (2 * texel)) & 3) as usize)
}

/// The texels whose index drops its top bit, which is implied zero.
fn anchor_map(subsets: usize, partition: usize) -> [bool; TEXELS] {
    std::array::from_fn(|texel| {
        texel == 0
            || (subsets == 2 && texel == usize::from(ANCHOR_TWO[partition]))
            || (subsets == 3
                && (texel == usize::from(ANCHOR_THREE_SECOND[partition])
                    || texel == usize::from(ANCHOR_THREE_THIRD[partition])))
    })
}

fn read_indices(bits: &mut Bits, width: u32, anchors: &[bool; TEXELS]) -> [u32; TEXELS] {
    std::array::from_fn(|texel| bits.take(width - u32::from(anchors[texel])))
}

fn weight(width: u32, index: u32) -> u32 {
    match width {
        2 => WEIGHTS_2[index as usize],
        3 => WEIGHTS_3[index as usize],
        _ => WEIGHTS_4[index as usize],
    }
}

fn interpolate(low: u8, high: u8, weight: u32) -> u8 {
    low_byte(((64 - weight) * u32::from(low) + weight * u32::from(high) + 32) >> 6)
}
