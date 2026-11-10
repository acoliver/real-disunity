//! A byte encoding of a [`GraphicsSnapshot`], for a host whose world lives in another image.
//!
//! The engine image takes the snapshot from its world and [`encode`]s it; the host [`decode`]s it
//! and has the same values, floats included bit for bit. The bytes start with a four byte marker,
//! the version of the encoding and the length of the body, so a reader can refuse input of a
//! version it does not know before it reads anything else. The body holds the snapshot's fields
//! in declaration order.
//!
//! A decoder refuses truncated or corrupt input with a named [`WireError`]; it never panics and
//! never allocates more than the input it was given can fill.
mod codec;
mod command_parts;
mod context_parts;
mod graphics_parts;
mod readback_parts;
mod renderer_list_parts;
mod scene_parts;

use super::{GraphicsSnapshot, OffscreenWork};
use codec::{Reader, Wire, Writer};

/// The version of the encoding this module reads and writes.
pub const WIRE_VERSION: u32 = 4;
/// Most bytes the body of a snapshot holds.
pub const MAX_SNAPSHOT_BYTES: u32 = 1 << 31;

const MARKER: [u8; 4] = *b"DGSN";
const LENGTH_AT: usize = 8;
const HEADER_BYTES: usize = 12;

/// Why a snapshot could not be encoded or decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("the snapshot ends before its data does")]
    Truncated,
    #[error("the bytes do not start with the graphics snapshot marker")]
    BadMarker,
    #[error("the snapshot has version {0}, which this reader does not know")]
    UnknownVersion(u32),
    #[error("a length or count is over its limit")]
    TooLong,
    #[error("the value {0} names no variant")]
    UnknownTag(u32),
    #[error("a string is not UTF-8")]
    BadText,
    #[error("a map holds a key twice")]
    DuplicateKey,
    #[error("bytes follow the end of the snapshot")]
    TrailingBytes,
}

/// Encodes a snapshot.
///
/// # Errors
/// Fails when the snapshot holds a list or string over its limit, or its body is over
/// [`MAX_SNAPSHOT_BYTES`].
pub fn encode(snapshot: &GraphicsSnapshot) -> Result<Vec<u8>, WireError> {
    let mut out = Writer::new();
    out.raw(&MARKER);
    out.raw(&WIRE_VERSION.to_le_bytes());
    out.raw(&[0; 4]);
    snapshot.write(&mut out)?;
    let mut bytes = out.into_bytes();
    let length = bytes
        .len()
        .checked_sub(HEADER_BYTES)
        .and_then(|length| u32::try_from(length).ok())
        .filter(|length| *length <= MAX_SNAPSHOT_BYTES)
        .ok_or(WireError::TooLong)?;
    bytes
        .get_mut(LENGTH_AT..HEADER_BYTES)
        .ok_or(WireError::Truncated)?
        .copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

/// Decodes a snapshot that [`encode`] wrote.
///
/// # Errors
/// Fails on input that is truncated, has another marker or version, holds a value no encoder
/// writes, or has bytes after the snapshot.
pub fn decode(bytes: &[u8]) -> Result<GraphicsSnapshot, WireError> {
    let mut input = Reader::new(bytes);
    if input.take(MARKER.len())? != MARKER {
        return Err(WireError::BadMarker);
    }
    let version = u32::read(&mut input)?;
    if version != WIRE_VERSION {
        return Err(WireError::UnknownVersion(version));
    }
    let length = u32::read(&mut input)?;
    if length > MAX_SNAPSHOT_BYTES {
        return Err(WireError::TooLong);
    }
    let length = usize::try_from(length).map_err(|_| WireError::TooLong)?;
    match input.remaining().cmp(&length) {
        std::cmp::Ordering::Less => return Err(WireError::Truncated),
        std::cmp::Ordering::Greater => return Err(WireError::TrailingBytes),
        std::cmp::Ordering::Equal => {}
    }
    let snapshot = GraphicsSnapshot::read(&mut input)?;
    if input.remaining() != 0 {
        return Err(WireError::TrailingBytes);
    }
    Ok(snapshot)
}

/// Encodes one record of offscreen work in the encoding the snapshot holds it in, without the
/// header. A host that takes records one by one reads the variants with variable-length parts
/// from these bytes.
///
/// # Errors
/// Fails when the record holds a list or string over its limit.
pub fn encode_work(work: &OffscreenWork) -> Result<Vec<u8>, WireError> {
    let mut out = Writer::new();
    work.write(&mut out)?;
    Ok(out.into_bytes())
}

/// Decodes a record that [`encode_work`] wrote.
///
/// # Errors
/// Fails on input that is truncated, holds a value no encoder writes, or has bytes after the
/// record.
pub fn decode_work(bytes: &[u8]) -> Result<OffscreenWork, WireError> {
    let mut input = Reader::new(bytes);
    let work = OffscreenWork::read(&mut input)?;
    if input.remaining() != 0 {
        return Err(WireError::TrailingBytes);
    }
    Ok(work)
}
