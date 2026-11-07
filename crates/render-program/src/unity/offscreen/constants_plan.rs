//! Where a frame's constant blocks go: into buffers that each fit the device, with blocks of equal
//! bytes sharing one slice.

use std::collections::HashMap;

use super::error::OffscreenError;

/// The sizes a frame's constants must stay within.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ConstantsLimits {
    /// The largest block one draw binds.
    pub(super) block: u64,
    /// The largest buffer, which each chunk of the plan fills at most.
    pub(super) chunk: u64,
    /// The most bytes all the chunks may hold together.
    pub(super) total: u64,
}

/// Where a block's bytes are: the chunk (buffer) and the offset in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Placement {
    pub(super) chunk: usize,
    pub(super) offset: u64,
}

/// A push that put bytes in a chunk, to undo.
struct Placed {
    /// The chunk's length before the push, and whether the push opened the chunk.
    before: usize,
    opened: bool,
    hash: Option<blake3::Hash>,
}

/// The constants of a frame's draws, as chunks that each fit one buffer.
///
/// A block goes at the next multiple of the uniform offset alignment of the last chunk, or at the
/// start of a new chunk when it does not fit there. A block whose bytes equal an earlier block's
/// takes that block's place.
pub(super) struct ConstantsPlan {
    chunks: Vec<Vec<u8>>,
    alignment: usize,
    limits: ConstantsLimits,
    total: u64,
    shared: HashMap<blake3::Hash, Placement>,
    journal: Vec<Placed>,
    /// The blocks pushed and the ones that took an earlier slice, draws left out later included.
    blocks: usize,
    shared_blocks: usize,
}

impl ConstantsPlan {
    pub(super) fn new(alignment: u32, limits: ConstantsLimits) -> Self {
        Self {
            chunks: Vec::new(),
            alignment: (alignment as usize).max(1),
            limits,
            total: 0,
            shared: HashMap::new(),
            journal: Vec::new(),
            blocks: 0,
            shared_blocks: 0,
        }
    }

    /// The chunks, each the bytes of one buffer.
    pub(super) fn chunks(&self) -> &[Vec<u8>] {
        &self.chunks
    }

    /// The blocks pushed and how many of them took the slice of an earlier block, including those
    /// of draws that were taken back.
    pub(super) fn block_counts(&self) -> (usize, usize) {
        (self.blocks, self.shared_blocks)
    }

    /// The most bytes one chunk holds.
    pub(super) fn chunk_limit(&self) -> u64 {
        self.limits.chunk
    }

    /// The bytes of all the chunks, padding included.
    pub(super) fn total_bytes(&self) -> u64 {
        self.total
    }

    /// Where the next draw's constants start, to give back to [`rollback`](Self::rollback).
    pub(super) fn mark(&self) -> usize {
        self.journal.len()
    }

    /// Takes back the blocks pushed since `mark`, for a draw that was left out after packing them.
    /// A block that shared an earlier slice put nothing in, so only the bytes it added go.
    pub(super) fn rollback(&mut self, mark: usize) {
        while self.journal.len() > mark {
            let Some(placed) = self.journal.pop() else {
                break;
            };
            if placed.opened {
                self.chunks.pop();
            } else if let Some(chunk) = self.chunks.last_mut() {
                chunk.truncate(placed.before);
            }
            if let Some(hash) = placed.hash {
                self.shared.remove(&hash);
            }
        }
        self.total = self.chunks.iter().map(|chunk| chunk.len() as u64).sum();
    }

    /// Places `slice` and says where.
    ///
    /// # Errors
    /// Fails when the block is larger than a binding or a buffer may be, or when the chunks would
    /// hold more than the limit. Neither is cured by another split, so the frame is refused at
    /// once, before the rest of its draws are packed.
    pub(super) fn push(&mut self, slice: &[u8]) -> Result<Placement, OffscreenError> {
        let len = slice.len() as u64;
        let largest = self.limits.block.min(self.limits.chunk);
        if len > largest {
            return Err(OffscreenError::ConstantsTooLarge {
                len,
                limit: largest,
            });
        }
        self.blocks += 1;
        let hash = blake3::hash(slice);
        if let Some(&placement) = self.shared.get(&hash)
            && self.bytes_at(placement, slice.len()) == Some(slice)
        {
            self.shared_blocks += 1;
            return Ok(placement);
        }
        let last = self.chunks.last().map_or(0, Vec::len);
        let start = last.next_multiple_of(self.alignment);
        let opened = self.chunks.is_empty() || (start + slice.len()) as u64 > self.limits.chunk;
        let start = if opened { 0 } else { start };
        let added = if opened {
            slice.len()
        } else {
            start + slice.len() - last
        };
        let needed = self.total + added as u64;
        if needed > self.limits.total {
            return Err(OffscreenError::ConstantsOverBudget {
                needed,
                budget: self.limits.total,
            });
        }
        if opened {
            self.chunks.push(Vec::new());
        }
        let chunk_index = self.chunks.len() - 1;
        let chunk = &mut self.chunks[chunk_index];
        let before = if opened { 0 } else { chunk.len() };
        chunk.resize(start, 0);
        chunk.extend(slice);
        self.total = needed;
        let placement = Placement {
            chunk: chunk_index,
            offset: start as u64,
        };
        // A hash already taken by other bytes is not registered, so those bytes keep their entry.
        let registered = !self.shared.contains_key(&hash);
        if registered {
            self.shared.insert(hash, placement);
        }
        self.journal.push(Placed {
            before,
            opened,
            hash: registered.then_some(hash),
        });
        Ok(placement)
    }

    fn bytes_at(&self, placement: Placement, len: usize) -> Option<&[u8]> {
        let start = usize::try_from(placement.offset).ok()?;
        self.chunks
            .get(placement.chunk)?
            .get(start..start.checked_add(len)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(chunk: u64, total: u64) -> ConstantsPlan {
        ConstantsPlan::new(
            4,
            ConstantsLimits {
                block: 64,
                chunk,
                total,
            },
        )
    }

    fn at(chunk: usize, offset: u64) -> Placement {
        Placement { chunk, offset }
    }

    #[test]
    fn a_block_starts_at_the_alignment_and_a_full_chunk_opens_the_next() {
        let mut plan = plan(16, 1024);
        assert_eq!(plan.push(&[1; 6]).unwrap(), at(0, 0));
        assert_eq!(plan.push(&[2; 6]).unwrap(), at(0, 8));
        assert_eq!(plan.push(&[3; 6]).unwrap(), at(1, 0));
        assert_eq!(plan.chunks()[0].len(), 14);
        assert_eq!(plan.total_bytes(), 20);
    }

    #[test]
    fn equal_bytes_take_the_place_of_the_first() {
        let mut plan = plan(16, 1024);
        let first = plan.push(&[7; 8]).unwrap();
        plan.push(&[8; 8]).unwrap();
        assert_eq!(plan.push(&[7; 8]).unwrap(), first);
        assert_eq!(plan.total_bytes(), 16);
        // The same bytes of another length are another block.
        assert_ne!(plan.push(&[7; 4]).unwrap(), first);
    }

    #[test]
    fn taking_back_a_draw_takes_back_what_it_added_and_the_slices_it_made() {
        let mut plan = plan(16, 1024);
        plan.push(&[1; 8]).unwrap();
        let mark = plan.mark();
        plan.push(&[2; 8]).unwrap();
        plan.push(&[3; 8]).unwrap();
        plan.rollback(mark);
        assert_eq!(plan.chunks().len(), 1);
        assert_eq!(plan.total_bytes(), 8);
        // The rolled-back bytes are placed afresh, not found at their old place.
        assert_eq!(plan.push(&[3; 8]).unwrap(), at(0, 8));
        assert_eq!(plan.push(&[1; 8]).unwrap(), at(0, 0));
    }

    #[test]
    fn a_block_that_fits_no_buffer_is_refused_with_the_sizes() {
        let mut plan = plan(16, 1024);
        let error = plan.push(&[0; 17]).unwrap_err();
        assert!(matches!(
            error,
            OffscreenError::ConstantsTooLarge { len: 17, limit: 16 }
        ));
        let mut plan = ConstantsPlan::new(
            4,
            ConstantsLimits {
                block: 8,
                chunk: 1024,
                total: 1024,
            },
        );
        let error = plan.push(&[0; 9]).unwrap_err();
        assert!(matches!(
            error,
            OffscreenError::ConstantsTooLarge { len: 9, limit: 8 }
        ));
    }

    #[test]
    fn constants_past_the_total_are_refused_where_they_pass_it() {
        let mut plan = plan(16, 24);
        plan.push(&[1; 16]).unwrap();
        let error = plan.push(&[2; 16]).unwrap_err();
        assert!(matches!(
            error,
            OffscreenError::ConstantsOverBudget {
                needed: 32,
                budget: 24
            }
        ));
        assert_eq!(plan.total_bytes(), 16, "the refused block added nothing");
    }
}
