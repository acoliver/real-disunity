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

/// Bytes of the `static const unsigned char` array in a reference header.
fn header_bytes(text: &str) -> Vec<u8> {
    let (_, body) = text.split_once("Bytes[] = {").expect("byte array");
    let (body, _) = body.split_once('}').expect("end of array");
    body.split(',')
        .filter_map(|token| token.trim().strip_prefix("0x"))
        .map(|digits| u8::from_str_radix(digits, 16).expect("hex byte"))
        .collect()
}

/// Compares byte for byte with the published headers. Fetch them without committing:
/// `curl -O https://raw.githubusercontent.com/iryoku/smaa/master/Textures/{AreaTex,SearchTex}.h`
/// and run with `SMAA_REFERENCE_DIR` set to that directory.
#[test]
#[ignore = "needs SMAA_REFERENCE_DIR with the reference AreaTex.h and SearchTex.h"]
fn generated_textures_equal_the_downloaded_reference_headers() {
    let dir = std::path::PathBuf::from(std::env::var("SMAA_REFERENCE_DIR").expect("directory"));
    let read = |name: &str| header_bytes(&std::fs::read_to_string(dir.join(name)).unwrap());
    let area = read("AreaTex.h");
    let search = read("SearchTex.h");
    assert_eq!(area.len(), 160 * 560 * 2);
    assert_eq!(search.len(), 64 * 16);
    assert!(area == area_texture(true), "area texture differs");
    assert!(search == search_texture(), "search texture differs");
    assert_eq!(fnv1a(&area), AREA_DIGEST);
    assert_eq!(fnv1a(&search), SEARCH_DIGEST);
}
