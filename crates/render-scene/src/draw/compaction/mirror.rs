//! One storage buffer holding the instance records of every gathered batch.
//!
//! Each batch keeps its own vertex buffer, and a compute dispatch binds only a handful of
//! buffers, while a view selects from hundreds of batches. The mirror gives every gathered batch
//! a fixed record range in one buffer (and one more for previous transforms, and one for the skin
//! descriptors of skinned batches at the same record indices), so a single dispatch gathers a
//! whole view. A batch's range is copied again only when the batch's content
//! generation moved since the mirror copied it, so a moving view over unchanged placements
//! copies nothing into the mirror.
//!
//! Like the view buffers, the mirror is overwritten in place within a transaction, and what it
//! holds publishes with the transaction, so a cancelled frame leaves the committed state exact.
//!
//! The state is shared, never edited in place: a transaction that changes the layout or a range's
//! content builds its own copy, and one that changes nothing hands on the state it found. A
//! moving view therefore clones no layout and no content records.
//!
//! Its buffers count as instance bytes in the scene's memory census until the last state that
//! holds them (committed or in a transaction) drops.
use std::collections::BTreeMap;
use std::{cell::RefCell, rc::Rc};

use wgpu::Buffer;
use worldrunner_render_gpu::census::{ResourceClass, Tracked};
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::PrepareContext;

use super::{SKIN_STRIDE, STRIDE};
use crate::instances::{Ascending, BatchKey, InstanceBatch, Instances};
use verified::Verified;

mod verified;

/// The record range of one batch in the mirror.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Extent {
    pub base: u32,
    pub reserved: u32,
}

/// Where each gathered batch lives in the mirror. Ranges are handed out at the end; a batch
/// whose buffer outgrew its range moves to the end, and a full mirror is packed again.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Layout {
    pub extents: BTreeMap<BatchKey, Extent>,
    pub end: u32,
    pub capacity: u32,
}

impl Layout {
    /// Gives `key` `reserved` records unless its range already holds that many.
    /// Returns false when the mirror has no room left for it.
    pub fn reserve(&mut self, key: BatchKey, reserved: u32) -> bool {
        if self.holds(key, reserved) {
            return true;
        }
        let Some(end) = self
            .end
            .checked_add(reserved)
            .filter(|end| *end <= self.capacity)
        else {
            return false;
        };
        self.extents.insert(
            key,
            Extent {
                base: self.end,
                reserved,
            },
        );
        self.end = end;
        true
    }

    /// Whether `key`'s range already holds `reserved` records.
    fn holds(&self, key: BatchKey, reserved: u32) -> bool {
        self.extents
            .get(&key)
            .is_some_and(|extent| extent.reserved >= reserved)
    }

    /// Packs `batches` from record zero in key order into the next power of two of their total.
    pub fn packed(batches: &BTreeMap<BatchKey, u32>) -> Self {
        let total: u64 = batches.values().map(|reserved| u64::from(*reserved)).sum();
        let capacity = total
            .checked_next_power_of_two()
            .and_then(|capacity| u32::try_from(capacity).ok())
            .expect("gather mirror records fit u32");
        let mut layout = Self {
            capacity,
            ..Self::default()
        };
        for (&key, &reserved) in batches {
            assert!(layout.reserve(key, reserved), "packed layout fits");
        }
        layout
    }

    /// The first record of `key`'s range, found by a search.
    pub fn base(&self, key: BatchKey) -> u32 {
        self.extents[&key].base
    }

    /// Looks up the bases of `wanted` batches asked for in ascending key order.
    pub fn bases(&self, wanted: usize) -> Bases<'_> {
        Bases(Ascending::new(&self.extents, wanted))
    }
}

/// The bases of a layout's ranges for keys in ascending order, one walk along the layout for a
/// view that names many of its batches.
pub(super) struct Bases<'a>(Ascending<'a, Extent>);

impl Bases<'_> {
    /// The first record of `key`'s range, which is not below any key asked for before.
    pub fn base(&mut self, key: BatchKey) -> u32 {
        self.0
            .get(&key)
            .expect("the mirror holds every gathered batch")
            .base
    }
}

/// The batch content generation a mirror range holds, and whether it holds previous transforms
/// and skin descriptors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    content: u64,
    previous: bool,
    skin: bool,
}

/// Which halves beside the current records a gather reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Halves {
    pub previous: bool,
    /// The skin descriptors of skinned batches; set while the scene has skinned batches.
    pub skin: bool,
}

/// The mirror buffers; `previous` exists once a gather needed previous transforms, and `skin`
/// once a gather needed skin descriptors.
#[derive(Clone, Debug)]
pub(super) struct Buffers {
    pub current: Rc<Tracked<Buffer>>,
    pub previous: Option<Rc<Tracked<Buffer>>>,
    /// One 16-byte descriptor per record, at the record's index.
    pub skin: Option<Rc<Tracked<Buffer>>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub layout: Layout,
    pub buffers: Option<Buffers>,
    held: BTreeMap<BatchKey, Held>,
    /// The batches this state was last found to hold at their current content.
    verified: Verified,
}

impl State {
    /// The record of what copying `batch` into its range would leave there, or `None` when the
    /// range already holds the batch's current content.
    fn stale(held: Option<&Held>, batch: &InstanceBatch, halves: Halves) -> Option<Held> {
        let wanted = Held {
            content: batch.content(),
            previous: halves.previous,
            skin: halves.skin && batch.skin.is_some(),
        };
        let current = held.is_some_and(|held| {
            held.content == wanted.content
                && (held.previous || !wanted.previous)
                && (held.skin || !wanted.skin)
        });
        (!current).then_some(wanted)
    }
}

/// Committed and in-transaction mirror state, like a view's compaction. A state is replaced,
/// never edited, once it is committed or staged for publication.
#[derive(Debug, Default)]
pub(super) struct Mirror {
    pub committed: Rc<RefCell<Rc<State>>>,
    drawing: RefCell<Option<(Publications, Rc<State>)>>,
    /// How many times `prepare` checked batch ranges one by one.
    #[cfg(test)]
    scans: std::cell::Cell<usize>,
    /// How many batch ranges those checks started from: the keys the state was not known to hold.
    #[cfg(test)]
    checked: std::cell::Cell<usize>,
}

/// What a test can compare of a mirror state: its layout, what each range holds, and which
/// buffers back it.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Snapshot {
    layout: Layout,
    held: BTreeMap<BatchKey, Held>,
    buffers: Option<[Option<usize>; 3]>,
}

#[cfg(test)]
impl Snapshot {
    /// Batches whose ranges hold content.
    pub fn batches(&self) -> usize {
        self.held.len()
    }
}

#[cfg(test)]
impl Mirror {
    pub fn scans(&self) -> usize {
        self.scans.get()
    }

    pub fn checked(&self) -> usize {
        self.checked.get()
    }

    pub fn snapshot(&self) -> Snapshot {
        let committed = Rc::clone(&self.committed.borrow());
        let identity = |buffer: &Rc<Tracked<Buffer>>| Rc::as_ptr(buffer) as usize;
        Snapshot {
            layout: committed.layout.clone(),
            held: committed.held.clone(),
            buffers: committed.buffers.as_ref().map(|buffers| {
                [
                    Some(identity(&buffers.current)),
                    buffers.previous.as_ref().map(identity),
                    buffers.skin.as_ref().map(identity),
                ]
            }),
        }
    }
}

/// Records a batch's buffer can hold.
fn reserved(buffer: &Buffer) -> u32 {
    u32::try_from(buffer.size() / STRIDE).expect("instance batch records fit u32")
}

impl Mirror {
    fn state_in(&self, transaction: &Publications) -> Rc<State> {
        self.drawing
            .borrow()
            .as_ref()
            .filter(|(owner, _)| owner.same_transaction(transaction))
            .map_or_else(
                || Rc::clone(&self.committed.borrow()),
                |(_, state)| Rc::clone(state),
            )
    }

    /// Brings the ranges of `keys`, which the caller lists in ascending order, up to their
    /// batches' content, staging the copies in `context`, and returns the mirror state the
    /// gather reads. Every key must name a batch with a flushed buffer, with a previous buffer
    /// when `halves.previous`, and with a flushed skin stream when the batch is skinned.
    /// `generation` is `instances.content_generation()`. A state that already passed these keys at
    /// that generation is returned without checking any batch, and one that passed all but a few
    /// of them is checked for those few.
    pub fn prepare(
        &self,
        context: &mut PrepareContext<'_>,
        instances: &Instances,
        keys: &[BatchKey],
        (halves, generation): (Halves, u64),
    ) -> Rc<State> {
        debug_assert_eq!(generation, instances.content_generation());
        let owner = context.publications();
        let found = self.state_in(&owner);
        let unremembered = found.verified.missing(generation, halves, keys);
        let state = if unremembered.is_empty() {
            found
        } else {
            #[cfg(test)]
            {
                self.scans.set(self.scans.get() + 1);
                self.checked.set(self.checked.get() + unremembered.len());
            }
            Self::brought_up_to_date(
                context,
                found,
                (instances, keys, &unremembered),
                (halves, generation),
            )
        };
        *self.drawing.borrow_mut() = Some((owner, Rc::clone(&state)));
        let committed = self.committed.clone();
        let published = Rc::clone(&state);
        crate::compositor::publication::stage(context, &committed.clone(), move || {
            *committed.borrow_mut() = published;
        });
        state
    }

    /// `found` with the ranges of `keys` copied up to their batches' content. `unremembered` are
    /// the keys `found` was not known to hold; a relayout leaves it holding none of them.
    fn brought_up_to_date(
        context: &mut PrepareContext<'_>,
        found: Rc<State>,
        (instances, keys, unremembered): (&Instances, &[BatchKey], &[BatchKey]),
        (halves, generation): (Halves, u64),
    ) -> Rc<State> {
        let mut state = Self::laid_out(
            context.device(),
            found,
            (instances, keys, unremembered),
            halves,
        );
        let unchecked = state.verified.missing(generation, halves, keys);
        let outdated: Vec<(BatchKey, &InstanceBatch, Held)> = {
            let mut batches = Ascending::new(&instances.batches, unchecked.len());
            let mut held = Ascending::new(&state.held, unchecked.len());
            unchecked
                .iter()
                .filter_map(|key| {
                    let batch = batches.get(key).expect("a selected batch exists");
                    State::stale(held.get(key), batch, halves).map(|wanted| (*key, batch, wanted))
                })
                .collect()
        };
        for (key, batch, wanted) in outdated {
            // The state is shared with the committed one and with earlier publications, so
            // the first edit of a transaction copies it.
            let editable = Rc::make_mut(&mut state);
            Self::refresh(context, editable, (key, batch), wanted);
        }
        state.verified.remember(generation, halves, &unchecked);
        state
    }

    /// `state` when its ranges already hold `unremembered`, otherwise a copy with them appended
    /// or, when the mirror is full or lacks a half, a packed relayout in new buffers.
    fn laid_out(
        device: &wgpu::Device,
        state: Rc<State>,
        (instances, keys, unremembered): (&Instances, &[BatchKey], &[BatchKey]),
        halves: Halves,
    ) -> Rc<State> {
        let batches = &instances.batches;
        let halves_exist = state.buffers.as_ref().is_some_and(|buffers| {
            (!halves.previous || buffers.previous.is_some())
                && (!halves.skin || buffers.skin.is_some())
        });
        let outgrown: Vec<(BatchKey, u32)> = {
            let mut sources = Ascending::new(batches, unremembered.len());
            let mut extents = Ascending::new(&state.layout.extents, unremembered.len());
            unremembered
                .iter()
                .filter_map(|key| {
                    let buffer = sources
                        .get(key)
                        .and_then(|batch| batch.buffer.as_deref())
                        .expect("flushed instance buffer");
                    let records = reserved(buffer);
                    let holds = extents
                        .get(key)
                        .is_some_and(|extent| extent.reserved >= records);
                    (!holds).then_some((*key, records))
                })
                .collect()
        };
        if halves_exist && outgrown.is_empty() {
            return state;
        }
        let needed: u64 = outgrown
            .iter()
            .map(|&(_, records)| u64::from(records))
            .sum();
        let room = u64::from(state.layout.capacity - state.layout.end);
        if !halves_exist || needed > room {
            return Rc::new(Self::allocate(device, &state, instances, keys, halves));
        }
        let mut state = state;
        let editable = Rc::make_mut(&mut state);
        for (key, records) in outgrown {
            // A batch that moved to the end holds nothing at its new range yet.
            if editable.layout.extents.contains_key(&key) {
                editable.held.remove(&key);
                editable.verified.forget(key);
            }
            assert!(
                editable.layout.reserve(key, records),
                "the room was checked"
            );
        }
        state
    }

    /// A packed layout of every batch the mirror held and `keys`, in new buffers that hold
    /// nothing yet. A replaced buffer is never an in-place copy target.
    fn allocate(
        device: &wgpu::Device,
        state: &State,
        instances: &Instances,
        keys: &[BatchKey],
        halves: Halves,
    ) -> State {
        let batches = &instances.batches;
        let sizes: BTreeMap<BatchKey, u32> = state
            .layout
            .extents
            .keys()
            .chain(keys)
            .map(|&key| (key, reserved(flushed(batches, key))))
            .collect();
        let layout = Layout::packed(&sizes);
        let bytes = u64::from(layout.capacity) * STRIDE;
        let limits = device.limits();
        assert!(
            bytes <= limits.max_buffer_size && bytes <= limits.max_storage_buffer_binding_size,
            "gather mirror of {bytes} bytes exceeds the device's storage buffer limits"
        );
        let create = |label, size| {
            Rc::new(instances.census.create_buffer(
                device,
                ResourceClass::Instance,
                &wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            ))
        };
        let kept = |half: fn(&Buffers) -> bool| state.buffers.as_ref().is_some_and(half);
        let previous = halves.previous || kept(|buffers| buffers.previous.is_some());
        let skin = halves.skin || kept(|buffers| buffers.skin.is_some());
        let skin_bytes =
            (u64::from(layout.capacity) * SKIN_STRIDE).max(wgpu::COPY_BUFFER_ALIGNMENT);
        State {
            layout,
            buffers: Some(Buffers {
                current: create("gather mirror", bytes),
                previous: previous.then(|| create("gather previous mirror", bytes)),
                skin: skin.then(|| create("gather skin mirror", skin_bytes)),
            }),
            held: BTreeMap::new(),
            verified: Verified::default(),
        }
    }

    /// Copies a batch into its range, which [`State::stale`] found to lack `held`.
    fn refresh(
        context: &mut PrepareContext<'_>,
        state: &mut State,
        (key, batch): (BatchKey, &InstanceBatch),
        held: Held,
    ) {
        let (previous, skin) = (held.previous, held.skin);
        let buffers = state.buffers.as_ref().expect("allocated gather mirror");
        let base = u64::from(state.layout.base(key)) * STRIDE;
        let current = batch.buffer.as_deref().expect("flushed instance buffer");
        let records = (batch.bytes.len() as u64 / STRIDE).min(u64::from(reserved(current)));
        context.copy_buffer_region((current, 0), (&**buffers.current, base), records * STRIDE);
        if previous {
            let source = batch.previous.as_deref().expect("flushed previous buffer");
            let target: &Buffer = buffers
                .previous
                .as_deref()
                .expect("allocated previous mirror");
            let records = records.min(u64::from(reserved(source)));
            context.copy_buffer_region((source, 0), (target, base), records * STRIDE);
        }
        if skin {
            let stream = batch
                .skin
                .as_ref()
                .expect("a skinned batch has a skin stream");
            let source = stream.buffer.as_deref().expect("flushed skin stream");
            let target: &Buffer = buffers.skin.as_deref().expect("allocated skin mirror");
            // The stream holds the same slots as the instance buffer, a descriptor each.
            let records = records.min(source.size() / SKIN_STRIDE);
            let base = u64::from(state.layout.base(key)) * SKIN_STRIDE;
            context.copy_buffer_region((source, 0), (target, base), records * SKIN_STRIDE);
        }
        state.held.insert(key, held);
    }
}

fn flushed(batches: &BTreeMap<BatchKey, InstanceBatch>, key: BatchKey) -> &Buffer {
    batches[&key]
        .buffer
        .as_deref()
        .expect("flushed instance buffer")
}
