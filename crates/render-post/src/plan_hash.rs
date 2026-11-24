//! Hash maps for the small keys of one frame's plan: resource names, pass entries, textures and
//! subresource cells.
//!
//! A frame's plan indexes a few hundred such keys several times each, so the std hasher's
//! per-key cost is a large share of validating it. These keys are the host's own declarations
//! within one process, so the hasher does not resist chosen-key collisions.
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

pub(crate) type PlanMap<K, V> = HashMap<K, V, BuildHasherDefault<PlanHasher>>;
pub(crate) type PlanSet<K> = HashSet<K, BuildHasherDefault<PlanHasher>>;

/// A map with room for `capacity` keys.
pub(crate) fn map_with_capacity<K, V>(capacity: usize) -> PlanMap<K, V> {
    PlanMap::with_capacity_and_hasher(capacity, BuildHasherDefault::default())
}

/// A set with room for `capacity` keys.
pub(crate) fn set_with_capacity<K>(capacity: usize) -> PlanSet<K> {
    PlanSet::with_capacity_and_hasher(capacity, BuildHasherDefault::default())
}

/// Multiplicative word hashing: each word is folded into the state with a rotate, an xor and a
/// multiply by an odd constant.
#[derive(Default)]
pub(crate) struct PlanHasher(u64);

const MULTIPLIER: u64 = 0xf135_7aea_2e62_a9c5;

impl PlanHasher {
    fn fold(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(MULTIPLIER);
    }
}

impl Hasher for PlanHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut words = bytes.chunks_exact(8);
        for word in words.by_ref() {
            self.fold(u64::from_le_bytes(
                word.try_into().expect("an eight byte chunk"),
            ));
        }
        let tail = words.remainder();
        if !tail.is_empty() {
            let mut padded = [0; 8];
            padded[..tail.len()].copy_from_slice(tail);
            self.fold(u64::from_le_bytes(padded));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.fold(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.fold(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.fold(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.fold(value as u64);
    }

    fn finish(&self) -> u64 {
        // The multiply mixes upward, so the low bits the table indexes by are the poorest.
        self.0.rotate_left(26)
    }
}
