//! A bounding volume tree over one batch's live slots, so a view rejects whole groups of
//! placements with one box test, and a view tests the placements of a group from boxes that lie
//! in the order of the groups.

use glam::Vec3;

/// Slots per leaf. Leaves are tested slot by slot, so they stay small.
const LEAF_SLOTS: usize = 16;

#[derive(Debug, Default)]
pub(crate) struct Tree {
    /// Depth-first order: the first child of an inner node follows it.
    nodes: Vec<NodeData>,
    /// Live slots in leaf order; each leaf's slots ascend.
    slots: Vec<u32>,
    /// The boxes of each slot of `slots`, `stride` of them, in the same order, so that a leaf's
    /// boxes lie together and not among those of slots that are not in its group.
    boxes: Vec<[Vec3; 2]>,
    stride: usize,
}

#[derive(Clone, Copy, Debug)]
struct NodeData {
    /// Union of the boxes of every slot below the node.
    bounds: [Vec3; 2],
    first: u32,
    count: u32,
    /// Index of the second child; zero for a leaf.
    second: u32,
}

/// One node of a tree: its box and either its children or its slots.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Node<'a> {
    Inner {
        bounds: [Vec3; 2],
        children: [usize; 2],
    },
    Leaf {
        bounds: [Vec3; 2],
        slots: &'a [u32],
        /// The boxes of `slots`, as the tree's `stride` of them for each.
        boxes: &'a [[Vec3; 2]],
    },
}

impl Node<'_> {
    pub fn bounds(self) -> [Vec3; 2] {
        match self {
            Self::Inner { bounds, .. } | Self::Leaf { bounds, .. } => bounds,
        }
    }
}

impl Tree {
    /// Groups `items`, each a slot and the union of its boxes, by splitting at the median
    /// centre along the widest axis until a group fits a leaf. `boxes` holds `stride` boxes for
    /// every slot, by slot, which the tree keeps in its own order.
    pub fn build(
        mut items: Vec<(usize, [Vec3; 2])>,
        (boxes, stride): (&[[Vec3; 2]], usize),
    ) -> Self {
        let mut tree = Self {
            nodes: Vec::with_capacity(2 * items.len().div_ceil(LEAF_SLOTS)),
            slots: Vec::with_capacity(items.len()),
            boxes: Vec::new(),
            stride: stride.max(1),
        };
        if !items.is_empty() {
            tree.split(&mut items);
        }
        tree.boxes = tree
            .slots
            .iter()
            .flat_map(|&slot| &boxes[slot as usize * stride..(slot as usize + 1) * stride])
            .copied()
            .collect();
        tree
    }

    fn split(&mut self, items: &mut [(usize, [Vec3; 2])]) -> usize {
        let index = self.nodes.len();
        let bounds = items
            .iter()
            .map(|(_, bounds)| *bounds)
            .reduce(|[min, max], [low, high]| [min.min(low), max.max(high)])
            .expect("a node has slots");
        self.nodes.push(NodeData {
            bounds,
            first: u32::try_from(self.slots.len()).expect("instance count fits draw range"),
            count: u32::try_from(items.len()).expect("instance count fits draw range"),
            second: 0,
        });
        if items.len() <= LEAF_SLOTS {
            let start = self.slots.len();
            self.slots.extend(
                items
                    .iter()
                    .map(|(slot, _)| u32::try_from(*slot).expect("validated instance slot")),
            );
            self.slots[start..].sort_unstable();
            return index;
        }
        let center = |[min, max]: [Vec3; 2]| (min + max) * 0.5;
        let (low, high) = items.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(low, high), (_, bounds)| (low.min(center(*bounds)), high.max(center(*bounds))),
        );
        let axis = (high - low).max_position();
        let middle = items.len() / 2;
        items.select_nth_unstable_by(middle, |(_, a), (_, b)| {
            center(*a)[axis].total_cmp(&center(*b)[axis])
        });
        let (first, second) = items.split_at_mut(middle);
        self.split(first);
        let second = self.split(second);
        self.nodes[index].second = u32::try_from(second).expect("node count fits u32");
        index
    }

    /// The root node's index; none without live slots.
    pub fn root(&self) -> Option<usize> {
        (!self.nodes.is_empty()).then_some(0)
    }

    pub fn node(&self, index: usize) -> Node<'_> {
        let node = self.nodes[index];
        if node.second == 0 {
            let (first, end) = (node.first as usize, (node.first + node.count) as usize);
            Node::Leaf {
                bounds: node.bounds,
                slots: &self.slots[first..end],
                boxes: &self.boxes[first * self.stride..end * self.stride],
            }
        } else {
            Node::Inner {
                bounds: node.bounds,
                children: [index + 1, node.second as usize],
            }
        }
    }

    /// Number of live slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Every live slot in leaf order. A tree of at most `LEAF_SLOTS` slots is one leaf, so
    /// these are that leaf's slots, ascending.
    pub fn slots(&self) -> &[u32] {
        &self.slots
    }

    /// The boxes of `slots`, `stride` for each.
    pub fn boxes(&self) -> &[[Vec3; 2]] {
        &self.boxes
    }

    /// How many boxes each slot has, at least one.
    pub fn stride(&self) -> usize {
        self.stride
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slot_lands_in_one_leaf_inside_every_box_above_it() {
        let items: Vec<(usize, [Vec3; 2])> = (0..1000)
            .map(|slot| {
                let x = f32::from(u16::try_from((slot * 7919) % 1000).unwrap());
                let y = f32::from(u16::try_from((slot * 104_729) % 997).unwrap());
                let low = Vec3::new(x, y, f32::from(u8::try_from(slot % 13).unwrap()));
                (slot * 2, [low, low + Vec3::splat(1.5)])
            })
            .collect();
        let boxes: Vec<[Vec3; 2]> = (0..items.len() * 2)
            .map(|slot| items.get(slot / 2).map_or([Vec3::ZERO; 2], |item| item.1))
            .collect();
        let tree = Tree::build(items.clone(), (&boxes, 1));
        assert_eq!(tree.len(), items.len());
        let mut seen = Vec::new();
        let mut stack = vec![(tree.root().unwrap(), [Vec3::NEG_INFINITY, Vec3::INFINITY])];
        while let Some((index, [outer_min, outer_max])) = stack.pop() {
            let node = tree.node(index);
            let [min, max] = node.bounds();
            assert!(min.cmpge(outer_min).all() && max.cmple(outer_max).all());
            match node {
                Node::Inner { children, .. } => {
                    stack.extend(children.map(|child| (child, [min, max])));
                }
                Node::Leaf { slots, boxes, .. } => {
                    assert!(slots.len() <= LEAF_SLOTS);
                    assert!(slots.is_sorted());
                    assert_eq!(boxes.len(), slots.len() * tree.stride());
                    for (&slot, stored) in slots.iter().zip(boxes.chunks_exact(tree.stride())) {
                        assert_eq!(stored, [items[slot as usize / 2].1]);
                        let [low, high] = items[slot as usize / 2].1;
                        assert!(low.cmpge(min).all() && high.cmple(max).all());
                        seen.push(slot as usize);
                    }
                }
            }
        }
        seen.sort_unstable();
        assert_eq!(
            seen,
            items.iter().map(|(slot, _)| *slot).collect::<Vec<_>>()
        );
        assert!(Tree::build(Vec::new(), (&[], 1)).root().is_none());
    }
}
