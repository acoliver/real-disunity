#[cfg(test)]
mod test_hooks;
use std::collections::BTreeSet;
use std::ops::Range;

use worldrunner_render_plan::{SceneObject, SceneTransform};

use worldrunner_render_gpu::census::{MemoryCensus, ResourceClass, Tracked};
use worldrunner_render_gpu::submission::{Publications, TransactionStatus};

use super::skin_stream::SkinStream;
use super::slot_bounds::SlotBounds;
use super::staged_uploads::StagedUploads;
use super::totals::Totals;
use crate::geometry::{INSTANCE_BYTES as STRIDE, pack_object};

#[derive(Debug, Default)]
pub(crate) struct InstanceBatch {
    placements: Vec<Option<(SceneObject, bool)>>,
    free: BTreeSet<usize>,
    dirty: BTreeSet<usize>,
    pub bytes: Vec<u8>,
    uploaded: std::rc::Rc<std::cell::RefCell<Vec<u8>>>,
    buffer_valid: std::rc::Rc<std::cell::Cell<bool>>,
    encoded: Option<(Publications, u64)>,
    staged: StagedUploads,
    pub buffer: Option<Tracked<wgpu::Buffer>>,
    pub(crate) history: super::history::History,
    /// The previous-frame transform of every slot, same layout as `buffer`; velocity draws only.
    pub previous: Option<Tracked<wgpu::Buffer>>,
    /// The per-slot skin descriptors, present when the batch's mesh has a skinned level.
    pub skin: Option<SkinStream>,
    /// View-independent world bounds per slot, filled during selection.
    pub bounds: std::cell::RefCell<SlotBounds>,
    /// Counts placement edits, so a cached view selection can tell that it is stale.
    edits: u64,
    /// Rises whenever a frame stages bytes into this batch's buffers or allocates them, so a
    /// copy of the buffer contents can tell that it is stale. Previous-transform bytes are
    /// rewritten a frame after an edit without `edits` changing.
    content: u64,
    /// The scene's sums of the counters above and of the history's, which this batch raises
    /// together with its own.
    totals: Totals,
}

impl InstanceBatch {
    /// A batch of a skinned mesh, which keeps a skin descriptor for each slot.
    pub fn skinned() -> Self {
        Self {
            skin: Some(SkinStream::default()),
            ..Self::default()
        }
    }

    /// Stores the skin descriptor of `position` and marks the slot for upload.
    ///
    /// A pose change leaves the 80-byte placement unchanged, so the slot is dirty through its
    /// descriptor alone.
    pub fn set_skin(&mut self, position: usize, descriptor: [u32; 4]) {
        let stream = self
            .skin
            .as_mut()
            .expect("a skinned batch has a skin stream");
        stream.set(position, descriptor);
        self.edited();
        self.dirty.insert(position);
    }

    /// Whether a copy must replace this owner's staged bytes or the submitted baseline.
    fn slot_pending(&self, position: usize, owner: Option<&Publications>) -> bool {
        let range = position * STRIDE..(position + 1) * STRIDE;
        let skin = self.skin.as_ref().and_then(|stream| {
            let start = position * crate::skinning::SKIN_INSTANCE_BYTES;
            stream
                .bytes
                .get(start..start + crate::skinning::SKIN_INSTANCE_BYTES)
        });
        self.staged
            .differs(owner, position, &self.bytes[range], skin)
            .unwrap_or_else(|| self.slot_unpublished(position))
    }

    fn slot_unpublished(&self, position: usize) -> bool {
        let range = position * STRIDE..(position + 1) * STRIDE;
        self.uploaded.borrow().get(range.clone()) != Some(&self.bytes[range])
            || self
                .skin
                .as_ref()
                .is_some_and(|stream| stream.differs(position))
    }

    /// Resets remain pending until a final-colour submission publishes them.
    pub fn reset_velocity_history(&mut self) {
        let slots: Vec<_> = self.placements().map(|(slot, _, _)| slot).collect();
        self.history.reset(slots);
    }

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
        self.edited();
        self.bounds.get_mut().invalidate(position);
        self.history.add(position);
        self.pack(position);
        position
    }

    pub fn get(&self, position: usize) -> (SceneObject, bool) {
        self.placements[position].expect("live instance")
    }

    pub fn remove(&mut self, position: usize) {
        self.placements[position] = None;
        self.edited();
        self.bounds.get_mut().invalidate(position);
        self.free.insert(position);
        self.dirty.remove(&position);
        self.history.remove(position);
    }

    pub fn set_transform(&mut self, position: usize, transform: SceneTransform) {
        let (object, _) = self.get(position);
        self.set_object(position, object.with_transform(transform));
    }

    pub fn set_object(&mut self, position: usize, object: SceneObject) {
        self.placements[position].as_mut().expect("live instance").0 = object;
        self.edited();
        self.bounds.get_mut().invalidate(position);
        self.pack(position);
    }

    fn pack(&mut self, position: usize) {
        let packed = pack_object(self.get(position).0);
        let bytes = &mut self.bytes[position * STRIDE..(position + 1) * STRIDE];
        if *bytes != packed[..] {
            bytes.copy_from_slice(&packed);
            self.history.edit(position);
        }
        self.dirty.insert(position);
    }

    pub fn set_visible(&mut self, position: usize, visible: bool) {
        self.placements[position].as_mut().expect("live instance").1 = visible;
        self.edited();
    }

    /// Counts this batch's changes, past and future, in the scene's `totals` as well.
    pub fn count_in(&mut self, totals: &Totals) {
        totals.edits.add(self.edits);
        totals.content.add(self.content);
        self.history.count_in(&totals.history);
        self.totals = totals.clone();
    }

    fn edited(&mut self) {
        self.edits += 1;
        self.totals.edits.add(1);
    }

    fn staged_content(&mut self) {
        self.content += 1;
        self.totals.content.add(1);
    }

    pub fn edits(&self) -> u64 {
        self.edits
    }

    pub fn content(&self) -> u64 {
        self.content
    }

    /// Rises with every change to the history that `ready_for` reads.
    pub fn history_version(&self) -> u64 {
        self.history.version()
    }

    /// Drops every slot's cached bounds after the batch's mesh changed, and counts it as an
    /// edit so a cached view selection is not reused.
    pub fn forget_bounds(&mut self) {
        *self.bounds.get_mut() = SlotBounds::default();
        self.edited();
    }

    /// Scans all allocated slots, excluding removed slots from selection and
    /// statistics while retaining their storage for reuse.
    pub fn placements(&self) -> impl Iterator<Item = (usize, SceneObject, bool)> + '_ {
        self.placements
            .iter()
            .enumerate()
            .filter_map(|(slot, value)| value.map(|(object, visible)| (slot, object, visible)))
    }

    /// Every allocated slot; removed ones are `None`.
    pub fn slots(&self) -> &[Option<(SceneObject, bool)>] {
        &self.placements
    }

    #[cfg(test)]
    pub fn has_pending(&self) -> bool {
        let current = Publications::current();
        if current.as_ref().is_some_and(|current| {
            self.encoded.as_ref().is_some_and(|(encoded, edits)| {
                encoded.same_transaction(current)
                    && *edits == self.edits
                    && current.status() == TransactionStatus::Preparing
            })
        }) {
            return false;
        }
        !self.buffer_valid.get()
            || (self.previous.is_some() && self.history.has_pending())
            || self
                .dirty
                .iter()
                .any(|position| self.slot_pending(*position, current.as_ref()))
    }

    /// Physical readiness is independent of work still owed to a visible colour frame.
    pub fn ready_for(&self, owner: Option<&Publications>) -> bool {
        if self.bytes.is_empty() {
            return true;
        }
        let in_frame = owner.is_some_and(|owner| {
            self.encoded.as_ref().is_some_and(|(encoded, edits)| {
                encoded.same_transaction(owner)
                    && *edits == self.edits
                    && owner.status() == TransactionStatus::Preparing
            })
        });
        (in_frame
            || (self.buffer_valid.get()
                && !self
                    .dirty
                    .iter()
                    .any(|slot| self.slot_pending(*slot, owner))))
            && self
                .history
                .ready(&self.bytes, in_frame, self.previous.is_some())
    }

    pub fn has_instances(&self) -> bool {
        self.free.len() != self.placements.len()
    }

    /// Whether `ready_for` is true for every owner and whatever a transaction has staged: the
    /// batch holds no bytes, or its buffer is current, no slot waits for upload and the history
    /// has nothing pending.
    pub fn is_idle(&self) -> bool {
        self.bytes.is_empty()
            || (self.dirty.is_empty() && self.buffer_valid.get() && !self.history.has_pending())
    }

    /// Whether the buffers hold every slot, the previous-transform buffer too when it is kept and
    /// the skin stream when the batch has one, so `grow` has nothing to make.
    fn buffers_fit(&self, track_previous: bool) -> bool {
        let size = self.bytes.len() as u64;
        let fits = |buffer: &Option<Tracked<wgpu::Buffer>>| {
            buffer.as_deref().is_some_and(|b| b.size() >= size)
        };
        fits(&self.buffer)
            && (!track_previous || fits(&self.previous))
            && self.skin.as_ref().is_none_or(SkinStream::fits)
    }

    /// Whether `flush_prepared` would find nothing to write, copy or allocate: no slot is
    /// waiting for upload, the buffers are current and hold every slot, and the history has
    /// nothing pending.
    pub(super) fn is_settled(&self, track_previous: bool) -> bool {
        self.dirty.is_empty()
            && self.buffer_valid.get()
            && !self.history.has_pending()
            && self.buffers_fit(track_previous || self.previous.is_some())
    }

    /// Whether a gather can read this batch: its instance buffer exists, its previous-transform
    /// buffer when `previous`, and its skin buffer when it has a skin stream.
    pub fn is_flushed(&self, previous: bool) -> bool {
        self.buffer.is_some()
            && (!previous || self.previous.is_some())
            && self
                .skin
                .as_ref()
                .is_none_or(|stream| stream.buffer.is_some())
    }

    /// Keeps copies needed by this encoder and edits still owed if its transaction cancels.
    fn prune_published(&mut self, owner: &Publications) {
        let pending: BTreeSet<usize> = self
            .dirty
            .iter()
            .copied()
            .filter(|position| {
                self.slot_pending(*position, Some(owner)) || self.slot_unpublished(*position)
            })
            .collect();
        self.dirty = pending;
    }

    fn pending_ranges_for(&self, owner: Option<&Publications>) -> Vec<Range<u32>> {
        ranges(
            self.dirty
                .iter()
                .copied()
                .filter(|position| self.slot_pending(*position, owner)),
        )
    }

    #[cfg(test)]
    pub fn commit(&mut self, ranges: &[Range<u32>]) {
        let mut uploaded = self.uploaded.borrow_mut();
        uploaded.resize(self.bytes.len(), 0);
        for range in ranges {
            let bytes = byte_range(range);
            uploaded[bytes.clone()].copy_from_slice(&self.bytes[bytes]);
        }
        if let Some(stream) = &self.skin {
            *stream.uploaded.borrow_mut() = stream.bytes.clone();
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
    census: &MemoryCensus,
    track_previous: bool,
) -> (Vec<Range<u32>>, Option<super::history::Candidate>) {
    let owner = context.publications();
    batch.prune_published(&owner);
    let writes = batch.pending_ranges_for(Some(&owner));
    if batch.bytes.is_empty() {
        return (writes, None);
    }
    let track_previous = track_previous || batch.previous.is_some();
    grow(batch, context, census, track_previous);
    let full = !batch.buffer_valid.get();
    if !full && writes.is_empty() && !batch.history.has_pending() {
        return (writes, None);
    }
    let ranges = if full {
        std::iter::once(0..u32::try_from(batch.bytes.len() / STRIDE).expect("instance count"))
            .collect()
    } else {
        writes.clone()
    };
    let candidate = batch
        .history
        .prepare(&batch.bytes, &batch.dirty, &context.publications());
    let previous = if track_previous {
        batch
            .history
            .previous_uploads(&candidate, &batch.bytes, full)
    } else {
        Vec::new()
    };
    if !ranges.is_empty() || !previous.is_empty() {
        stage_uploads(batch, context, &ranges, &previous);
    }
    if track_previous {
        let acknowledged = candidate.clone();
        crate::compositor::publication::stage(context, &batch.uploaded, move || {
            acknowledged.acknowledge_previous(full);
        });
    }
    batch.encoded = Some((context.publications(), batch.edits));
    (writes, (!candidate.is_empty()).then_some(candidate))
}

/// Physical content changes before the caller refreshes the mirror and gathers view records.
fn stage_uploads(
    batch: &mut InstanceBatch,
    context: &mut worldrunner_render_post::PrepareContext<'_>,
    ranges: &[Range<u32>],
    previous: &[(u64, Vec<u8>)],
) {
    batch.staged_content();
    if let Some(buffer) = batch.previous.as_deref() {
        for (start, bytes) in previous {
            context.upload_buffer(buffer, *start, bytes);
        }
    }
    let buffer = batch.buffer.as_deref().expect("allocated instance buffer");
    let chunks: Vec<_> = ranges
        .iter()
        .map(|range| {
            let range = byte_range(range);
            context.upload_buffer(buffer, range.start as u64, &batch.bytes[range.clone()]);
            (range.clone(), batch.bytes[range].to_vec())
        })
        .collect();
    let skin_published = batch
        .skin
        .as_ref()
        .map(|stream| stream.stage(context, ranges));
    batch
        .staged
        .record(&context.publications(), &chunks, batch.skin.as_ref());
    let uploaded = batch.uploaded.clone();
    let valid = batch.buffer_valid.clone();
    let changes = batch.history.changes();
    let length = batch.bytes.len();
    crate::compositor::publication::stage(context, &uploaded.clone(), move || {
        let mut uploaded = uploaded.borrow_mut();
        uploaded.resize(length, 0);
        for (range, bytes) in chunks {
            uploaded[range].copy_from_slice(&bytes);
        }
        if let Some((uploaded, bytes)) = skin_published {
            *uploaded.borrow_mut() = bytes;
        }
        valid.set(true);
        changes.record();
    });
}

fn grow(
    batch: &mut InstanceBatch,
    context: &mut worldrunner_render_post::PrepareContext<'_>,
    census: &MemoryCensus,
    track_previous: bool,
) {
    let size = batch.bytes.len() as u64;
    if batch.buffers_fit(track_previous) {
        return;
    }
    let device = context.device().clone();
    let fits = |buffer: &Option<Tracked<wgpu::Buffer>>| {
        buffer.as_deref().is_some_and(|b| b.size() >= size)
    };
    let current_fits = fits(&batch.buffer);
    let previous_fits = !track_previous || fits(&batch.previous);
    let capacity = size
        .checked_next_power_of_two()
        .expect("instance buffer size overflow");
    assert!(
        capacity <= device.limits().max_buffer_size,
        "instance buffer exceeds device limit"
    );
    let create = |label, capacity| {
        census.create_buffer(
            &device,
            ResourceClass::Instance,
            &wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            },
        )
    };
    if !current_fits {
        let buffer = create("retained instances", capacity);
        if let Some(old) = batch.buffer.as_deref() {
            context.copy_buffer(old, &buffer, old.size());
        }
        batch.buffer = Some(buffer);
    }
    // A new previous buffer is rewritten in full, so it needs no copy of the old contents.
    if !previous_fits {
        batch.previous = Some(create("retained previous instances", capacity));
    }
    // The skin stream holds a fifth of the instance bytes per slot, and is rewritten in full
    // with the instance buffer, so a replacement needs no copy either.
    if let Some(stream) = batch.skin.as_mut().filter(|stream| !stream.fits()) {
        stream.buffer = Some(create("retained skin instances", skin_capacity(capacity)));
    }
    batch.buffer_valid.set(false);
    batch.staged_content();
}

/// Bytes of a skin stream buffer beside an instance buffer of `instance_capacity` bytes: the same
/// slot count at 16 bytes a slot instead of 80, rounded down to the copy alignment.
fn skin_capacity(instance_capacity: u64) -> u64 {
    let slots = instance_capacity / STRIDE as u64;
    (slots * crate::skinning::SKIN_INSTANCE_BYTES as u64).max(wgpu::COPY_BUFFER_ALIGNMENT)
}

#[cfg(test)]
#[path = "ready_memo_tests.rs"]
mod ready_memo_tests;
#[cfg(test)]
#[path = "totals_tests.rs"]
mod totals_tests;
#[cfg(test)]
#[path = "velocity_reset_cpu_tests.rs"]
mod velocity_reset_cpu_tests;
