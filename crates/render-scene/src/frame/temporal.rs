//! Current and previous camera data for temporal accumulation.
//!
//! # Where this data lives
//!
//! The 320-byte [`FrameUniform`](super::FrameUniform) block is a fixed ABI that every scene
//! shader reads, and its padding lanes are already assigned. Temporal data is therefore not
//! appended to it. [`TemporalView`] has its own 176-byte block, [`TemporalView::to_bytes`],
//! which only a temporal resolve binds. With jitter off the scene block is unchanged byte for
//! byte; with jitter on only its first sixty-four bytes differ, because the clip-from-scene
//! matrix carries the sub-pixel offset.
//!
//! # Pass policy
//!
//! The jittered matrix is the one the GPU rasterizes with, so every pass that reads
//! [`FrameUniform::view_projection`](super::FrameUniform::view_projection) agrees on where a
//! pixel is:
//!
//! | Pass | Matrix |
//! | --- | --- |
//! | Forward, terrain, instanced and clutter, foliage, impostors, cards | jittered |
//! | Sky, sky meshes and camera rays | jittered |
//! | Depth and normal prepass, SSAO and SSR reconstruction | jittered |
//! | Water surfaces and their opaque snapshots | jittered |
//! | Decals | jittered |
//! | Shadow cascade fitting and shadow casters | unjittered, light-space matrices |
//! | CPU culling, level of detail, streaming and decal selection | unjittered |
//!
//! CPU selection reads [`FrameUniform::selection_view_projection`](super::FrameUniform::selection_view_projection)
//! so that chunks, instances and decals do not change from one frame to the next because of a
//! sub-pixel offset.

use glam::{Mat4, Vec3};

/// Size in bytes of the block [`TemporalView::to_bytes`] encodes.
pub const TEMPORAL_UNIFORM_BYTES: usize = 176;

/// The camera of the frame being drawn and of the last frame that was published.
///
/// Present on a [`FrameUniform`](super::FrameUniform) only while projection jitter is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TemporalView {
    /// Clip-from-scene transform without jitter, the one the resolve reprojects with.
    pub unjittered_view_projection: Mat4,
    /// Unjittered clip-from-scene transform of the previous published frame.
    ///
    /// Equal to the current matrix when [`history_valid`](Self::history_valid) is false, so
    /// a reprojection through it is the identity rather than a stale lookup.
    pub previous_view_projection: Mat4,
    /// Sub-pixel offset applied to this frame's projection, in output pixels, +y down.
    pub jitter_pixels: [f32; 2],
    /// Offset applied to the previous published frame. Equal to the current one without history.
    pub previous_jitter_pixels: [f32; 2],
    /// False for the first frame and after a camera cut, resize, jitter change or reset.
    pub history_valid: bool,
    /// Animation clock of the previous published frame, in the milliseconds of
    /// [`FrameUniform::animation_time_milliseconds`](super::FrameUniform::animation_time_milliseconds).
    /// Equal to the current clock without history. A frame whose clock is earlier than the
    /// previous frame's discards the history instead, so this never exceeds the current clock.
    pub previous_animation_time_ms: u32,
    /// Camera position of the previous published frame in scene meters, for camera-dependent
    /// geometry such as billboards. Equal to the current position without history.
    pub previous_camera_position: Vec3,
}

impl TemporalView {
    /// Encodes the block for a temporal resolve.
    ///
    /// Bytes `0..64` are the unjittered matrix and `64..128` the previous one, both column-major.
    /// Bytes `128..144` hold `vec4(jitter, previous_jitter)` in pixels. Bytes `144..160` hold
    /// `vec4(history_valid, 0, 0, 0)` with `history_valid` as 1.0 or 0.0. Bytes `160..176` hold
    /// `vec4(previous_camera_position, previous_animation_time_ms)`, where the last lane is the
    /// raw `u32` bits (read it with `bitcast<u32>`), the same convention as the scene block.
    #[must_use]
    pub fn to_bytes(&self) -> [u8; TEMPORAL_UNIFORM_BYTES] {
        let mut lanes = Vec::with_capacity(TEMPORAL_UNIFORM_BYTES / 4);
        lanes.extend(self.unjittered_view_projection.to_cols_array());
        lanes.extend(self.previous_view_projection.to_cols_array());
        lanes.extend(self.jitter_pixels);
        lanes.extend(self.previous_jitter_pixels);
        lanes.extend([f32::from(u8::from(self.history_valid)), 0.0, 0.0, 0.0]);
        lanes.extend(self.previous_camera_position.to_array());
        lanes.push(f32::from_bits(self.previous_animation_time_ms));
        let mut bytes = [0u8; TEMPORAL_UNIFORM_BYTES];
        for (chunk, lane) in bytes.chunks_exact_mut(4).zip(lanes) {
            chunk.copy_from_slice(&lane.to_le_bytes());
        }
        bytes
    }
}

#[cfg(test)]
#[path = "temporal_tests.rs"]
mod tests;
