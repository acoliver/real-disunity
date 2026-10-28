//! Decompression of LZ4 blocks, the format Unity compresses compiled shader programs with.
//!
//! A block is a series of sequences. Each starts with a token whose high four bits give a
//! literal count and whose low four bits give a match length less four; a nibble of 15 is
//! extended by following bytes until one is not 255. The literals follow, then a two-byte
//! little-endian offset back into the output, from which the match is copied. The last
//! sequence has literals only.

use crate::SerializedError;

/// The most output one byte of a block can produce: each length extension byte adds 255.
const MAX_RATIO: usize = 255;
const MIN_MATCH: usize = 4;

fn invalid(offset: usize, reason: &'static str) -> SerializedError {
    SerializedError::InvalidCompressedBlock { offset, reason }
}

fn byte(input: &[u8], pos: usize) -> Result<u8, SerializedError> {
    input.get(pos).copied().ok_or(SerializedError::Truncated {
        offset: pos,
        needed: 1,
        len: input.len(),
    })
}

/// A literal count or match length: the token's nibble, extended while bytes are 255.
fn length(input: &[u8], pos: &mut usize, nibble: u8) -> Result<usize, SerializedError> {
    let mut len = usize::from(nibble);
    if nibble == 15 {
        loop {
            let extra = byte(input, *pos)?;
            *pos += 1;
            len = len.saturating_add(usize::from(extra));
            if extra != 255 {
                break;
            }
        }
    }
    Ok(len)
}

/// Copies a match of `len` bytes starting `offset` bytes back; it may overlap its own output.
fn copy_match(out: &mut Vec<u8>, offset: usize, len: usize) {
    let start = out.len() - offset;
    if offset >= len {
        out.extend_from_within(start..start + len);
    } else {
        for i in 0..len {
            out.push(out[start + i]);
        }
    }
}

/// Decompresses one LZ4 block that must produce exactly `decompressed_len` bytes. Error
/// offsets are bytes into `input`.
///
/// # Errors
/// Fails when the block is truncated, refers back before the start of its output, or
/// produces more or fewer bytes than declared.
pub fn decompress_block(input: &[u8], decompressed_len: usize) -> Result<Vec<u8>, SerializedError> {
    if decompressed_len > input.len().saturating_mul(MAX_RATIO) {
        return Err(invalid(
            0,
            "the declared size is more than the block can produce",
        ));
    }
    let mut out = Vec::new();
    out.try_reserve_exact(decompressed_len)
        .map_err(|_| invalid(0, "the declared size cannot be allocated"))?;
    let mut pos = 0;
    loop {
        let token = byte(input, pos)?;
        pos += 1;
        let literals = length(input, &mut pos, token >> 4)?;
        let end = pos
            .checked_add(literals)
            .filter(|&end| end <= input.len())
            .ok_or_else(|| invalid(pos, "the literals run past the end of the block"))?;
        if literals > decompressed_len - out.len() {
            return Err(invalid(pos, "the output exceeds the declared size"));
        }
        out.extend_from_slice(&input[pos..end]);
        pos = end;
        if pos == input.len() {
            break;
        }
        let offset = usize::from(u16::from_le_bytes([
            byte(input, pos)?,
            byte(input, pos + 1)?,
        ]));
        if offset == 0 || offset > out.len() {
            return Err(invalid(
                pos,
                "the match refers back before the start of the output",
            ));
        }
        pos += 2;
        let len = length(input, &mut pos, token & 15)?.saturating_add(MIN_MATCH);
        if len > decompressed_len - out.len() {
            return Err(invalid(pos, "the output exceeds the declared size"));
        }
        copy_match(&mut out, offset, len);
    }
    if out.len() != decompressed_len {
        return Err(invalid(pos, "the output is shorter than the declared size"));
    }
    Ok(out)
}
