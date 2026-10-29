//! The generated lookup textures must equal the reference `AreaTex.h` and `SearchTex.h`.
//! The reference bytes are too large to commit, so the tests compare 64-bit FNV-1a
//! digests taken from those headers (iryoku/smaa, `Textures/`).
use crate::smaa_lut::{AREA_EXTENT, SEARCH_EXTENT, area_texture, search_texture};

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Left half only: the reference with the diagonal columns zeroed.
const ORTHOGONAL_DIGEST: u64 = 0x4798_80f8_efbd_c215;
/// Right half only: the reference with the orthogonal columns zeroed.
const DIAGONAL_DIGEST: u64 = 0x84c2_ee2b_6f16_1815;
const AREA_DIGEST: u64 = 0x247a_58bb_ba65_292d;
const SEARCH_DIGEST: u64 = 0x21c1_fcf0_aa63_1065;

#[test]
fn texture_sizes_match_the_reference_layout() {
    assert_eq!(AREA_EXTENT, [160, 560]);
    assert_eq!(SEARCH_EXTENT, [64, 16]);
    assert_eq!(area_texture(false).len(), 160 * 560 * 2);
    assert_eq!(search_texture().len(), 64 * 16);
}

#[test]
fn orthogonal_area_texture_equals_the_reference_bytes() {
    assert_eq!(fnv1a(&area_texture(false)), ORTHOGONAL_DIGEST);
}

#[test]
fn search_texture_equals_the_reference_bytes() {
    assert_eq!(fnv1a(&search_texture()), SEARCH_DIGEST);
}

#[test]
fn full_area_texture_equals_the_reference_bytes() {
    let ortho = area_texture(false);
    let full = area_texture(true);
    assert_eq!(fnv1a(&full), AREA_DIGEST);
    let mut diagonal = full;
    for (out, base) in diagonal.iter_mut().zip(&ortho) {
        *out ^= *base;
    }
    assert_eq!(fnv1a(&diagonal), DIAGONAL_DIGEST);
}
