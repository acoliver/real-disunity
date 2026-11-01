//! BC7 blocks built field by field from the format description, with the texels worked out by hand
//! from the interpolation weights (2-bit 0 21 43 64, 3-bit 0 9 18 27 37 46 55 64, 4-bit 0 4 9 13
//! 17 21 26 30 34 38 43 47 51 55 60 64) as (64 - w) * low + w * high + 32 >> 6 per channel.
use crate::bc7::decode_block;

/// 8-bit values of a black to white ramp, by index, for each index width.
const RAMP_2: [u8; 4] = [0, 84, 171, 255];
const RAMP_3: [u8; 8] = [0, 36, 72, 108, 147, 183, 219, 255];
const RAMP_4: [u8; 16] = [
    0, 16, 36, 52, 68, 84, 104, 120, 135, 151, 171, 187, 203, 219, 239, 255,
];

struct Writer {
    bits: u128,
    at: u32,
}

impl Writer {
    /// A block of the given mode: its unary prefix of zeros then a one.
    fn mode(mode: u32) -> Self {
        Self {
            bits: 1 << mode,
            at: mode + 1,
        }
    }

    fn put(&mut self, value: u32, width: u32) -> &mut Self {
        assert!(
            u64::from(value) < 1 << width,
            "{value} needs more than {width} bits"
        );
        self.bits |= u128::from(value) << self.at;
        self.at += width;
        self
    }

    /// One field per entry, each `width` bits.
    fn put_all(&mut self, values: &[u32], width: u32) -> &mut Self {
        for &value in values {
            self.put(value, width);
        }
        self
    }

    /// Sixteen indices of `width` bits, one bit fewer at the anchor texels.
    fn indices(&mut self, width: u32, anchors: &[usize], pick: impl Fn(usize) -> u32) -> &mut Self {
        for texel in 0..16 {
            let bits = width - u32::from(anchors.contains(&texel));
            self.put(pick(texel), bits);
        }
        self
    }

    fn block(&self) -> [u8; 16] {
        assert_eq!(self.at, 128, "a block is exactly 128 bits");
        self.bits.to_le_bytes()
    }
}

fn grey(value: u8) -> [u8; 4] {
    [value, value, value, value]
}

#[test]
fn mode_6_decodes_a_four_bit_ramp_with_p_bits_in_colour_and_alpha() {
    let build = |p1: u32, pick: fn(usize) -> u32| {
        let mut w = Writer::mode(6);
        w.put_all(&[0, 127, 0, 127, 0, 127, 0, 127], 7);
        w.put_all(&[0, p1], 1);
        w.indices(4, &[0], pick);
        w.block()
    };
    let ramp = decode_block(&build(1, |t| t as u32));
    for (texel, got) in ramp.iter().enumerate() {
        assert_eq!(*got, grey(RAMP_4[texel]), "texel {texel}");
    }
    let clear = decode_block(&build(0, |t| if t == 0 { 0 } else { 15 }));
    assert_eq!(
        clear[5],
        grey(254),
        "7 bits and a clear P-bit make 254, in alpha too"
    );
}

#[test]
fn mode_5_has_separate_colour_and_alpha_indices_and_four_rotations() {
    let build = |rotation: u32| {
        let mut w = Writer::mode(5);
        w.put(rotation, 2);
        w.put_all(&[0, 127], 7)
            .put_all(&[0, 0], 7)
            .put_all(&[0, 0], 7);
        w.put_all(&[0, 255], 8);
        w.indices(2, &[0], |t| t as u32 % 4);
        w.indices(2, &[0], |t| t as u32 / 4);
        w.block()
    };
    for (rotation, texel) in [(0, 6), (1, 6), (2, 6), (3, 6)] {
        let got = decode_block(&build(rotation))[texel];
        let (red, alpha) = (RAMP_2[texel % 4], RAMP_2[texel / 4]);
        let expected = match rotation {
            0 => [red, 0, 0, alpha],
            1 => [alpha, 0, 0, red],
            2 => [red, alpha, 0, 0],
            _ => [red, 0, alpha, 0],
        };
        assert_eq!(got, expected, "rotation {rotation}");
    }
}

#[test]
fn mode_4_index_selection_swaps_which_index_set_drives_colour() {
    let build = |select: u32| {
        let mut w = Writer::mode(4);
        w.put(0, 2).put(select, 1);
        w.put_all(&[0, 31, 0, 31, 0, 31], 5);
        w.put_all(&[0, 63], 6);
        w.indices(2, &[0], |t| t as u32 % 4);
        w.indices(3, &[0], |t| t as u32 % 8);
        w.block()
    };
    for texel in [1_usize, 6, 11, 14] {
        let narrow = RAMP_2[texel % 4];
        let wide = RAMP_3[texel % 8];
        assert_eq!(
            decode_block(&build(0))[texel],
            [narrow, narrow, narrow, wide]
        );
        assert_eq!(decode_block(&build(1))[texel], [wide, wide, wide, narrow]);
    }
}

#[test]
fn mode_1_picks_subsets_from_the_partition_and_shares_a_p_bit_per_subset() {
    let build = |p0: u32, p1: u32| {
        let mut w = Writer::mode(1);
        w.put(1, 6);
        w.put_all(&[0, 63, 0, 0], 6)
            .put_all(&[0, 0, 0, 0], 6)
            .put_all(&[0, 0, 0, 63], 6);
        w.put_all(&[p0, p1], 1);
        w.indices(3, &[0, 15], |t| if t == 0 || t == 15 { 3 } else { 7 });
        w.block()
    };
    let texels = decode_block(&build(1, 1));
    for (texel, got) in texels.iter().enumerate() {
        let expected = match (texel % 4 == 3, texel) {
            (_, 0) => [109, 2, 2, 255],
            (_, 15) => [2, 2, 109, 255],
            (false, _) => [255, 2, 2, 255],
            (true, _) => [2, 2, 255, 255],
        };
        assert_eq!(*got, expected, "texel {texel}");
    }
    assert_eq!(
        decode_block(&build(0, 1))[1],
        [253, 0, 0, 255],
        "6 bits plus a clear shared bit"
    );
}

#[test]
fn mode_0_has_three_subsets_with_a_four_bit_partition_and_p_bits_per_endpoint() {
    let mut w = Writer::mode(0);
    w.put(0, 4);
    w.put_all(&[0, 15, 0, 0, 0, 0], 4);
    w.put_all(&[0, 0, 0, 15, 0, 0], 4);
    w.put_all(&[0, 0, 0, 0, 0, 15], 4);
    w.put_all(&[0, 1, 0, 1, 0, 1], 1);
    w.indices(
        3,
        &[0, 3, 15],
        |t| if [0, 3, 15].contains(&t) { 3 } else { 7 },
    );
    let texels = decode_block(&w.block());
    let subsets = [0, 0, 1, 1, 0, 0, 1, 1, 0, 2, 2, 1, 2, 2, 2, 2];
    for (texel, got) in texels.iter().enumerate() {
        // A set P-bit lifts the other two channels of an endpoint from 0 to 8 (5 bits holding 1).
        let anchor = [0, 3, 15].contains(&texel);
        let mut expected = if anchor {
            [3, 3, 3, 255]
        } else {
            [8, 8, 8, 255]
        };
        expected[subsets[texel]] = if anchor { 108 } else { 255 };
        assert_eq!(*got, expected, "texel {texel}");
    }
}

#[test]
fn mode_2_has_three_subsets_with_six_partition_bits_and_five_bit_colours() {
    let mut w = Writer::mode(2);
    w.put(2, 6);
    w.put_all(&[0, 31, 0, 0, 0, 0], 5);
    w.put_all(&[0, 0, 0, 31, 0, 0], 5);
    w.put_all(&[0, 0, 0, 0, 0, 31], 5);
    let anchors = [0, 15, 8];
    w.indices(2, &anchors, |t| if anchors.contains(&t) { 1 } else { 3 });
    let texels = decode_block(&w.block());
    let subsets = [0, 0, 0, 0, 2, 0, 0, 1, 2, 2, 1, 1, 2, 2, 1, 1];
    for (texel, got) in texels.iter().enumerate() {
        let channel = subsets[texel];
        let level = if anchors.contains(&texel) { 84 } else { 255 };
        let mut expected = [0, 0, 0, 255];
        expected[channel] = level;
        assert_eq!(*got, expected, "texel {texel}");
    }
}

#[test]
fn mode_3_has_seven_bit_colours_with_a_p_bit_per_endpoint() {
    let mut w = Writer::mode(3);
    w.put(0, 6);
    w.put_all(&[0, 127, 127, 0], 7)
        .put_all(&[0, 0, 0, 0], 7)
        .put_all(&[0, 0, 0, 0], 7);
    w.put_all(&[0, 1, 1, 0], 1);
    w.indices(2, &[0, 15], |t| if t == 0 || t == 15 { 1 } else { 3 });
    let texels = decode_block(&w.block());
    // The P-bit applies to every channel: an endpoint with it set is (255, 1, 1), one without (0, 0, 0)
    // or (255, 0, 0) for the red endpoint 127 with it set, as 7 bits plus 1 widen to 255 and 1.
    for (texel, got) in texels.iter().enumerate() {
        let expected = match (texel % 4 >= 2, texel) {
            (_, 0) => [84, 0, 0, 255],
            (_, 15) => [171, 1, 1, 255],
            (false, _) => [255, 1, 1, 255],
            (true, _) => [0, 0, 0, 255],
        };
        assert_eq!(*got, expected, "texel {texel}");
    }
}

#[test]
fn mode_7_has_five_bit_alpha_whose_p_bit_widens_it_to_six_bits() {
    let mut w = Writer::mode(7);
    w.put(0, 6);
    w.put_all(&[0, 31, 0, 31], 5)
        .put_all(&[0, 31, 0, 31], 5)
        .put_all(&[0, 31, 0, 31], 5);
    w.put_all(&[0, 31, 31, 0], 5);
    w.put_all(&[0, 1, 0, 1], 1);
    w.indices(2, &[0, 15], |t| if t == 0 || t == 15 { 1 } else { 3 });
    let texels = decode_block(&w.block());
    assert_eq!(
        texels[0],
        grey(84),
        "subset 0 anchor, index 1 of 4, every channel"
    );
    assert_eq!(
        texels[15],
        [84, 84, 84, 170],
        "subset 1 anchor, alpha falls from 251 to 4"
    );
    assert_eq!(texels[1], grey(255));
    assert_eq!(texels[2], [255, 255, 255, 4]);
}

#[test]
fn a_block_with_no_mode_bit_is_transparent_black() {
    assert_eq!(decode_block(&[0; 16]), [[0; 4]; 16]);
}
