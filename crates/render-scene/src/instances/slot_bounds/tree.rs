//! A bounding volume tree over one batch's live slots, so a view rejects whole groups of
//! placements with one box test, and a view tests the placements of a group from boxes that lie
//! in the order of the groups.
//!
//! The tree is built once from the live slots and then follows slots as they come and go: a
//! removed slot leaves its leaf and a placed one joins the leaf its box suits, splitting it when
//! it overflows, so a change costs in the depth of the tree and not in the size of the batch.
//! The tree is built afresh when the changes since it was built outnumber its slots, which
//! restores the balance that changes erode.

use glam::Vec3;

/// Slots per leaf. Leaves are tested slot by slot, so they stay small.
const LEAF_SLOTS: usize = 16;

const NONE: u32 = u32::MAX;

#[derive(Debug, Default)]
pub(crate) struct Tree {
    nodes: Vec<NodeData>,
    free_nodes: Vec<u32>,
    root: Option<u32>,
    /// One block of `LEAF_SLOTS` entries for each leaf, of which the first `count` hold the
    /// leaf's slots in ascending order.
    slots: Vec<u32>,
    /// The boxes of each entry of `slots`, `stride` of them, in the same order, so that a leaf's
    /// boxes lie together and not among those of slots that are not in its group.
    boxes: Vec<[Vec3; 2]>,
    /// Where the blocks of `slots` that no leaf uses begin.
    free_blocks: Vec<u32>,
    /// The leaf of each slot, `NONE` for a slot that is not in the tree.
    leaf_of: Vec<u32>,
    stride: usize,
    live: usize,
    /// How many slots the last build grouped, and how many slots have joined or left since.
    built: usize,
    changes: usize,
}

#[derive(Clone, Copy, Debug)]
struct NodeData {
    /// Union of the boxes of every slot below the node.
    bounds: [Vec3; 2],
    parent: u32,
    /// Live slots below the node. Every inner node has more than `LEAF_SLOTS` of them, so a
    /// tree of at most that many slots is one leaf.
    count: u32,
    /// For a leaf, where its block of `slots` begins.
    first: u32,
    /// Both `NONE` for a leaf.
    children: [u32; 2],
}

impl NodeData {
    fn is_leaf(&self) -> bool {
        self.children[0] == NONE
    }
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

fn count(value: usize) -> u32 {
    u32::try_from(value).expect("instance count fits draw range")
}

fn union([min, max]: [Vec3; 2], [low, high]: [Vec3; 2]) -> [Vec3; 2] {
    [min.min(low), max.max(high)]
}

/// The box around every box of the slot in `boxes`, which hold `stride` boxes for each slot.
fn slot_union(boxes: &[[Vec3; 2]], stride: usize, slot: usize) -> [Vec3; 2] {
    boxes[slot * stride..(slot + 1) * stride]
        .iter()
        .copied()
        .reduce(union)
        .expect("a slot has at least one box")
}

/// Half the surface of the box, the measure of how much a ray test of the box costs.
fn area([min, max]: [Vec3; 2]) -> f32 {
    let size = max - min;
    size.x * size.y + size.y * size.z + size.z * size.x
}

/// Orders `items` so that those before the returned position are the ones whose centres lie
/// below the median along the widest axis of the centres, and returns that position.
fn halve(items: &mut [(usize, [Vec3; 2])]) -> usize {
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
    middle
}

impl Tree {
    /// Groups `slots` by splitting at the median centre along the widest axis until a group fits
    /// a leaf. `boxes` holds `stride` boxes for every slot, by slot, which the tree keeps in its
    /// own order.
    pub fn build(slots: Vec<usize>, boxes: &[[Vec3; 2]], stride: usize) -> Self {
        let stride = stride.max(1);
        let mut items: Vec<_> = slots
            .into_iter()
            .map(|slot| (slot, slot_union(boxes, stride, slot)))
            .collect();
        let mut tree = Self {
            stride,
            live: items.len(),
            built: items.len(),
            ..Self::default()
        };
        tree.nodes.reserve(2 * items.len().div_ceil(LEAF_SLOTS));
        if !items.is_empty() {
            tree.root = Some(tree.split(&mut items, NONE, boxes));
        }
        tree
    }

    fn split(&mut self, items: &mut [(usize, [Vec3; 2])], parent: u32, boxes: &[[Vec3; 2]]) -> u32 {
        let bounds = items
            .iter()
            .map(|(_, bounds)| *bounds)
            .reduce(union)
            .expect("a node has slots");
        let node = self.add_node(NodeData {
            bounds,
            parent,
            count: count(items.len()),
            first: NONE,
            children: [NONE; 2],
        });
        if items.len() <= LEAF_SLOTS {
            let mut slots: Vec<u32> = items.iter().map(|&(slot, _)| count(slot)).collect();
            slots.sort_unstable();
            self.fill_leaf(node, &slots, boxes);
            return node;
        }
        let middle = halve(items);
        let (low, high) = items.split_at_mut(middle);
        let first = self.split(low, node, boxes);
        let second = self.split(high, node, boxes);
        self.nodes[node as usize].children = [first, second];
        node
    }

    /// Whether the slots that joined or left since the build outnumber those it grouped, past
    /// which a build afresh restores what the changes have worn from the grouping.
    pub fn is_degraded(&self) -> bool {
        self.changes > self.built.max(LEAF_SLOTS)
    }

    /// Takes `slot`, whose boxes `boxes` hold as they do for `build`, into the leaf its box
    /// suits. The slot must not be in the tree.
    pub fn insert(&mut self, slot: usize, boxes: &[[Vec3; 2]]) {
        debug_assert!(
            self.leaf_of.get(slot).is_none_or(|&leaf| leaf == NONE),
            "a slot joins the tree once"
        );
        let bounds = slot_union(boxes, self.stride, slot);
        self.live += 1;
        self.changes += 1;
        let Some(mut node) = self.root else {
            let leaf = self.add_node(NodeData {
                bounds,
                parent: NONE,
                count: 1,
                first: NONE,
                children: [NONE; 2],
            });
            self.fill_leaf(leaf, &[count(slot)], boxes);
            self.root = Some(leaf);
            return;
        };
        loop {
            let data = &mut self.nodes[node as usize];
            data.bounds = union(data.bounds, bounds);
            data.count += 1;
            if data.is_leaf() {
                break;
            }
            node = self.cheaper_child(node, bounds);
        }
        if self.nodes[node as usize].count as usize <= LEAF_SLOTS {
            self.insert_in_leaf(node, slot, boxes);
        } else {
            self.split_leaf(node, slot, boxes);
        }
    }

    /// The child of the inner `node` that grows least, by area, to take `bounds`.
    fn cheaper_child(&self, node: u32, bounds: [Vec3; 2]) -> u32 {
        let [first, second] = self.nodes[node as usize].children;
        let growth = |child: u32| {
            let child = &self.nodes[child as usize];
            area(union(child.bounds, bounds)) - area(child.bounds)
        };
        let fewer = self.nodes[first as usize]
            .count
            .cmp(&self.nodes[second as usize].count);
        if growth(first).total_cmp(&growth(second)).then(fewer).is_le() {
            first
        } else {
            second
        }
    }

    /// Places `slot` in its ascending position in `leaf`, which has room and whose count
    /// already includes it.
    fn insert_in_leaf(&mut self, leaf: u32, slot: usize, boxes: &[[Vec3; 2]]) {
        let stride = self.stride;
        let first = self.nodes[leaf as usize].first as usize;
        let used = self.nodes[leaf as usize].count as usize - 1;
        let at =
            first + self.slots[first..first + used].partition_point(|&other| other < count(slot));
        self.slots.copy_within(at..first + used, at + 1);
        self.boxes
            .copy_within(at * stride..(first + used) * stride, (at + 1) * stride);
        self.slots[at] = count(slot);
        self.boxes[at * stride..(at + 1) * stride]
            .copy_from_slice(&boxes[slot * stride..(slot + 1) * stride]);
        self.set_leaf(slot, leaf);
    }

    /// Turns the full `leaf` into an inner node over two leaves that share its slots and `slot`.
    fn split_leaf(&mut self, leaf: u32, slot: usize, boxes: &[[Vec3; 2]]) {
        let stride = self.stride;
        let first = self.nodes[leaf as usize].first as usize;
        let mut items: Vec<_> = self.slots[first..first + LEAF_SLOTS]
            .iter()
            .map(|&other| other as usize)
            .chain(std::iter::once(slot))
            .map(|slot| (slot, slot_union(boxes, stride, slot)))
            .collect();
        self.free_blocks.push(count(first));
        let middle = halve(&mut items);
        let children = [&items[..middle], &items[middle..]].map(|half| {
            let bounds = half.iter().map(|&(_, bounds)| bounds).reduce(union);
            let mut slots: Vec<u32> = half.iter().map(|&(slot, _)| count(slot)).collect();
            slots.sort_unstable();
            let child = self.add_node(NodeData {
                bounds: bounds.expect("a half has slots"),
                parent: leaf,
                count: count(slots.len()),
                first: NONE,
                children: [NONE; 2],
            });
            self.fill_leaf(child, &slots, boxes);
            child
        });
        let node = &mut self.nodes[leaf as usize];
        node.children = children;
        node.first = NONE;
    }

    /// Drops `slot` from the tree if it is in it.
    pub fn remove(&mut self, slot: usize) {
        let Some(&leaf) = self.leaf_of.get(slot).filter(|&&leaf| leaf != NONE) else {
            return;
        };
        self.leaf_of[slot] = NONE;
        self.live -= 1;
        self.changes += 1;
        let stride = self.stride;
        let first = self.nodes[leaf as usize].first as usize;
        let used = self.nodes[leaf as usize].count as usize;
        let at = first
            + self.slots[first..first + used]
                .binary_search(&count(slot))
                .expect("a leaf lists its slots");
        self.slots.copy_within(at + 1..first + used, at);
        self.boxes
            .copy_within((at + 1) * stride..(first + used) * stride, at * stride);
        let mut above = leaf;
        while above != NONE {
            self.nodes[above as usize].count -= 1;
            above = self.nodes[above as usize].parent;
        }
        let from = if used == 1 {
            self.detach(leaf)
        } else {
            let bounds = self.leaf_bounds(leaf);
            if bounds != self.nodes[leaf as usize].bounds {
                self.nodes[leaf as usize].bounds = bounds;
                self.refit(self.nodes[leaf as usize].parent);
            }
            leaf
        };
        self.merge_small_subtree(from);
    }

    /// Removes the empty `leaf`, which takes its parent's place by its sibling, and returns the
    /// node now in the parent's place, or `NONE` when the tree is empty.
    fn detach(&mut self, leaf: u32) -> u32 {
        let NodeData { parent, first, .. } = self.nodes[leaf as usize];
        if parent == NONE {
            let stride = self.stride;
            *self = Self {
                stride,
                ..Self::default()
            };
            return NONE;
        }
        self.free_blocks.push(first);
        self.free_nodes.push(leaf);
        let [a, b] = self.nodes[parent as usize].children;
        let sibling = if a == leaf { b } else { a };
        let grandparent = self.nodes[parent as usize].parent;
        self.nodes[sibling as usize].parent = grandparent;
        if grandparent == NONE {
            self.root = Some(sibling);
        } else {
            for child in &mut self.nodes[grandparent as usize].children {
                if *child == parent {
                    *child = sibling;
                }
            }
        }
        self.free_nodes.push(parent);
        self.refit(grandparent);
        sibling
    }

    /// Recomputes the boxes of `node` and the nodes above it from their children, up to the
    /// first that does not change.
    fn refit(&mut self, mut node: u32) {
        while node != NONE {
            let [first, second] = self.nodes[node as usize].children;
            let bounds = union(
                self.nodes[first as usize].bounds,
                self.nodes[second as usize].bounds,
            );
            if bounds == self.nodes[node as usize].bounds {
                break;
            }
            self.nodes[node as usize].bounds = bounds;
            node = self.nodes[node as usize].parent;
        }
    }

    /// Makes one leaf of the largest subtree at or above `from` whose slots fit a leaf.
    fn merge_small_subtree(&mut self, from: u32) {
        let mut top = None;
        let mut node = from;
        while node != NONE && self.nodes[node as usize].count as usize <= LEAF_SLOTS {
            if !self.nodes[node as usize].is_leaf() {
                top = Some(node);
            }
            node = self.nodes[node as usize].parent;
        }
        let Some(top) = top else {
            return;
        };
        let stride = self.stride;
        let mut entries: Vec<(u32, usize)> = Vec::with_capacity(LEAF_SLOTS);
        let mut blocks = Vec::new();
        let mut stack = vec![top];
        while let Some(node) = stack.pop() {
            let data = self.nodes[node as usize];
            if data.is_leaf() {
                let first = data.first as usize;
                entries.extend(
                    (first..first + data.count as usize).map(|entry| (self.slots[entry], entry)),
                );
                blocks.push(data.first);
            } else {
                stack.extend(data.children);
            }
            if node != top {
                self.free_nodes.push(node);
            }
        }
        entries.sort_unstable();
        // The new block is taken before the old ones are free, so that it is not one of them.
        let first = self.alloc_block() as usize;
        for (at, &(slot, entry)) in entries.iter().enumerate() {
            self.slots[first + at] = slot;
            self.boxes
                .copy_within(entry * stride..(entry + 1) * stride, (first + at) * stride);
            self.set_leaf(slot as usize, top);
        }
        self.free_blocks.extend(blocks);
        let node = &mut self.nodes[top as usize];
        node.children = [NONE; 2];
        node.first = count(first);
    }

    fn add_node(&mut self, node: NodeData) -> u32 {
        if let Some(free) = self.free_nodes.pop() {
            self.nodes[free as usize] = node;
            free
        } else {
            self.nodes.push(node);
            count(self.nodes.len() - 1)
        }
    }

    /// Where a block of `LEAF_SLOTS` entries of `slots` and their boxes begins.
    fn alloc_block(&mut self) -> u32 {
        self.free_blocks.pop().unwrap_or_else(|| {
            let first = self.slots.len();
            self.slots.resize(first + LEAF_SLOTS, 0);
            self.boxes
                .resize((first + LEAF_SLOTS) * self.stride, [Vec3::ZERO; 2]);
            count(first)
        })
    }

    fn set_leaf(&mut self, slot: usize, leaf: u32) {
        if slot >= self.leaf_of.len() {
            self.leaf_of.resize(slot + 1, NONE);
        }
        self.leaf_of[slot] = leaf;
    }

    /// Gives `leaf` a block holding `slots`, which ascend, and their boxes from `boxes`.
    fn fill_leaf(&mut self, leaf: u32, slots: &[u32], boxes: &[[Vec3; 2]]) {
        let stride = self.stride;
        let first = self.alloc_block();
        for (at, &slot) in slots.iter().enumerate() {
            let entry = first as usize + at;
            self.slots[entry] = slot;
            self.boxes[entry * stride..(entry + 1) * stride]
                .copy_from_slice(&boxes[slot as usize * stride..(slot as usize + 1) * stride]);
            self.set_leaf(slot as usize, leaf);
        }
        self.nodes[leaf as usize].first = first;
    }

    /// The box around the slots of `leaf`.
    fn leaf_bounds(&self, leaf: u32) -> [Vec3; 2] {
        let NodeData { first, count, .. } = self.nodes[leaf as usize];
        let (first, end) = (first as usize, (first + count) as usize);
        self.boxes[first * self.stride..end * self.stride]
            .iter()
            .copied()
            .reduce(union)
            .expect("a leaf has slots")
    }

    /// The root node's index; none without live slots.
    pub fn root(&self) -> Option<usize> {
        self.root.map(|root| root as usize)
    }

    pub fn node(&self, index: usize) -> Node<'_> {
        let node = self.nodes[index];
        if node.is_leaf() {
            let (first, end) = (node.first as usize, (node.first + node.count) as usize);
            Node::Leaf {
                bounds: node.bounds,
                slots: &self.slots[first..end],
                boxes: &self.boxes[first * self.stride..end * self.stride],
            }
        } else {
            Node::Inner {
                bounds: node.bounds,
                children: node.children.map(|child| child as usize),
            }
        }
    }

    /// Number of live slots.
    pub fn len(&self) -> usize {
        self.live
    }

    /// The live slots of a tree of at most `LEAF_SLOTS` slots, which is one leaf, ascending.
    pub fn slots(&self) -> &[u32] {
        match self.root.map(|root| self.node(root as usize)) {
            None => &[],
            Some(Node::Leaf { slots, .. }) => slots,
            Some(Node::Inner { .. }) => panic!("a tree of several leaves has no single list"),
        }
    }

    /// The boxes of `slots`, `stride` for each.
    pub fn boxes(&self) -> &[[Vec3; 2]] {
        match self.root.map(|root| self.node(root as usize)) {
            None => &[],
            Some(Node::Leaf { boxes, .. }) => boxes,
            Some(Node::Inner { .. }) => panic!("a tree of several leaves has no single list"),
        }
    }

    /// How many boxes each slot has, at least one.
    pub fn stride(&self) -> usize {
        self.stride
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const SLOTS: usize = 500;

    struct Scatter(u64);

    impl Scatter {
        fn next(&mut self, below: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(self.0 >> 33).unwrap() % below
        }

        /// A slot's boxes, `stride` of them, around a point of a 40-by-40 field.
        fn boxes(&mut self, stride: usize) -> Vec<[Vec3; 2]> {
            let x = f32::from(u8::try_from(self.next(40)).unwrap());
            let y = f32::from(u8::try_from(self.next(40)).unwrap());
            (0..stride)
                .map(|scale| {
                    let low = Vec3::new(x, y, f32::from(u8::try_from(self.next(5)).unwrap()));
                    [
                        low,
                        low + Vec3::splat(0.5 + f32::from(u8::try_from(scale).unwrap())),
                    ]
                })
                .collect()
        }
    }

    /// Walks the tree through its public nodes and checks what a view relies on: every live slot
    /// is in one leaf of at most `LEAF_SLOTS` ascending slots with its own boxes, every box is
    /// the box around what lies below it, and a tree that fits a leaf is one.
    fn assert_groups(tree: &Tree, table: &[[Vec3; 2]], live: &BTreeSet<usize>) {
        let stride = tree.stride();
        assert_eq!(tree.len(), live.len());
        let mut seen = Vec::new();
        let mut stack: Vec<usize> = tree.root().into_iter().collect();
        let mut leaves = 0;
        let mut around = std::collections::BTreeMap::new();
        let mut order = Vec::new();
        while let Some(index) = stack.pop() {
            order.push(index);
            match tree.node(index) {
                Node::Inner { children, .. } => stack.extend(children),
                Node::Leaf {
                    bounds,
                    slots,
                    boxes,
                } => {
                    leaves += 1;
                    assert!(!slots.is_empty() && slots.len() <= LEAF_SLOTS);
                    assert!(slots.is_sorted());
                    assert_eq!(boxes.len(), slots.len() * stride);
                    let mut all = Vec::new();
                    for (&slot, stored) in slots.iter().zip(boxes.chunks_exact(stride)) {
                        let slot = slot as usize;
                        assert_eq!(stored, &table[slot * stride..(slot + 1) * stride]);
                        all.extend_from_slice(stored);
                        seen.push(slot);
                    }
                    assert_eq!(bounds, all.into_iter().reduce(union).unwrap());
                    around.insert(index, bounds);
                }
            }
        }
        for &index in order.iter().rev() {
            if let Node::Inner { bounds, children } = tree.node(index) {
                assert_eq!(
                    bounds,
                    union(
                        tree.node(children[0]).bounds(),
                        tree.node(children[1]).bounds()
                    )
                );
                around.insert(index, bounds);
            }
        }
        seen.sort_unstable();
        assert_eq!(seen, live.iter().copied().collect::<Vec<_>>());
        if live.len() <= LEAF_SLOTS {
            assert_eq!(leaves, usize::from(!live.is_empty()));
            assert_eq!(tree.slots().len(), live.len());
            assert_eq!(tree.boxes().len(), live.len() * stride);
        }
    }

    #[test]
    fn built_tree_groups_every_slot_in_one_leaf_inside_every_box_above_it() {
        for stride in [1, 3] {
            let mut scatter = Scatter(7);
            let table: Vec<_> = (0..SLOTS).flat_map(|_| scatter.boxes(stride)).collect();
            let live: BTreeSet<usize> = (0..SLOTS).filter(|slot| slot % 3 != 0).collect();
            let tree = Tree::build(live.iter().copied().collect(), &table, stride);
            assert_groups(&tree, &table, &live);
            assert!(!tree.is_degraded());
        }
        let empty = Tree::build(Vec::new(), &[], 1);
        assert!(empty.root().is_none());
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn slots_joining_leaving_and_moving_keep_the_grouping_exact() {
        for (stride, grow_to) in [(1, SLOTS), (2, 12), (3, 60)] {
            let mut scatter = Scatter(11 + u64::try_from(stride).unwrap());
            let mut table: Vec<_> = (0..SLOTS).flat_map(|_| scatter.boxes(stride)).collect();
            let mut live: BTreeSet<usize> = (0..grow_to / 2).collect();
            let mut tree = Tree::build(live.iter().copied().collect(), &table, stride);
            for step in 0..4000 {
                let slot = scatter.next(grow_to.min(SLOTS));
                let boxes = scatter.boxes(stride);
                match (live.contains(&slot), scatter.next(3)) {
                    (false, _) => {
                        table[slot * stride..(slot + 1) * stride].copy_from_slice(&boxes);
                        tree.insert(slot, &table);
                        live.insert(slot);
                    }
                    (true, 0) => {
                        tree.remove(slot);
                        live.remove(&slot);
                    }
                    (true, _) => {
                        tree.remove(slot);
                        table[slot * stride..(slot + 1) * stride].copy_from_slice(&boxes);
                        tree.insert(slot, &table);
                    }
                }
                if step % 7 == 0 {
                    assert_groups(&tree, &table, &live);
                }
            }
            assert_groups(&tree, &table, &live);
            assert!(tree.is_degraded());
            let rebuilt = Tree::build(live.iter().copied().collect(), &table, stride);
            assert_groups(&rebuilt, &table, &live);
            assert!(!rebuilt.is_degraded());
        }
    }

    #[test]
    fn removing_every_slot_empties_the_tree_and_it_takes_slots_again() {
        let mut scatter = Scatter(3);
        let table: Vec<_> = (0..100).flat_map(|_| scatter.boxes(1)).collect();
        let mut live: BTreeSet<usize> = (0..100).collect();
        let mut tree = Tree::build(live.iter().copied().collect(), &table, 1);
        for slot in (0..100).rev() {
            tree.remove(slot);
            live.remove(&slot);
            assert_groups(&tree, &table, &live);
        }
        assert!(tree.root().is_none());
        tree.remove(5);
        for slot in 0..40 {
            tree.insert(slot, &table);
            live.insert(slot);
        }
        assert_groups(&tree, &table, &live);
    }
}
