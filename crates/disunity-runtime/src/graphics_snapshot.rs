//! The frame's graphics snapshot: the offscreen work scripts recorded and everything the work
//! names, taken from the engine image's world and decoded into plain data.
//!
//! The engine image encodes the snapshot (see
//! `worldrunner_world_simulation::graphics::snapshot_wire`); the host decodes it here into the
//! same [`GraphicsSnapshot`] that `GraphicsSnapshot::take` gives a host whose world is in its own
//! process.
use worldrunner_world_simulation::graphics::{GraphicsSnapshot, snapshot_wire};

use crate::gpu_flush::symbol;
use crate::native::DisunityResult;
use crate::{DisunityHost, GpuFlush, HostError};

/// `disunity_graphics_snapshot`: world, buffer, capacity and the length written.
pub(crate) type SnapshotCall = unsafe extern "C" fn(u64, *mut u8, u32, *mut u32) -> DisunityResult;

/// Asks the engine image for the snapshot's size, then for its bytes, and decodes them. The size
/// query holds the snapshot in the engine image, so the copy that follows gets the same one and
/// no record is lost or taken twice.
fn fetch(
    call: SnapshotCall,
    world: u64,
    check: impl Fn(DisunityResult) -> Result<(), HostError>,
) -> Result<GraphicsSnapshot, HostError> {
    let mut length = 0_u32;
    // SAFETY: a zero capacity writes only the length; `length` is writable for one value.
    check(unsafe { call(world, std::ptr::null_mut(), 0, &mut length) })?;
    let mut bytes = vec![0_u8; length as usize];
    // SAFETY: `bytes` holds `length` writable bytes, the capacity passed.
    check(unsafe { call(world, bytes.as_mut_ptr(), length, &mut length) })?;
    bytes.truncate(length as usize);
    snapshot_wire::decode(&bytes).map_err(HostError::Snapshot)
}

impl DisunityHost {
    /// Takes the offscreen work scripts recorded since the last take, with the cameras, renderers,
    /// materials, shaders, render textures, global state and compute resources that work names,
    /// from the world of the running slice. Like `GraphicsSnapshot::take` it drains: the next call
    /// returns only work recorded after this one, and a frame with no work gives an empty
    /// snapshot.
    pub fn graphics_snapshot(&mut self) -> Result<GraphicsSnapshot, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let call: SnapshotCall = symbol(&self._library, b"disunity_graphics_snapshot\0")?;
        fetch(call, world, |result| {
            if result.code == 0 {
                Ok(())
            } else {
                Err(self.native_error(result))
            }
        })
    }
}

impl GpuFlush<'_> {
    /// The same snapshot as [`DisunityHost::graphics_snapshot`], for the world being flushed:
    /// the pending offscreen work, dispatches included, with the shaders, materials, render
    /// textures, compute shaders and compute buffers it names. A flush handler takes it to
    /// execute the work, then writes results with [`GpuFlush::write_compute_buffer`].
    pub fn snapshot(&mut self) -> Result<GraphicsSnapshot, HostError> {
        fetch(self.calls.snapshot, self.world, |result| self.check(result))
    }
}
