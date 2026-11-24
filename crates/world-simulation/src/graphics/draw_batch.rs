//! The instances of a batch draw: a window of a raw buffer a game's shader reads per instance,
//! the game's metadata words and the indexes of the instances to draw.
use super::properties::check_name;
use super::{ComputeBufferId, GraphicsError, MAX_PROPERTIES_PER_KIND};
use crate::{World, WorldError};

/// Most instance indexes one batch draw lists.
pub const MAX_BATCH_VISIBLE: usize = 1 << 20;

/// Most pass tags one batch draw names.
pub const MAX_BATCH_PASS_TAGS: usize = 8;

/// What a batch draw makes instances of: `size` bytes of `buffer` from `offset`, which the shader
/// reads as its instance data. The world and the ABI do not read the bytes or know their layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawBatch {
    pub buffer: ComputeBufferId,
    pub offset: u64,
    pub size: u64,
    /// The metadata word of each per-instance property the game set, by the property's name. The
    /// host copies a word into the shader's metadata member of that name as it is.
    pub metadata: Vec<(String, u32)>,
    /// The index of each instance to draw, in the order they draw. An empty list draws nothing
    /// and keeps the record's place among the frame's records.
    pub visible: Vec<u32>,
    /// The `LightMode` values of the passes the draw is for, in order of preference, as a
    /// renderer list names them. A draw with tags draws the pass of the first tag any pass of the
    /// material's shader has, and is skipped when no pass has one. A draw without tags uses the
    /// record's pass.
    pub pass_tags: Vec<String>,
}

impl World {
    /// Checks a batch: its buffer is live, its window is not empty and lies inside the buffer,
    /// its metadata names are property names, its pass tags are names, and its lists are within their limits.
    pub(super) fn check_batch(&self, batch: &DrawBatch) -> Result<(), WorldError> {
        let held = self.compute_buffer(batch.buffer)?;
        let end = batch
            .offset
            .checked_add(batch.size)
            .filter(|&end| batch.size > 0 && end <= held.contents.len() as u64);
        if end.is_none() {
            return Err(GraphicsError::ComputeRange.into());
        }
        if batch.metadata.len() > MAX_PROPERTIES_PER_KIND
            || batch.visible.len() > MAX_BATCH_VISIBLE
            || batch.pass_tags.len() > MAX_BATCH_PASS_TAGS
        {
            return Err(GraphicsError::InvalidOffscreen.into());
        }
        for name in batch
            .metadata
            .iter()
            .map(|(name, _)| name)
            .chain(&batch.pass_tags)
        {
            check_name(name)?;
        }
        Ok(())
    }
}
