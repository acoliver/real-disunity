//! Comparison of GPU output whose last bits depend on the driver.
//!
//! Vulkan does not specify the rounding of float arithmetic, of the conversion
//! to float16 or of 8-bit normalisation, and each shader compile may contract
//! or reorder operations differently. Byte identity is therefore only valid
//! between two runs of the same code on the same device. When two different
//! shader sources are mathematically equal, their results agree before storage
//! to far better than one storage step, so after storage they are equal or
//! adjacent. These helpers measure the largest distance for such a test.

/// Largest distance, in float16 representable values, between little-endian
/// float16 words. Words are ordered numerically, so -0 and +0 are one step apart.
pub(crate) fn max_half_steps(actual: &[u8], expected: &[u8]) -> u32 {
    assert_eq!(actual.len(), expected.len(), "buffer lengths differ");
    let ordered = |bytes: &[u8]| {
        let word = u16::from_le_bytes([bytes[0], bytes[1]]);
        let magnitude = i32::from(word & 0x7fff);
        if word & 0x8000 == 0 {
            magnitude
        } else {
            -magnitude
        }
    };
    actual
        .chunks_exact(2)
        .zip(expected.chunks_exact(2))
        .map(|(a, e)| ordered(a).abs_diff(ordered(e)))
        .max()
        .unwrap_or(0)
}

/// Largest distance in 8-bit codes between two byte buffers.
pub(crate) fn max_byte_codes(actual: &[u8], expected: &[u8]) -> u32 {
    assert_eq!(actual.len(), expected.len(), "buffer lengths differ");
    actual
        .iter()
        .zip(expected)
        .map(|(a, e)| u32::from(a.abs_diff(*e)))
        .max()
        .unwrap_or(0)
}
