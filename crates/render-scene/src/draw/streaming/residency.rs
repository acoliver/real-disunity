//! Identifies the resident chunk buffers a frame draws, so a retained shadow cascade can
//! tell whether the terrain it holds is still the terrain the scene would draw.
use crate::terrain::streaming::TerrainChunk;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};

// One counter for every streaming source, so a replacement source never repeats a
// generation that a cascade was rendered with.
static NEXT: AtomicU64 = AtomicU64::new(1);

fn fresh() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug)]
pub(super) struct Residency {
    generation: Cell<u64>,
    keys: RefCell<Vec<TerrainChunk>>,
}

impl Residency {
    pub fn new() -> Self {
        Self {
            generation: Cell::new(fresh()),
            keys: RefCell::default(),
        }
    }

    /// Records the chunks a frame draws. A changed chunk set or any newly uploaded
    /// chunk buffer starts a new generation.
    pub fn observe(&self, keys: impl Iterator<Item = TerrainChunk>, uploaded: bool) {
        let keys: Vec<_> = keys.collect();
        if uploaded || *self.keys.borrow() != keys {
            self.generation.set(fresh());
            *self.keys.borrow_mut() = keys;
        }
    }

    /// Strictly increases whenever the drawn chunk buffers change.
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(minimum: u32, stride: u32) -> TerrainChunk {
        TerrainChunk {
            minimum: [minimum, 0],
            maximum: [minimum + stride * 4, stride * 4],
            stride,
        }
    }

    #[test]
    fn generation_changes_only_with_drawn_chunks_or_uploads() {
        let root = chunk(0, 2);
        let children = [chunk(0, 1), chunk(4, 1)];
        let residency = Residency::new();
        let initial = residency.generation();
        residency.observe([root].into_iter(), false);
        let first = residency.generation();
        assert!(first > initial);
        residency.observe([root].into_iter(), false);
        assert_eq!(residency.generation(), first);
        residency.observe([root].into_iter(), true);
        let uploaded = residency.generation();
        assert!(uploaded > first);
        residency.observe(children.into_iter(), false);
        assert!(residency.generation() > uploaded);
        assert!(Residency::new().generation() > residency.generation());
    }
}
