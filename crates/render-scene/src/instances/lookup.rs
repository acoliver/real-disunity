//! Finds the entries of a batch-keyed map for keys that arrive in ascending order.
//!
//! A view's gather names every batch it selects once, in key order, and asks for the batch, its
//! mirror range and what the mirror holds of it, one tree search each. When the view selects a
//! fair share of all batches, one walk along the map answers every key for the cost of a few
//! searches.
use std::collections::{BTreeMap, btree_map};
use std::iter::Peekable;

use super::BatchKey;

/// A key per this many entries or more is answered by walking the map.
const WALK_SHARE: usize = 16;

/// Looks up keys in `map`, which the caller asks for in ascending order.
pub(crate) struct Ascending<'a, V> {
    map: &'a BTreeMap<BatchKey, V>,
    /// The walk along the map, when walking is cheaper than searching `wanted` times.
    walk: Option<Peekable<btree_map::Iter<'a, BatchKey, V>>>,
    #[cfg(debug_assertions)]
    last: Option<BatchKey>,
}

impl<'a, V> Ascending<'a, V> {
    /// `wanted` is how many keys will be asked for.
    pub fn new(map: &'a BTreeMap<BatchKey, V>, wanted: usize) -> Self {
        Self {
            map,
            walk: (wanted.saturating_mul(WALK_SHARE) >= map.len()).then(|| map.iter().peekable()),
            #[cfg(debug_assertions)]
            last: None,
        }
    }

    /// The value at `key`, which is not below any key asked for before.
    pub fn get(&mut self, key: &BatchKey) -> Option<&'a V> {
        #[cfg(debug_assertions)]
        {
            assert!(
                self.last.is_none_or(|last| last <= *key),
                "keys are asked for in ascending order"
            );
            self.last = Some(*key);
        }
        let Some(walk) = &mut self.walk else {
            return self.map.get(key);
        };
        while walk.next_if(|(held, _)| *held < key).is_some() {}
        walk.peek()
            .filter(|(held, _)| *held == key)
            .map(|&(_, value)| value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> BTreeMap<BatchKey, usize> {
        (0..400)
            .map(|index| ((index % 7, index / 7, index % 11), index))
            .collect()
    }

    #[test]
    fn walking_and_searching_find_what_a_search_for_each_key_finds() {
        let map = map();
        let mut state = 12_345_u64;
        for share in [1, 2, 7, 30, 200, 800] {
            // Ascending probes, some of which the map lacks and some repeated.
            let mut probes: Vec<BatchKey> = (0..share)
                .map(|_| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    let pick = usize::try_from(state >> 40).unwrap();
                    (pick % 8, pick / 8 % 60, pick % 12)
                })
                .collect();
            probes.sort_unstable();
            let wanted = if share == 800 { 1 } else { probes.len() };
            for wanted in [wanted, probes.len(), 1] {
                let mut lookup = Ascending::new(&map, wanted);
                for key in &probes {
                    assert_eq!(lookup.get(key), map.get(key), "{key:?} of {share}");
                }
            }
        }
    }

    #[test]
    fn a_large_share_of_the_map_is_walked_and_a_small_one_searched() {
        let map = map();
        assert!(Ascending::new(&map, map.len()).walk.is_some());
        assert!(Ascending::new(&map, map.len() / WALK_SHARE).walk.is_some());
        assert!(Ascending::new(&map, 1).walk.is_none());
        assert!(
            Ascending::new(&BTreeMap::<BatchKey, u8>::new(), 0)
                .walk
                .is_some()
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "ascending order")]
    fn keys_out_of_order_are_refused_in_debug_builds() {
        let map = map();
        let mut lookup = Ascending::new(&map, map.len());
        lookup.get(&(3, 0, 0));
        lookup.get(&(1, 0, 0));
    }
}
