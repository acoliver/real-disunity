//! The order a grouped pass records its held draws in.
//!
//! Sorting the held draws themselves moves a draw and three buffer handles at every swap and
//! compares handles through the backend. A pass instead numbers each distinct buffer once as
//! the walk names it, ranks those few buffers, and sorts small keys of plain integers.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use wgpu::Buffer;

use crate::object::layout::GeometryLayout;

/// What orders one held draw: draws that share a pipeline, then a material, then buffers sort
/// together, and those that draw the same indices sort by where their instances start. Buffers are
/// by rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct GroupKey {
    pub pipeline: usize,
    pub geometry: GeometryLayout,
    pub material: usize,
    pub vertices: u32,
    pub base_vertex: i32,
    pub indices: u32,
    pub instances: u32,
    /// Where the draw's indices start in the index buffer, and how many it draws.
    pub first_index: u32,
    pub index_count: u32,
    /// Where the draw's instances start in the instance buffer.
    pub first_instance: u32,
}

impl GroupKey {
    /// Whether a draw with this key draws more instances of what a run of draws with the key `run`
    /// draws, the instances of the run ending at `end`: the same pipeline, material, buffers and
    /// indices, and instances that start where the run's end.
    pub fn continues(&self, run: &Self, end: u32) -> bool {
        self.first_instance == end
            && Self {
                first_instance: run.first_instance,
                ..*self
            } == *run
    }
}

/// What `grouped` sorts and answers in, kept between passes so that each reuses the storage the
/// one before grew.
#[derive(Debug, Default)]
pub(super) struct Grouping {
    packed: Vec<u128>,
    unpacked: Vec<(GroupKey, u32)>,
    order: Vec<u32>,
}

/// The positions of `keys` in the order a stable sort by key leaves them: keys that tie keep
/// the order the walk named them in.
///
/// Keys that fit 128 bits when each field takes the bits its largest value needs are packed,
/// most significant field first, with the position as the last field, and sorted as integers.
/// The position makes every packed key distinct and orders ties as a stable sort does, so the
/// unstable sort leaves the order of the stable one. Keys that do not fit sort as they are,
/// with the position as their last term.
pub(super) fn grouped<'a>(keys: &[GroupKey], storage: &'a mut Grouping) -> &'a [u32] {
    storage.order.clear();
    if let Some(packing) = Packing::fitting(keys) {
        storage.packed.clear();
        storage.packed.extend(
            keys.iter()
                .zip(0..)
                .map(|(key, position)| packing.pack(key, position)),
        );
        storage.packed.sort_unstable();
        storage
            .order
            .extend(storage.packed.iter().map(|&key| packing.position(key)));
    } else {
        storage.unpacked.clear();
        storage.unpacked.extend(keys.iter().copied().zip(0..));
        storage.unpacked.sort_unstable();
        storage
            .order
            .extend(storage.unpacked.iter().map(|&(_, position)| position));
    }
    &storage.order
}

/// How many bits each field of a packed key takes.
struct Packing {
    pipeline: u32,
    material: u32,
    vertices: u32,
    /// Base vertices count from the least of them, so a negative one packs too.
    least_base_vertex: i32,
    base_vertex: u32,
    indices: u32,
    instances: u32,
    first_index: u32,
    index_count: u32,
    first_instance: u32,
    position: u32,
}

/// The bits `value` and every smaller one need.
fn bits(value: u64) -> u32 {
    u64::BITS - value.leading_zeros()
}

impl Packing {
    /// The packing of `keys`, if their fields fit 128 bits together.
    fn fitting(keys: &[GroupKey]) -> Option<Self> {
        let (mut pipeline, mut material, mut vertices, mut indices, mut instances) =
            (0_u64, 0_u64, 0_u64, 0_u64, 0_u64);
        let (mut first_index, mut index_count, mut first_instance) = (0_u64, 0_u64, 0_u64);
        let (mut least, mut greatest) = (i32::MAX, i32::MIN);
        for key in keys {
            pipeline = pipeline.max(key.pipeline as u64);
            material = material.max(key.material as u64);
            vertices = vertices.max(u64::from(key.vertices));
            indices = indices.max(u64::from(key.indices));
            instances = instances.max(u64::from(key.instances));
            first_index = first_index.max(u64::from(key.first_index));
            index_count = index_count.max(u64::from(key.index_count));
            first_instance = first_instance.max(u64::from(key.first_instance));
            least = least.min(key.base_vertex);
            greatest = greatest.max(key.base_vertex);
        }
        if keys.is_empty() {
            return Some(Self {
                pipeline: 0,
                material: 0,
                vertices: 0,
                least_base_vertex: 0,
                base_vertex: 0,
                indices: 0,
                instances: 0,
                first_index: 0,
                index_count: 0,
                first_instance: 0,
                position: 0,
            });
        }
        let packing = Self {
            pipeline: bits(pipeline),
            material: bits(material),
            vertices: bits(vertices),
            least_base_vertex: least,
            base_vertex: bits(i64::from(greatest).abs_diff(i64::from(least))),
            indices: bits(indices),
            instances: bits(instances),
            first_index: bits(first_index),
            index_count: bits(index_count),
            first_instance: bits(first_instance),
            position: bits(keys.len() as u64 - 1),
        };
        // One bit for the vertex layout.
        let total = packing.pipeline
            + 1
            + packing.material
            + packing.vertices
            + packing.base_vertex
            + packing.indices
            + packing.instances
            + packing.first_index
            + packing.index_count
            + packing.first_instance
            + packing.position;
        (total <= u128::BITS).then_some(packing)
    }

    fn pack(&self, key: &GroupKey, position: u32) -> u128 {
        let base_vertex = i64::from(key.base_vertex).abs_diff(i64::from(self.least_base_vertex));
        let mut packed = key.pipeline as u128;
        for (field, width) in [
            (u128::from(key.geometry == GeometryLayout::Skinned), 1),
            (key.material as u128, self.material),
            (u128::from(key.vertices), self.vertices),
            (u128::from(base_vertex), self.base_vertex),
            (u128::from(key.indices), self.indices),
            (u128::from(key.instances), self.instances),
            (u128::from(key.first_index), self.first_index),
            (u128::from(key.index_count), self.index_count),
            (u128::from(key.first_instance), self.first_instance),
            (u128::from(position), self.position),
        ] {
            packed = packed << width | field;
        }
        packed
    }

    fn position(&self, packed: u128) -> u32 {
        let mask = (1_u128 << self.position) - 1;
        u32::try_from(packed & mask).expect("a position fits the bits of the draw count")
    }
}

/// A multiplicative hash for the few machine words a buffer's identity hashes to. The table is
/// private to one pass and its keys are not chosen by anything outside the process, so the
/// default hasher's resistance to collision attacks buys nothing here.
#[derive(Default)]
struct WordHasher(u64);

impl WordHasher {
    fn mix(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for WordHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.mix(u64::from(byte));
        }
    }

    fn write_u32(&mut self, word: u32) {
        self.mix(u64::from(word));
    }

    fn write_u64(&mut self, word: u64) {
        self.mix(word);
    }

    fn write_usize(&mut self, word: usize) {
        self.mix(word as u64);
    }
}

/// The distinct buffers a pass names, each numbered by first appearance.
#[derive(Default)]
pub(super) struct BufferTable {
    numbers: HashMap<Buffer, u32, BuildHasherDefault<WordHasher>>,
    buffers: Vec<Buffer>,
    /// The buffer named last, which the walk usually names again.
    last: Option<u32>,
}

impl BufferTable {
    /// The number of `buffer`, which is the same for every buffer that equals it.
    pub fn number(&mut self, buffer: &Buffer) -> u32 {
        if let Some(last) = self.last
            && self.buffers[last as usize] == *buffer
        {
            return last;
        }
        let number = if let Some(&number) = self.numbers.get(buffer) {
            number
        } else {
            let number = u32::try_from(self.buffers.len()).expect("few distinct buffers");
            self.numbers.insert(buffer.clone(), number);
            self.buffers.push(buffer.clone());
            number
        };
        self.last = Some(number);
        number
    }

    /// Forgets every buffer, keeping the storage.
    pub fn clear(&mut self) {
        self.numbers.clear();
        self.buffers.clear();
        self.last = None;
    }

    pub fn get(&self, number: u32) -> &Buffer {
        &self.buffers[number as usize]
    }

    /// The rank of each numbered buffer among them, in the order buffers compare. Distinct
    /// buffers never compare equal, so ranks are distinct.
    pub fn ranks(&self) -> Vec<u32> {
        let mut by_order: Vec<u32> = (0..self.buffers.len())
            .map(|number| u32::try_from(number).expect("few distinct buffers"))
            .collect();
        by_order.sort_unstable_by(|&a, &b| self.get(a).cmp(self.get(b)));
        let mut ranks = vec![0; by_order.len()];
        for (rank, number) in by_order.into_iter().enumerate() {
            ranks[number as usize] = u32::try_from(rank).expect("few distinct buffers");
        }
        ranks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order_of(keys: &[GroupKey]) -> Vec<u32> {
        grouped(keys, &mut Grouping::default()).to_vec()
    }

    fn lcg(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *state >> 33
    }

    /// A number below `bound`, as whichever integer type is wanted.
    fn below<T: TryFrom<u64>>(state: &mut u64, bound: u64) -> T {
        T::try_from(lcg(state) % bound.max(1)).unwrap_or_else(|_| unreachable!("a small bound"))
    }

    fn key(state: &mut u64, spread: u64) -> GroupKey {
        GroupKey {
            pipeline: below(state, spread),
            geometry: if below::<u8>(state, 2) == 0 {
                GeometryLayout::Static
            } else {
                GeometryLayout::Skinned
            },
            material: below(state, spread),
            vertices: below(state, spread),
            base_vertex: below::<i32>(state, spread * 3) - i32::try_from(spread).unwrap(),
            indices: below(state, spread),
            instances: below(state, spread),
            first_index: below(state, spread * 4),
            index_count: below(state, spread * 4),
            first_instance: below(state, spread * 4),
        }
    }

    fn stable_reference(keys: &[GroupKey]) -> Vec<u32> {
        let mut reference: Vec<(usize, GroupKey)> = keys.iter().copied().enumerate().collect();
        reference.sort_by_key(|&(_, key)| key);
        reference
            .iter()
            .map(|&(position, _)| u32::try_from(position).unwrap())
            .collect()
    }

    #[test]
    fn grouping_equals_a_stable_sort_of_the_draws_by_key() {
        let mut state = 0x5eed;
        for spread in [1, 2, 3, 5, 40, 1000] {
            for count in [0, 1, 2, 17, 64, 300, 2000] {
                let keys: Vec<GroupKey> = (0..count).map(|_| key(&mut state, spread)).collect();
                assert!(
                    Packing::fitting(&keys).is_some(),
                    "spread {spread}, {count} draws pack"
                );
                assert_eq!(
                    order_of(&keys),
                    stable_reference(&keys),
                    "spread {spread}, {count} draws"
                );
            }
        }
    }

    #[test]
    fn keys_too_wide_to_pack_sort_as_a_stable_sort_does() {
        let mut state = 0xfeed;
        for count in [1, 2, 17, 300] {
            // Pipeline and material alone fill 128 bits once their largest values are huge.
            let keys: Vec<GroupKey> = (0..count)
                .map(|index| GroupKey {
                    pipeline: if index % 5 == 0 {
                        usize::MAX
                    } else {
                        below(&mut state, 3)
                    },
                    material: if index % 7 == 0 {
                        usize::MAX
                    } else {
                        below(&mut state, 3)
                    },
                    ..key(&mut state, 4)
                })
                .collect();
            assert!(Packing::fitting(&keys).is_none());
            assert_eq!(order_of(&keys), stable_reference(&keys), "{count} draws");
        }
    }

    #[test]
    fn keys_at_the_extremes_of_each_field_pack_and_order_correctly() {
        let mut state = 0xbeef;
        let keys: Vec<GroupKey> = (0..400)
            .map(|index| GroupKey {
                pipeline: below(&mut state, 2),
                geometry: GeometryLayout::Static,
                material: below(&mut state, 3),
                vertices: [0, 1 << 12][index % 2],
                base_vertex: [i32::MIN, -1, 0, 1, i32::MAX][index % 5],
                indices: [0, 1 << 12][(index / 2) % 2],
                instances: [0, 1][(index / 4) % 2],
                first_index: [0, 1 << 12][(index / 8) % 2],
                index_count: [0, 1 << 12][(index / 8) % 2],
                first_instance: [0, 1 << 12][(index / 16) % 2],
            })
            .collect();
        assert!(Packing::fitting(&keys).is_some());
        assert_eq!(order_of(&keys), stable_reference(&keys));
    }

    #[test]
    fn keys_that_tie_stay_in_the_order_they_were_named() {
        let tied = GroupKey {
            pipeline: 1,
            geometry: GeometryLayout::Static,
            material: 2,
            vertices: 0,
            base_vertex: 0,
            indices: 0,
            instances: 0,
            first_index: 0,
            index_count: 0,
            first_instance: 0,
        };
        let earlier = GroupKey {
            pipeline: 0,
            ..tied
        };
        assert_eq!(order_of(&[tied, earlier, tied, tied]), [1, 0, 2, 3]);
    }

    /// The keys of draws of one mesh level in one material that draw the instances `starts`, each
    /// with the count after it.
    fn level(starts: &[(u32, u32)]) -> Vec<GroupKey> {
        starts
            .iter()
            .map(|&(first_instance, _)| GroupKey {
                pipeline: 1,
                geometry: GeometryLayout::Static,
                material: 2,
                vertices: 0,
                base_vertex: 7,
                indices: 0,
                instances: 0,
                first_index: 30,
                index_count: 6,
                first_instance,
            })
            .collect()
    }

    /// The instances each run of `order` draws, as a recording joins them.
    fn runs(order: &[u32], keys: &[GroupKey], ends: &[u32]) -> Vec<(u32, u32)> {
        let mut runs: Vec<(u32, u32)> = Vec::new();
        let mut first: Option<usize> = None;
        for &position in order {
            let position = position as usize;
            match (first, runs.last_mut()) {
                (Some(run), Some((_, end))) if keys[position].continues(&keys[run], *end) => {
                    *end = ends[position];
                }
                _ => {
                    first = Some(position);
                    runs.push((keys[position].first_instance, ends[position]));
                }
            }
        }
        runs
    }

    #[test]
    fn draws_of_one_level_whose_instances_follow_one_another_are_one_run_however_they_were_named() {
        // Named out of order, the instances 0..2, 2..5 and 5..6 sort side by side.
        let keys = level(&[(5, 6), (0, 2), (2, 5)]);
        let order = order_of(&keys);
        assert_eq!(order, [1, 2, 0]);
        assert_eq!(runs(&order, &keys, &[6, 2, 5]), [(0, 6)]);
    }

    #[test]
    fn a_gap_between_instances_or_a_change_of_what_is_drawn_ends_a_run() {
        let mut keys = level(&[(0, 2), (2, 4), (5, 6), (6, 8), (8, 9), (9, 10)]);
        keys[3].material = 3;
        keys[4].index_count = 3;
        keys[5].first_index = 31;
        let order = order_of(&keys);
        assert_eq!(
            runs(&order, &keys, &[2, 4, 6, 8, 9, 10]),
            [(8, 9), (0, 4), (5, 6), (9, 10), (6, 8)]
        );
    }

    #[test]
    fn buffers_are_numbered_once_each_and_ranked_in_the_order_they_compare() {
        let _session = crate::test_support::SceneTestSession::acquire();
        let context = crate::device::GpuContext::shared_for_test()
            .expect("GPU acceptance requires a real adapter");
        let make = || {
            context.device().create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            })
        };
        let buffers: Vec<Buffer> = (0..9).map(|_| make()).collect();
        let mut state = 0x5eed;
        let mut table = BufferTable::default();
        let mut first_seen: Vec<usize> = Vec::new();
        for _ in 0..400 {
            let pick: usize = below(&mut state, 9);
            // A clone is the same buffer, and a different handle to it numbers alike.
            let number = table.number(&buffers[pick].clone());
            let expected = if let Some(position) = first_seen.iter().position(|&seen| seen == pick)
            {
                position
            } else {
                first_seen.push(pick);
                first_seen.len() - 1
            };
            assert_eq!(usize::try_from(number).unwrap(), expected);
            assert_eq!(table.get(number), &buffers[pick]);
        }
        let ranks = table.ranks();
        for (a, &pick_a) in first_seen.iter().enumerate() {
            for (b, &pick_b) in first_seen.iter().enumerate() {
                assert_eq!(
                    ranks[a].cmp(&ranks[b]),
                    buffers[pick_a].cmp(&buffers[pick_b]),
                    "rank of {a} against {b}"
                );
            }
        }
    }
}
