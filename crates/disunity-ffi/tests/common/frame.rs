//! Reads the world's frame the way a host does: through `disunity_graphics_snapshot`, the one
//! reader of the frame's work, materials and global state.
use worldrunner_disunity_ffi::graphics_snapshot::*;
use worldrunner_world_simulation::graphics::{GraphicsSnapshot, snapshot_wire};

/// Drains the frame into a snapshot with a size query and one copy.
pub fn snapshot(world: u64) -> GraphicsSnapshot {
    let mut length = 0;
    unsafe {
        assert_eq!(
            disunity_graphics_snapshot(world, std::ptr::null_mut(), 0, &mut length).code,
            0
        )
    };
    let mut bytes = vec![0_u8; length as usize];
    unsafe {
        assert_eq!(
            disunity_graphics_snapshot(world, bytes.as_mut_ptr(), length, &mut length).code,
            0
        )
    };
    snapshot_wire::decode(&bytes[..length as usize]).unwrap()
}
