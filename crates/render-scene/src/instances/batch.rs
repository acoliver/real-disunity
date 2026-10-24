use std::collections::BTreeSet;
use std::ops::Range;

use worldrunner_render_plan::{SceneObject, SceneTransform};

use crate::geometry::{INSTANCE_BYTES as STRIDE, pack_instance};

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
}

impl InstanceBatch {
    pub fn add(&mut self, object: SceneObject, visible: bool) -> usize {
        let position = self.free.pop_first().unwrap_or_else(|| {
            let position = self.placements.len();
            u32::try_from(position + 1).expect("instance batch exceeds draw range");
            self.placements.push(None);
            self.bytes.resize((position + 1) * STRIDE, 0);
            position
        });
        self.placements[position] = Some((object, visible));
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
    }

    pub fn set_transform(&mut self, position: usize, transform: SceneTransform) {
        let (object, visible) = self.get(position);
        self.placements[position] = Some((
            SceneObject::new(transform, object.mesh(), object.material()),
            visible,
        ));
        self.pack(position);
    }

    fn pack(&mut self, position: usize) {
        let object = self.get(position).0;
        self.bytes[position * STRIDE..(position + 1) * STRIDE]
            .copy_from_slice(&pack_instance(object.transform()));
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
            || self.dirty.iter().any(|position| {
                let range = position * STRIDE..(position + 1) * STRIDE;
                self.uploaded.borrow().get(range.clone()) != Some(&self.bytes[range])
            })
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
) -> Vec<Range<u32>> {
    let writes = batch.pending_ranges();
    let mut ranges = writes.clone();
    if batch.bytes.is_empty() {
        return ranges;
    }
    grow(batch, context);
    if !batch.buffer_valid.get() {
        ranges =
            std::iter::once(0..u32::try_from(batch.bytes.len() / STRIDE).expect("instance count"))
                .collect();
    }
    if ranges.is_empty() {
        return ranges;
    }
    let buffer = batch.buffer.as_ref().expect("allocated instance buffer");
    for range in &ranges {
        let bytes = byte_range(range);
        context.upload_buffer(buffer, bytes.start as u64, &batch.bytes[bytes]);
    }
    let uploaded = batch.uploaded.clone();
    let valid = batch.buffer_valid.clone();
    let bytes = batch.bytes.clone();
    context.publish(move || {
        *uploaded.borrow_mut() = bytes;
        valid.set(true);
    });
    batch.encoded = worldrunner_render_gpu::submission::Publications::current();
    writes
}

fn grow(batch: &mut InstanceBatch, context: &mut worldrunner_render_post::PrepareContext<'_>) {
    let device = context.device();
    let size = batch.bytes.len() as u64;
    if batch
        .buffer
        .as_ref()
        .is_some_and(|buffer| buffer.size() >= size)
    {
        return;
    }
    let capacity = size
        .checked_next_power_of_two()
        .expect("instance buffer size overflow");
    assert!(
        capacity <= device.limits().max_buffer_size,
        "instance buffer exceeds device limit"
    );
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("retained instances"),
        size: capacity,
        usage: wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    if let Some(old) = &batch.buffer {
        context.copy_buffer(old, &buffer, old.size());
    }
    batch.buffer = Some(buffer);
    batch.buffer_valid.set(false);
}
