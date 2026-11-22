//! The instances of a batch draw: a window of a raw buffer a game's shader reads per instance,
//! the game's metadata words and the indexes of the instances to draw.
use super::properties::check_name;
use super::{ComputeBufferId, GraphicsError, MAX_PROPERTIES_PER_KIND};
use crate::{World, WorldError};

/// Most instance indexes one batch draw lists.
pub const MAX_BATCH_VISIBLE: usize = 1 << 20;

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
}

impl World {
    /// Checks a batch: its buffer is live, its window is not empty and lies inside the buffer,
    /// its metadata names are property names, and its lists are within their limits.
    pub(super) fn check_batch(&self, batch: &DrawBatch) -> Result<(), WorldError> {
        let held = self.compute_buffer(batch.buffer)?;
        let end = batch
            .offset
            .checked_add(batch.size)
            .filter(|&end| batch.size > 0 && end <= held.contents.len() as u64);
        if end.is_none() {
            return Err(GraphicsError::ComputeRange.into());
        }
        if batch.metadata.len() > MAX_PROPERTIES_PER_KIND || batch.visible.len() > MAX_BATCH_VISIBLE
        {
            return Err(GraphicsError::InvalidOffscreen.into());
        }
        for (name, _) in &batch.metadata {
            check_name(name)?;
        }
        Ok(())
    }
}
