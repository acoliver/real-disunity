//! Generational storage for the graphics resources of a world.
use crate::{LimitKind, WorldError};

/// Generations start at 1, so the all-zero bit pattern never names a resource.
pub(super) const FIRST_GENERATION: u32 = 1;

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Slots that reuse vacated indices and bump the generation on every reuse, so an id of a
/// destroyed resource never reads its successor.
pub(super) struct Slab<T> {
    slots: Vec<Slot<T>>,
    vacant: Vec<usize>,
    limit: usize,
}

impl<T> Slab<T> {
    pub(super) const fn new(limit: usize) -> Self {
        Self {
            slots: Vec::new(),
            vacant: Vec::new(),
            limit,
        }
    }

    /// Stores `value` and returns the bits of its id: the slot in the low half, the generation in
    /// the high half.
    pub(super) fn insert(&mut self, value: T) -> Result<u64, WorldError> {
        let capacity = WorldError::CapacityExceeded(LimitKind::Graphics);
        if let Some(&index) = self.vacant.last() {
            let slot = &mut self.slots[index];
            slot.generation = slot.generation.checked_add(1).ok_or(capacity)?;
            slot.value = Some(value);
            self.vacant.pop();
            return Ok(pack(index, slot.generation));
        }
        if self.slots.len() >= self.limit {
            return Err(capacity);
        }
        self.slots.push(Slot {
            generation: FIRST_GENERATION,
            value: Some(value),
        });
        Ok(pack(self.slots.len() - 1, FIRST_GENERATION))
    }

    pub(super) fn get(&self, bits: u64) -> Option<&T> {
        let (index, generation) = unpack(bits)?;
        let slot = self.slots.get(index)?;
        if slot.generation == generation {
            slot.value.as_ref()
        } else {
            None
        }
    }

    pub(super) fn get_mut(&mut self, bits: u64) -> Option<&mut T> {
        let (index, generation) = unpack(bits)?;
        let slot = self.slots.get_mut(index)?;
        if slot.generation == generation {
            slot.value.as_mut()
        } else {
            None
        }
    }

    /// Every live value with the bits of its id, in slot order.
    pub(super) fn iter(&self) -> impl Iterator<Item = (u64, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.value
                .as_ref()
                .map(|value| (pack(index, slot.generation), value))
        })
    }

    pub(super) fn remove(&mut self, bits: u64) -> Option<T> {
        let (index, generation) = unpack(bits)?;
        let slot = self.slots.get_mut(index)?;
        if slot.generation != generation {
            return None;
        }
        let removed = slot.value.take();
        if removed.is_some() {
            self.vacant.push(index);
        }
        removed
    }
}

fn pack(index: usize, generation: u32) -> u64 {
    let index = u32::try_from(index).expect("the slab limit fits 32 bits");
    (u64::from(generation) << u32::BITS) | u64::from(index)
}

fn unpack(bits: u64) -> Option<(usize, u32)> {
    let index = usize::try_from(bits & u64::from(u32::MAX)).ok()?;
    let generation = u32::try_from(bits >> u32::BITS).ok()?;
    (generation >= FIRST_GENERATION).then_some((index, generation))
}
