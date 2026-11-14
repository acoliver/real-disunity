//! One storage buffer holding the instance records of every gathered batch.
//!
//! Each batch keeps its own vertex buffer, and a compute dispatch binds only a handful of
//! buffers, while a view selects from hundreds of batches. The mirror gives every gathered batch
//! a fixed record range in one buffer (and one more for previous transforms), so a single
//! dispatch gathers a whole view. A batch's range is copied again only when the batch's content
//! generation moved since the mirror copied it, so a moving view over unchanged placements
//! copies nothing into the mirror.
//!
//! Like the view buffers, the mirror is overwritten in place within a transaction, and what it
//! holds publishes with the transaction, so a cancelled frame leaves the committed state exact.
//!
//! Its buffers count as instance bytes in the scene's memory census until the last state that
//! holds them (committed or in a transaction) drops.
use std::collections::BTreeMap;
use std::{cell::RefCell, rc::Rc};

use wgpu::Buffer;
use worldrunner_render_gpu::census::{ResourceClass, Tracked};
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::PrepareContext;

use super::STRIDE;
use crate::instances::{BatchKey, InstanceBatch, Instances};

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
        if self
            .extents
            .get(&key)
            .is_some_and(|extent| extent.reserved >= reserved)
        {
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

    pub fn base(&self, key: BatchKey) -> u32 {
        self.extents[&key].base
    }
}

/// The batch content generation a mirror range holds, and whether it holds previous transforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Held {
    content: u64,
    previous: bool,
}

/// The mirror buffers; `previous` exists once a gather needed previous transforms.
#[derive(Clone, Debug)]
pub(super) struct Buffers {
    pub current: Rc<Tracked<Buffer>>,
    pub previous: Option<Rc<Tracked<Buffer>>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct State {
    pub layout: Layout,
    pub buffers: Option<Buffers>,
    held: BTreeMap<BatchKey, Held>,
}

/// Committed and in-transaction mirror state, like a view's compaction.
#[derive(Debug, Default)]
pub(super) struct Mirror {
    pub committed: Rc<RefCell<State>>,
    drawing: RefCell<Option<(Publications, State)>>,
}

/// Records a batch's buffer can hold.
fn reserved(buffer: &Buffer) -> u32 {
    u32::try_from(buffer.size() / STRIDE).expect("instance batch records fit u32")
}

impl Mirror {
    fn state_in(&self, transaction: &Publications) -> State {
        self.drawing
            .borrow()
            .as_ref()
            .filter(|(owner, _)| owner.same_transaction(transaction))
            .map_or_else(
                || self.committed.borrow().clone(),
                |(_, state)| state.clone(),
            )
    }

    /// Brings the ranges of `keys` up to their batches' content, staging the copies in
    /// `context`, and returns the mirror state the gather reads. Every key must name a batch
    /// with a flushed buffer, and with a previous buffer when `previous`.
    pub fn prepare(
        &self,
        context: &mut PrepareContext<'_>,
        instances: &Instances,
        keys: &[BatchKey],
        previous: bool,
    ) -> State {
        let batches = &instances.batches;
        let owner = context.publications();
        let mut state = self.state_in(&owner);
        let mut layout = state.layout.clone();
        let fits = state
            .buffers
            .as_ref()
            .is_some_and(|buffers| !previous || buffers.previous.is_some())
            && keys
                .iter()
                .all(|key| layout.reserve(*key, reserved(flushed(batches, *key))));
        if fits {
            // A batch that moved to the end holds nothing at its new range yet.
            let moved = &state.layout.extents;
            state
                .held
                .retain(|key, _| layout.extents.get(key) == moved.get(key));
            state.layout = layout;
        } else {
            state = Self::allocate(context.device(), &state, instances, keys, previous);
        }
        for &key in keys {
            Self::refresh(context, &mut state, (key, &batches[&key]), previous);
        }
        *self.drawing.borrow_mut() = Some((owner, state.clone()));
        let committed = self.committed.clone();
        let published = state.clone();
        crate::compositor::publication::stage(context, &committed.clone(), move || {
            *committed.borrow_mut() = published;
        });
        state
    }

    /// A packed layout of every batch the mirror held and `keys`, in new buffers that hold
    /// nothing yet. A replaced buffer is never an in-place copy target.
    fn allocate(
        device: &wgpu::Device,
        state: &State,
        instances: &Instances,
        keys: &[BatchKey],
        previous: bool,
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
        let create = |label| {
            Rc::new(instances.census.create_buffer(
                device,
                ResourceClass::Instance,
                &wgpu::BufferDescriptor {
                    label: Some(label),
                    size: bytes,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            ))
        };
        let keep_previous = state
            .buffers
            .as_ref()
            .is_some_and(|buffers| buffers.previous.is_some());
        State {
            layout,
            buffers: Some(Buffers {
                current: create("gather mirror"),
                previous: (previous || keep_previous).then(|| create("gather previous mirror")),
            }),
            held: BTreeMap::new(),
        }
    }

    /// Copies a batch into its range unless the range already holds its current content.
    fn refresh(
        context: &mut PrepareContext<'_>,
        state: &mut State,
        (key, batch): (BatchKey, &InstanceBatch),
        previous: bool,
    ) {
        let held = Held {
            content: batch.content(),
            previous,
        };
        if state
            .held
            .get(&key)
            .is_some_and(|h| h.content == held.content && (h.previous || !previous))
        {
            return;
        }
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
        state.held.insert(key, held);
    }
}

fn flushed(batches: &BTreeMap<BatchKey, InstanceBatch>, key: BatchKey) -> &Buffer {
    batches[&key]
        .buffer
        .as_deref()
        .expect("flushed instance buffer")
}
