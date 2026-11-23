//! Per-view compaction of the selected instance records into one vertex buffer.
//!
//! A selection is a list of short slot ranges per (batch, LOD). Drawing each range separately
//! costs a vertex buffer binding and an indexed draw per material slot, which dominates encoding
//! on large retained scenes. Compaction gathers the selected records, ordered by (batch, LOD, slot),
//! into a buffer owned by the view, so each (batch, LOD) group becomes one instanced draw.
//!
//! The gather is one compute dispatch per view, staged only when the compaction key changes. It
//! reads a scene-wide mirror of the gathered batches (see [`mirror`]) through a list of runs
//! uploaded into a buffer the view reuses, so recording a moved view costs one upload and one
//! dispatch however fragmented its selection is. Preparation overwrites the view buffer in place
//! and a cancelled frame discards its encoder, so the committed state stays exact. Within one transaction a draw may use the compacted
//! groups only when that transaction prepared the view with the draw's key, or prepared nothing
//! for the view and the committed key equals the draw's key.
//!
//! Skinned batches gather like rigid ones: the mirror and the view buffers have a skin half with
//! one 16-byte descriptor per record at the record's index, and the same runs copy it, so a
//! gathered record and the descriptor its skinned vertex stage reads share one compacted index.
//! Records of rigid batches leave their descriptors unwritten; nothing reads them.
//!
//! The view buffers, the mirror and the run lists count as instance bytes in the scene's memory
//! census, like the batch buffers they gather from.
use std::cell::RefCell;
use std::rc::Rc;

use glam::Mat4;
use wgpu::Buffer;
use worldrunner_render_gpu::census::MemoryCensus;
use worldrunner_render_gpu::census::ResourceClass;
use worldrunner_render_gpu::census::Tracked;
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::PrepareContext;

use super::FrameUniform;
use super::ObjectDraws;
use super::ObjectRecording;
use super::SceneGpu;
use super::selection_reuse::Key as SelectionKey;
use super::selection_reuse::{self};
use super::visibility::SelectedRange;
use crate::geometry::INSTANCE_BYTES;
use crate::instances::Ascending;
use crate::instances::BatchKey;
use crate::instances::Stamp;
use crate::skinning::SKIN_INSTANCE_BYTES;

mod gather;
mod mirror;
mod plan;
pub(super) use gather::Shared;
#[cfg(test)]
use plan::Plan;
use plan::Workspace;

const STRIDE: u64 = INSTANCE_BYTES as u64;
const SKIN_STRIDE: u64 = SKIN_INSTANCE_BYTES as u64;

/// Everything the gathered bytes depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key {
    selection: SelectionKey,
    /// Instance buffer contents, which the selection key does not cover.
    content: u64,
    /// Whether the previous-transform records are gathered too.
    previous: bool,
    /// Whether the scene has skinned batches, whose skin descriptors are gathered too.
    skin: bool,
}

/// A view buffer; the committed and in-transaction states share it, and it stays counted until
/// the last of them drops.
type ViewBuffer = Rc<Tracked<Buffer>>;

#[derive(Clone, Debug)]
struct Buffers {
    current: ViewBuffer,
    previous: Option<ViewBuffer>,
    /// One skin descriptor per record, at the record's index.
    skin: Option<ViewBuffer>,
}

/// What a gather needs of its destination buffers.
#[derive(Clone, Copy)]
struct Needs {
    records: u32,
    previous: bool,
    skin: bool,
}

impl Buffers {
    fn holds(&self, needs: Needs) -> bool {
        let big_enough = |buffer: &Option<ViewBuffer>, bytes: u64| {
            buffer.as_ref().is_some_and(|buffer| buffer.size() >= bytes)
        };
        let bytes = u64::from(needs.records) * STRIDE;
        self.current.size() >= bytes
            && (!needs.previous || big_enough(&self.previous, bytes))
            && (!needs.skin || big_enough(&self.skin, u64::from(needs.records) * SKIN_STRIDE))
    }

    fn allocate(device: &wgpu::Device, census: &MemoryCensus, needs: Needs) -> Option<Self> {
        let capacity = (u64::from(needs.records) * STRIDE).checked_next_power_of_two()?;
        let limits = device.limits();
        if capacity > limits.max_buffer_size {
            return None;
        }
        assert!(
            capacity <= limits.max_storage_buffer_binding_size,
            "view instance buffer of {capacity} bytes exceeds the device's storage binding limit"
        );
        let create = |label, size| {
            Rc::new(census.create_buffer(
                device,
                ResourceClass::Instance,
                &wgpu::BufferDescriptor {
                    label: Some(label),
                    size,
                    usage: wgpu::BufferUsages::VERTEX
                        | wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                },
            ))
        };
        Some(Self {
            current: create("view instances", capacity),
            previous: needs
                .previous
                .then(|| create("view previous instances", capacity)),
            skin: needs.skin.then(|| {
                let slots = capacity / STRIDE;
                create(
                    "view skin instances",
                    (slots * SKIN_STRIDE).max(wgpu::COPY_BUFFER_ALIGNMENT),
                )
            }),
        })
    }
}

/// The gathered groups of one view and the key they were gathered for.
#[derive(Clone, Debug, Default)]
pub(super) struct Resident {
    key: Option<Key>,
    buffers: Option<Buffers>,
    /// Ranges of the view buffer, one per (batch, LOD) in batch and LOD order.
    pub groups: Rc<[SelectedRange]>,
}

#[derive(Debug)]
struct Prepared {
    owner: Publications,
    resident: Resident,
}

/// Committed and in-transaction compaction of one view, like terrain streaming's residents.
#[derive(Debug, Default)]
pub(super) struct Compaction {
    pub committed: Rc<RefCell<Resident>>,
    drawing: RefCell<Option<Prepared>>,
    scratch: RefCell<gather::Scratch>,
    /// The plan and run lists of the view's last gather, built again by each one.
    workspace: RefCell<Workspace>,
    #[cfg(test)]
    pub observed: Observed,
}

/// What tests can see of a view's compaction.
#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct Observed {
    /// Selected ranges gathered so far.
    pub staged: std::cell::Cell<usize>,
    /// Draws of the view that used the compacted groups.
    pub compacted_draws: std::cell::Cell<usize>,
    /// Forces the ranged path, as the reference for pixel comparisons.
    pub ranged_only: std::cell::Cell<bool>,
}

impl Compaction {
    /// What a draw or preparation in `transaction` finds: that transaction's own preparation,
    /// otherwise the committed state.
    fn state_in(&self, transaction: Option<&Publications>) -> Resident {
        self.drawing
            .borrow()
            .as_ref()
            .filter(|prepared| transaction.is_some_and(|t| t.same_transaction(&prepared.owner)))
            .map_or_else(
                || self.committed.borrow().clone(),
                |prepared| prepared.resident.clone(),
            )
    }

    fn usable(&self, key: Key) -> Option<Resident> {
        #[cfg(test)]
        if self.observed.ranged_only.get() {
            return None;
        }
        let resident = self.state_in(Publications::current().as_ref());
        (resident.key == Some(key)).then_some(resident)
    }

    /// Capacity in bytes of the committed view buffer.
    #[cfg(test)]
    pub fn committed_capacity(&self) -> u64 {
        self.committed
            .borrow()
            .buffers
            .as_ref()
            .map_or(0, |buffers| buffers.current.size())
    }
}

impl SceneGpu {
    fn compaction_key(&self, shadow_matrix: Option<(u32, Mat4)>, frame: &FrameUniform) -> Key {
        self.compaction_key_at(self.instances.stamp(), shadow_matrix, frame)
    }

    /// `compaction_key` for a caller that already read the placements' `stamp`.
    fn compaction_key_at(
        &self,
        stamp: Stamp,
        shadow_matrix: Option<(u32, Mat4)>,
        frame: &FrameUniform,
    ) -> Key {
        Key {
            selection: self.selection_key_at(stamp.edits, shadow_matrix, frame),
            content: stamp.content,
            previous: self.instances.track_previous,
            skin: stamp.skinned,
        }
    }

    /// Selects together every view in `views` whose gathered instances `context`'s transaction
    /// must restage, so `prepare_view_compaction` then finds each selection built.
    pub(crate) fn select_stale_views(
        &self,
        context: &PrepareContext<'_>,
        views: &[Option<(u32, Mat4)>],
        frame: &FrameUniform,
    ) {
        let owner = context.publications();
        let stamp = self.instances.stamp();
        let stale: Vec<_> = views
            .iter()
            .copied()
            .filter(|&shadow_matrix| {
                let compaction = &self.views[selection_reuse::view_index(shadow_matrix)].compaction;
                compaction.state_in(Some(&owner)).key
                    != Some(self.compaction_key_at(stamp, shadow_matrix, frame))
            })
            .collect();
        self.select_views(&stale, frame);
    }

    /// Gathers a view's selected instances for the draws of the transaction that `context`
    /// belongs to. Call after the instance uploads and the frame are staged in `context`.
    /// Nothing is staged while the view's compaction key is unchanged.
    pub(crate) fn prepare_view_compaction(
        &self,
        context: &mut PrepareContext<'_>,
        shadow_matrix: Option<(u32, Mat4)>,
        frame: &FrameUniform,
    ) {
        let key = self.compaction_key(shadow_matrix, frame);
        let compaction = &self.views[selection_reuse::view_index(shadow_matrix)].compaction;
        let owner = context.publications();
        let latest = compaction.state_in(Some(&owner));
        if latest.key == Some(key) {
            return;
        }
        let selection = self.select_objects(shadow_matrix, frame);
        let mut work = compaction.workspace.borrow_mut();
        if !work.arrange(&selection.ranges) || !self.gather_sources(work.keys(), key.previous) {
            return;
        }
        // A rigid scene that gains a skinned batch changes the key, so the mirror and the view
        // buffers gain their skin halves before a skinned record is gathered.
        let needs = Needs {
            records: work.plan().total,
            previous: key.previous,
            skin: key.skin,
        };
        let buffers = if needs.records == 0 {
            latest.buffers.clone()
        } else if let Some(buffers) =
            Self::gather_buffers((context.device(), &self.instances.census), &latest, needs)
        {
            self.stage_gather(
                context,
                (compaction, &mut work),
                (&buffers, needs, key.content),
            );
            #[cfg(test)]
            compaction
                .observed
                .staged
                .set(compaction.observed.staged.get() + work.plan().copies.len());
            Some(buffers)
        } else {
            return;
        };
        let resident = Resident {
            key: Some(key),
            buffers,
            groups: work.plan().groups.as_slice().into(),
        };
        *compaction.drawing.borrow_mut() = Some(Prepared {
            owner,
            resident: resident.clone(),
        });
        let committed = compaction.committed.clone();
        crate::compositor::publication::stage(context, &committed.clone(), move || {
            *committed.borrow_mut() = resident;
        });
    }

    /// Whether every batch of `keys`, which are the selected batches in key order, has a flushed
    /// buffer (and a previous buffer when `previous`, and a skinned batch a flushed skin
    /// stream). A scene whose batches are all flushed answers without asking any of them; a
    /// selection is built from the scene's batches, which are never removed, so each key names
    /// one.
    fn gather_sources(&self, keys: &[BatchKey], previous: bool) -> bool {
        if self.instances.all_flushed(previous) {
            return true;
        }
        let mut batches = Ascending::new(&self.instances.batches, keys.len());
        keys.iter().all(|key| {
            #[cfg(test)]
            self.gather
                .source_checks
                .set(self.gather.source_checks.get() + 1);
            batches
                .get(key)
                .is_some_and(|batch| batch.is_flushed(previous))
        })
    }

    /// Reuses the latest buffers while they hold what `needs` asks, otherwise allocates new
    /// ones; a grown buffer is never an in-place copy target.
    fn gather_buffers(
        (device, census): (&wgpu::Device, &MemoryCensus),
        latest: &Resident,
        needs: Needs,
    ) -> Option<Buffers> {
        match latest.buffers.as_ref().filter(|b| b.holds(needs)) {
            Some(existing) => Some(existing.clone()),
            None => Buffers::allocate(device, census, needs),
        }
    }

    /// Brings the mirror up to date for the plan's batches, then stages the view's dispatch.
    fn stage_gather(
        &self,
        context: &mut PrepareContext<'_>,
        (compaction, work): (&Compaction, &mut Workspace),
        (buffers, needs, generation): (&Buffers, Needs, u64),
    ) {
        let halves = mirror::Halves {
            previous: needs.previous,
            skin: needs.skin,
        };
        let mirror =
            self.gather
                .mirror
                .prepare(context, &self.instances, work.keys(), (halves, generation));
        let mut bases = mirror.layout.bases(work.keys().len());
        let runs = work.runs(|key| bases.base(key));
        let bindings = gather::Bindings {
            mirror: mirror.buffers.as_ref().expect("allocated gather mirror"),
            current: &buffers.current,
            previous: buffers
                .previous
                .as_deref()
                .map(|b| &**b)
                .filter(|_| needs.previous),
            skin: buffers.skin.as_deref().map(|b| &**b).filter(|_| needs.skin),
        };
        self.gather.dispatch(
            context,
            (&compaction.scratch, &self.instances.census),
            runs,
            &bindings,
        );
    }

    /// Records a view's selection: one draw per (batch, LOD) group from the view buffer when
    /// it holds exactly this selection, otherwise one draw per selected range. Returns the
    /// draws recorded and which of the two ways recorded them.
    pub(super) fn record_selection(
        &self,
        sink: &mut impl super::lod_material::DrawSink,
        view: (Option<(u32, Mat4)>, &FrameUniform),
        ranges: &[SelectedRange],
    ) -> ObjectDraws {
        let (shadow_matrix, frame) = view;
        let key = self.compaction_key(shadow_matrix, frame);
        let compaction = &self.views[selection_reuse::view_index(shadow_matrix)].compaction;
        let Some(resident) = compaction.usable(key) else {
            let mut drawn = ObjectDraws::new(ObjectRecording::Ranged);
            for range in ranges {
                self.record_lod_range(sink, range, &mut drawn);
            }
            return drawn;
        };
        #[cfg(test)]
        compaction
            .observed
            .compacted_draws
            .set(compaction.observed.compacted_draws.get() + 1);
        let mut drawn = ObjectDraws::new(ObjectRecording::Compacted);
        let Some(buffers) = resident
            .buffers
            .as_ref()
            .filter(|_| !resident.groups.is_empty())
        else {
            return drawn;
        };
        sink.instances(
            &buffers.current,
            buffers.previous.as_deref().map(|b| &**b),
            buffers.skin.as_deref().map(|b| &**b),
        );
        for group in resident.groups.iter() {
            self.record_lod_draws(sink, group, &mut drawn);
        }
        drawn
    }
}

/// Bytes of the committed buffers compaction holds, by kind.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CompactionBytes {
    /// The mirror and its previous-transform half.
    pub mirror: u64,
    /// Every view's gathered records and previous transforms.
    pub views: u64,
    /// Every view's run list.
    pub runs: u64,
    /// The skin halves of the mirror and of every view's buffers.
    pub skin: u64,
}

#[cfg(test)]
impl CompactionBytes {
    pub fn total(self) -> u64 {
        self.mirror + self.views + self.runs + self.skin
    }
}

#[cfg(test)]
impl SceneGpu {
    /// Sizes of the committed mirror, view buffers and run lists.
    pub(crate) fn compaction_bytes(&self) -> CompactionBytes {
        let pair = |current: &Buffer, previous: Option<&Buffer>| {
            current.size() + previous.map_or(0, Buffer::size)
        };
        let size = |buffer: &Option<Rc<Tracked<Buffer>>>| buffer.as_deref().map_or(0, |b| b.size());
        let committed = self.gather.mirror.committed.borrow();
        let mirror = committed.buffers.as_ref().map_or(0, |buffers| {
            pair(&buffers.current, buffers.previous.as_deref().map(|b| &**b))
        });
        let mut skin = committed
            .buffers
            .as_ref()
            .map_or(0, |buffers| size(&buffers.skin));
        let mut views = 0;
        for view in &self.views {
            let resident = view.compaction.committed.borrow();
            if let Some(buffers) = resident.buffers.as_ref() {
                views += pair(&buffers.current, buffers.previous.as_deref().map(|b| &**b));
                skin += size(&buffers.skin);
            }
        }
        let runs = self
            .views
            .iter()
            .map(|view| view.compaction.scratch.borrow().bytes())
            .sum();
        CompactionBytes {
            mirror,
            views,
            runs,
            skin,
        }
    }

    /// The key the committed gather of `view` was staged for, as text.
    pub(crate) fn committed_compaction_key(&self, view: usize) -> Option<String> {
        let key = self.views[view].compaction.committed.borrow().key;
        key.map(|key| format!("{key:?}"))
    }

    /// Where the lists a view's gathers reuse live and how much each holds.
    pub(crate) fn scratch_buffers(&self, view: usize) -> Vec<(usize, usize)> {
        let compaction = &self.views[view].compaction;
        let mut buffers = compaction.workspace.borrow().buffers().to_vec();
        buffers.push(compaction.scratch.borrow().bytes_buffer());
        buffers
    }

    /// How many batches `gather_sources` checked one by one.
    pub(crate) fn source_checks(&self) -> usize {
        self.gather.source_checks.get()
    }

    /// How many times the gather mirror checked batch ranges one by one.
    pub(crate) fn mirror_scans(&self) -> usize {
        self.gather.mirror.scans()
    }

    /// How many batch ranges those checks started from: the keys the mirror was not known to hold.
    pub(crate) fn mirror_checked(&self) -> usize {
        self.gather.mirror.checked()
    }

    /// The committed gather mirror, for comparison.
    #[cfg(test)]
    pub(crate) fn committed_mirror(&self) -> mirror::Snapshot {
        self.gather.mirror.snapshot()
    }

    /// Selected ranges and (batch, LOD) groups of the view's last published selection.
    pub(crate) fn published_ranges_and_groups(&self, view: usize) -> (usize, usize) {
        let selection = self.views[view].published.borrow();
        let groups = Plan::new(&selection.ranges).map_or(0, |plan| plan.groups.len());
        (selection.ranges.len(), groups)
    }

    /// The committed view buffers of `view` and, for each range of its last published
    /// selection, the batch buffers and offsets a plain buffer copy of that range would use.
    pub(crate) fn committed_gather(&self, view: usize) -> CommittedGather {
        let resident = self.views[view].compaction.committed.borrow().clone();
        let buffers = resident.buffers.expect("committed view buffers");
        let plan = Plan::new(&self.views[view].published.borrow().ranges).expect("compactable");
        let copies = plan
            .copies
            .iter()
            .map(|copy| {
                let batch = &self.instances.batches[&copy.key];
                GatherCopy {
                    current: batch
                        .buffer
                        .as_deref()
                        .cloned()
                        .expect("flushed instance buffer"),
                    previous: batch.previous.as_deref().cloned(),
                    source: u64::from(copy.source.start) * STRIDE,
                    destination: u64::from(copy.destination) * STRIDE,
                    bytes: u64::from(copy.source.end - copy.source.start) * STRIDE,
                    skin: batch.skin.as_ref().map(|stream| SkinCopy {
                        buffer: stream
                            .buffer
                            .as_deref()
                            .cloned()
                            .expect("flushed skin stream"),
                        source: u64::from(copy.source.start) * SKIN_STRIDE,
                        destination: u64::from(copy.destination) * SKIN_STRIDE,
                        bytes: u64::from(copy.source.end - copy.source.start) * SKIN_STRIDE,
                    }),
                }
            })
            .collect();
        CommittedGather {
            current: Buffer::clone(&buffers.current),
            previous: buffers.previous.as_deref().map(|b| Buffer::clone(b)),
            skin: buffers.skin.as_deref().map(|b| Buffer::clone(b)),
            bytes: u64::from(plan.total) * STRIDE,
            skin_bytes: u64::from(plan.total) * SKIN_STRIDE,
            keys: plan
                .groups
                .iter()
                .map(|group| (group.key, group.lod))
                .collect(),
            copies,
        }
    }
}

/// What a test needs to rebuild a view's gather with plain buffer copies.
#[cfg(test)]
pub(crate) struct CommittedGather {
    pub current: Buffer,
    pub previous: Option<Buffer>,
    /// The view's skin descriptors, present while the scene has skinned batches.
    pub skin: Option<Buffer>,
    /// Bytes of the view buffer the gather wrote.
    pub bytes: u64,
    /// Bytes of the view's skin buffer the gather wrote.
    pub skin_bytes: u64,
    /// (batch, LOD) of every group, in view buffer order.
    pub keys: Vec<(BatchKey, usize)>,
    pub copies: Vec<GatherCopy>,
}

/// One selected range as a buffer copy from its batch buffers.
#[cfg(test)]
pub(crate) struct GatherCopy {
    pub current: Buffer,
    pub previous: Option<Buffer>,
    pub source: u64,
    pub destination: u64,
    pub bytes: u64,
    /// The same range of the batch's skin stream, for a skinned batch.
    pub skin: Option<SkinCopy>,
}

/// One selected range as a buffer copy from its batch's skin stream.
#[cfg(test)]
pub(crate) struct SkinCopy {
    pub buffer: Buffer,
    pub source: u64,
    pub destination: u64,
    pub bytes: u64,
}

#[cfg(test)]
mod tests;
