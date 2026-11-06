//! The frame's graphics snapshot, handed to a host as bytes.
//!
//! The host asks for the snapshot's size and then copies it out, as for the engine's other
//! variable-length results. The first call takes the world's pending offscreen work with the
//! materials, shaders, cameras, renderers and compute resources that work can reach, encodes it
//! (see `worldrunner_world_simulation::graphics::snapshot_wire`) and keeps the bytes in the
//! session; the call that copies hands them out and forgets them. The size query alone never
//! drains: asking twice returns the same size, and work recorded after the first query waits for
//! the next snapshot. A snapshot nobody copied is dropped when the world advances, as the
//! world's own pending work is.
use crate::{DisunityResult, callbacks::mutate, errors::AbiError};
use worldrunner_world_simulation::graphics::{GraphicsSnapshot, snapshot_wire};

/// Writes the length of the snapshot to `length`. With a zero `capacity` that is all it does.
/// Otherwise `capacity` bytes at `buffer` must hold the snapshot: it is copied there and the
/// session forgets it, and a smaller `capacity` is rejected without copying or dropping
/// anything, so the host can ask again with room for `length` bytes.
///
/// # Safety
/// `buffer` is writable for `capacity` bytes, or null when `capacity` is zero; `length` is
/// writable for one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_graphics_snapshot(
    world: u64,
    buffer: *mut u8,
    capacity: u32,
    length: *mut u32,
) -> DisunityResult {
    if length.is_null() || (capacity > 0 && buffer.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        if s.graphics_snapshot.is_none() {
            let snapshot = GraphicsSnapshot::take(&mut s.native)?;
            let bytes = snapshot_wire::encode(&snapshot)
                .map_err(|error| AbiError::Message(error.to_string()))?;
            s.graphics_snapshot = Some(bytes);
        }
        let held = s.graphics_snapshot.as_deref().unwrap_or_default();
        let needed = u32::try_from(held.len())
            .map_err(|_| AbiError::Message("the graphics snapshot is over 4 GiB".into()))?;
        unsafe { *length = needed };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < needed {
            return Err(AbiError::Message(
                "the buffer is too small for the graphics snapshot".into(),
            ));
        }
        unsafe { std::ptr::copy_nonoverlapping(held.as_ptr(), buffer, held.len()) };
        s.graphics_snapshot = None;
        Ok(())
    })
}
