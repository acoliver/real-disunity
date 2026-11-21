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
/// together. Buffers are by rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct GroupKey {
    pub pipeline: usize,
    pub geometry: GeometryLayout,
    pub material: usize,
    pub vertices: u32,
    pub base_vertex: i32,
    pub indices: u32,
    pub instances: u32,
}

/// The positions of `keys` in the order a stable sort by key leaves them: keys that tie keep
/// the order the walk named them in.
///
/// Keys that fit 128 bits when each field takes the bits its largest value needs are packed,
/// most significant field first, with the position as the last field, and sorted as integers.
/// The position makes every packed key distinct and orders ties as a stable sort does, so the
/// unstable sort leaves the order of the stable one. Keys that do not fit sort as they are,
/// with the position as their last term.
pub(super) fn grouped(keys: &[GroupKey]) -> Vec<u32> {
    if let Some(packing) = Packing::fitting(keys) {
        let mut packed: Vec<u128> = keys
            .iter()
            .zip(0..)
            .map(|(key, position)| packing.pack(key, position))
            .collect();
        packed.sort_unstable();
        return packed
            .into_iter()
            .map(|key| packing.position(key))
            .collect();
    }
    let mut order: Vec<(GroupKey, u32)> = keys.iter().copied().zip(0..).collect();
    order.sort_unstable();
    order.into_iter().map(|(_, position)| position).collect()
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
        let (mut least, mut greatest) = (i32::MAX, i32::MIN);
        for key in keys {
            pipeline = pipeline.max(key.pipeline as u64);
            material = material.max(key.material as u64);
            vertices = vertices.max(u64::from(key.vertices));
            indices = indices.max(u64::from(key.indices));
            instances = instances.max(u64::from(key.instances));
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
                    grouped(&keys),
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
            assert_eq!(grouped(&keys), stable_reference(&keys), "{count} draws");
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
                vertices: [0, u32::MAX][index % 2],
                base_vertex: [i32::MIN, -1, 0, 1, i32::MAX][index % 5],
                indices: [0, u32::MAX][(index / 2) % 2],
                instances: [0, 1][(index / 4) % 2],
            })
            .collect();
        assert!(Packing::fitting(&keys).is_some());
        assert_eq!(grouped(&keys), stable_reference(&keys));
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
        };
        let earlier = GroupKey {
            pipeline: 0,
            ..tied
        };
        assert_eq!(grouped(&[tied, earlier, tied, tied]), [1, 0, 2, 3]);
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
