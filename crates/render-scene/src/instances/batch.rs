use std::collections::BTreeSet;
use std::ops::Range;

use worldrunner_render_plan::{SceneObject, SceneTransform};

use crate::geometry::{INSTANCE_BYTES as STRIDE, pack_object};

#[derive(Debug, Default)]
pub(crate) struct InstanceBatch {
    placements: Vec<Option<(SceneObject, bool)>>,
    free: BTreeSet<usize>,
    dirty: BTreeSet<usize>,
    pub bytes: Vec<u8>,
    uploaded: std::rc::Rc<std::cell::RefCell<Vec<u8>>>,
    buffer_valid: std::rc::Rc<std::cell::Cell<bool>>,
    encoded: Option<worldrunner_render_gpu::submission::Publications>,
    pub buffer: Option<wgpu::Buffer>,
    /// Slots added since the last published frame; their previous transform is their current one.
    fresh: std::rc::Rc<std::cell::RefCell<BTreeSet<usize>>>,
    /// Slots whose previous transform differed from their current one in the last published
    /// frame. They are copied once more so an untouched slot converges to previous == current.
    stale: std::rc::Rc<std::cell::RefCell<BTreeSet<usize>>>,
    /// The previous-frame transform of every slot, same layout as `buffer`; velocity draws only.
    pub previous: Option<wgpu::Buffer>,
}

impl InstanceBatch {
    pub(crate) fn publication_key(&self) -> worldrunner_render_gpu::submission::capture::WriteKey {
        crate::compositor::publication::key(&self.uploaded)
    }

    pub fn add(&mut self, object: SceneObject, visible: bool) -> usize {
        let position = self.free.pop_first().unwrap_or_else(|| {
            let position = self.placements.len();
            u32::try_from(position + 1).expect("instance batch exceeds draw range");
            self.placements.push(None);
            self.bytes.resize((position + 1) * STRIDE, 0);
            position
        });
        self.placements[position] = Some((object, visible));
        self.fresh.borrow_mut().insert(position);
        self.pack(position);
        position
    }

    pub fn get(&self, position: usize) -> (SceneObject, bool) {
        self.placements[position].expect("live instance")
    }

    pub fn remove(&mut self, position: usize) {
        self.placements[position] = None;
        self.free.insert(position);
        self.dirty.remove(&position);
        self.stale.borrow_mut().remove(&position);
    }

    pub fn set_transform(&mut self, position: usize, transform: SceneTransform) {
        let (object, _) = self.get(position);
        self.set_object(position, object.with_transform(transform));
    }

    pub fn set_object(&mut self, position: usize, object: SceneObject) {
        self.placements[position].as_mut().expect("live instance").0 = object;
        self.pack(position);
    }

    fn pack(&mut self, position: usize) {
        let object = self.get(position).0;
        self.bytes[position * STRIDE..(position + 1) * STRIDE]
            .copy_from_slice(&pack_object(object));
        self.dirty.insert(position);
    }

    pub fn set_visible(&mut self, position: usize, visible: bool) {
        self.placements[position].as_mut().expect("live instance").1 = visible;
    }

    /// Scans all allocated slots, excluding removed slots from selection and
    /// statistics while retaining their storage for reuse.
    pub fn placements(&self) -> impl Iterator<Item = (usize, SceneObject, bool)> + '_ {
        self.placements
            .iter()
            .enumerate()
            .filter_map(|(slot, value)| value.map(|(object, visible)| (slot, object, visible)))
    }

    pub fn has_pending(&self) -> bool {
        if worldrunner_render_gpu::submission::Publications::current().is_some_and(|current| {
            self.encoded
                .as_ref()
                .is_some_and(|encoded| encoded.same_transaction(&current))
        }) {
            return false;
        }
        !self.buffer_valid.get()
            || (self.previous.is_some() && self.live_stale().next().is_some())
            || self.dirty.iter().any(|position| {
                let range = position * STRIDE..(position + 1) * STRIDE;
                self.uploaded.borrow().get(range.clone()) != Some(&self.bytes[range])
            })
    }

    fn live_stale(&self) -> impl Iterator<Item = usize> + '_ {
        let stale: Vec<usize> = self.stale.borrow().iter().copied().collect();
        stale
            .into_iter()
            .filter(|position| self.placements.get(*position).is_some_and(Option::is_some))
    }

    #[cfg(test)]
    pub fn visible_ranges(&self) -> Vec<Range<u32>> {
        ranges(
            self.placements
                .iter()
                .enumerate()
                .filter(|(_, placement)| placement.is_some_and(|(_, visible)| visible))
                .map(|(position, _)| position),
        )
    }

    pub fn has_instances(&self) -> bool {
        self.free.len() != self.placements.len()
    }

    pub fn pending_ranges(&self) -> Vec<Range<u32>> {
        ranges(self.dirty.iter().copied().filter(|position| {
            let range = position * STRIDE..(position + 1) * STRIDE;
            self.uploaded.borrow().get(range.clone()) != Some(&self.bytes[range])
        }))
    }

    #[cfg(test)]
    pub fn commit(&mut self, ranges: &[Range<u32>]) {
        let mut uploaded = self.uploaded.borrow_mut();
        uploaded.resize(self.bytes.len(), 0);
        for range in ranges {
            let bytes = byte_range(range);
            uploaded[bytes.clone()].copy_from_slice(&self.bytes[bytes]);
        }
        self.dirty.clear();
        self.buffer_valid.set(true);
    }
}

pub(crate) fn byte_range(range: &Range<u32>) -> Range<usize> {
    usize::try_from(range.start).expect("instance index fits usize") * STRIDE
        ..usize::try_from(range.end).expect("instance index fits usize") * STRIDE
}

fn ranges(positions: impl Iterator<Item = usize>) -> Vec<Range<u32>> {
    let mut ranges: Vec<Range<u32>> = Vec::new();
    for position in positions {
        let position = u32::try_from(position).expect("instance count fits draw range");
        if let Some(last) = ranges.last_mut().filter(|last| last.end == position) {
            last.end += 1;
        } else {
            ranges.push(position..position + 1);
        }
    }
    ranges
}

pub(crate) fn flush_prepared(
    batch: &mut InstanceBatch,
    context: &mut worldrunner_render_post::PrepareContext<'_>,
    track_previous: bool,
) -> Vec<Range<u32>> {
    let writes = batch.pending_ranges();
    let mut ranges = writes.clone();
    if batch.bytes.is_empty() {
        return ranges;
    }
    grow(batch, context, track_previous);
    let full = !batch.buffer_valid.get();
    if full {
        ranges =
            std::iter::once(0..u32::try_from(batch.bytes.len() / STRIDE).expect("instance count"))
                .collect();
    }
    let previous = if track_previous {
        previous_uploads(batch, &ranges, full)
    } else {
        PreviousUploads::default()
    };
    if ranges.is_empty() && previous.uploads.is_empty() {
        return ranges;
    }
    // Previous transforms are staged first: the encoder runs copies in staging order and the
    // previous buffer is a separate resource, so it can never observe this frame's bytes.
    if let Some(buffer) = batch.previous.as_ref().filter(|_| track_previous) {
        for (start, bytes) in &previous.uploads {
            context.upload_buffer(buffer, *start, bytes);
        }
    }
    let buffer = batch.buffer.as_ref().expect("allocated instance buffer");
    for range in &ranges {
        let bytes = byte_range(range);
        context.upload_buffer(buffer, bytes.start as u64, &batch.bytes[bytes]);
    }
    let uploaded = batch.uploaded.clone();
    let valid = batch.buffer_valid.clone();
    let bytes = batch.bytes.clone();
    let (fresh, stale) = (batch.fresh.clone(), batch.stale.clone());
    let included = batch.fresh.borrow().clone();
    crate::compositor::publication::stage(context, &uploaded.clone(), move || {
        *uploaded.borrow_mut() = bytes;
        valid.set(true);
        fresh.borrow_mut().retain(|slot| !included.contains(slot));
        if track_previous {
            *stale.borrow_mut() = previous.slots;
        }
    });
    batch.encoded = worldrunner_render_gpu::submission::Publications::current();
    writes
}

#[derive(Default)]
struct PreviousUploads {
    /// Slots whose previous transform still differs from the current one after this frame.
    slots: BTreeSet<usize>,
    uploads: Vec<(u64, Vec<u8>)>,
}

/// The previous transforms to write this frame, taken from the last published bytes so that a
/// cancelled frame leaves them untouched. Slots edited this frame or the last one are written;
/// a full upload rewrites every slot. Added slots start with previous == current.
fn previous_uploads(batch: &InstanceBatch, ranges: &[Range<u32>], full: bool) -> PreviousUploads {
    let mut slots: BTreeSet<usize> = batch.live_stale().collect();
    if full {
        slots.extend(0..batch.bytes.len() / STRIDE);
    } else {
        slots.extend(
            ranges
                .iter()
                .flat_map(|range| range.start as usize..range.end as usize),
        );
    }
    let fresh = batch.fresh.borrow();
    let uploaded = batch.uploaded.borrow();
    let source = |slot: usize| {
        let range = slot * STRIDE..(slot + 1) * STRIDE;
        let current = &batch.bytes[range.clone()];
        if fresh.contains(&slot) {
            current
        } else {
            uploaded.get(range).unwrap_or(current)
        }
    };
    let positions = slots.iter().copied();
    let mut result = PreviousUploads::default();
    for range in ranges_of(positions) {
        let bytes = range
            .clone()
            .flat_map(|slot| source(slot).iter().copied())
            .collect();
        result.uploads.push(((range.start * STRIDE) as u64, bytes));
    }
    result.slots = slots
        .into_iter()
        .filter(|slot| source(*slot) != &batch.bytes[slot * STRIDE..(slot + 1) * STRIDE])
        .collect();
    result
}

fn ranges_of(positions: impl Iterator<Item = usize>) -> Vec<Range<usize>> {
    ranges(positions)
        .into_iter()
        .map(|range| range.start as usize..range.end as usize)
        .collect()
}

fn grow(
    batch: &mut InstanceBatch,
    context: &mut worldrunner_render_post::PrepareContext<'_>,
    track_previous: bool,
) {
    let device = context.device().clone();
    let size = batch.bytes.len() as u64;
    let fits = |buffer: &Option<wgpu::Buffer>| buffer.as_ref().is_some_and(|b| b.size() >= size);
    let current_fits = fits(&batch.buffer);
    let previous_fits = !track_previous || fits(&batch.previous);
    if current_fits && previous_fits {
        return;
    }
    let capacity = size
        .checked_next_power_of_two()
        .expect("instance buffer size overflow");
    assert!(
        capacity <= device.limits().max_buffer_size,
        "instance buffer exceeds device limit"
    );
    let create = |label| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    };
    if !current_fits {
        let buffer = create("retained instances");
        if let Some(old) = &batch.buffer {
            context.copy_buffer(old, &buffer, old.size());
        }
        batch.buffer = Some(buffer);
    }
    // A new previous buffer is rewritten in full, so it needs no copy of the old contents.
    if !previous_fits {
        batch.previous = Some(create("retained previous instances"));
    }
    batch.buffer_valid.set(false);
}
