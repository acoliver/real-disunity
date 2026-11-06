//! Per-view compaction of the selected instance records into one vertex buffer.
//!
//! A selection is a list of short slot ranges per (batch, LOD). Drawing each range separately
//! costs a vertex buffer binding and an indexed draw per material slot, which dominates encoding
//! on large retained scenes. Compaction gathers the selected records, ordered by (batch, LOD, slot),
//! into a buffer owned by the view, so each (batch, LOD) group becomes one instanced draw.
//!
//! The gather is a GPU buffer copy per range, staged only when the compaction key changes.
//! Preparation overwrites the view buffer in place and a cancelled frame discards its encoder,
//! so the committed state stays exact. Within one transaction a draw may use the compacted
//! groups only when that transaction prepared the view with the draw's key, or prepared nothing
//! for the view and the committed key equals the draw's key.
use std::{cell::RefCell, rc::Rc};

use glam::Mat4;
use wgpu::Buffer;
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::PrepareContext;

use super::selection_reuse::{self, Key as SelectionKey};
use super::visibility::SelectedRange;
use super::{DrawCount, FrameUniform, SceneGpu};
use crate::geometry::INSTANCE_BYTES;

mod plan;
use plan::Plan;

const STRIDE: u64 = INSTANCE_BYTES as u64;

/// Everything the gathered bytes depend on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Key {
    selection: SelectionKey,
    /// Instance buffer contents, which the selection key does not cover.
    content: u64,
    /// Whether the previous-transform records are gathered too.
    previous: bool,
}

#[derive(Clone, Debug)]
struct Buffers {
    current: Buffer,
    previous: Option<Buffer>,
}

impl Buffers {
    fn holds(&self, bytes: u64, previous: bool) -> bool {
        self.current.size() >= bytes
            && (!previous
                || self
                    .previous
                    .as_ref()
                    .is_some_and(|buffer| buffer.size() >= bytes))
    }

    fn allocate(device: &wgpu::Device, bytes: u64, previous: bool) -> Option<Self> {
        let capacity = bytes.checked_next_power_of_two()?;
        if capacity > device.limits().max_buffer_size {
            return None;
        }
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
        Some(Self {
            current: create("view instances"),
            previous: previous.then(|| create("view previous instances")),
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
    #[cfg(test)]
    pub observed: Observed,
}

/// What tests can see of a view's compaction.
#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct Observed {
    /// Gather copies staged so far.
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

/// The batch buffers one gather copy reads.
struct Source<'a> {
    current: &'a Buffer,
    previous: Option<&'a Buffer>,
}

impl SceneGpu {
    fn compaction_key(&self, shadow_matrix: Option<(u32, Mat4)>, frame: &FrameUniform) -> Key {
        Key {
            selection: self.selection_key(shadow_matrix, frame),
            content: self.instances.content_generation(),
            previous: self.instances.track_previous,
        }
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
        let Some(plan) = Plan::new(&selection.ranges) else {
            return;
        };
        let Some(sources) = self.gather_sources(&plan, key.previous) else {
            return;
        };
        let buffers = if plan.total == 0 {
            latest.buffers.clone()
        } else if let Some(buffers) =
            Self::gather_buffers(context.device(), &latest, plan.total, key.previous)
        {
            Self::stage_gather(context, &plan, &sources, &buffers);
            #[cfg(test)]
            compaction
                .observed
                .staged
                .set(compaction.observed.staged.get() + plan.copies.len());
            Some(buffers)
        } else {
            return;
        };
        let resident = Resident {
            key: Some(key),
            buffers,
            groups: plan.groups.into(),
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

    /// The batch buffers of each copy, or `None` when a selected batch has no flushed buffer.
    fn gather_sources(&self, plan: &Plan, previous: bool) -> Option<Vec<Source<'_>>> {
        plan.copies
            .iter()
            .map(|copy| {
                let batch = self.instances.batches.get(&copy.key)?;
                let current = batch.buffer.as_ref()?;
                Some(Source {
                    current,
                    previous: if previous {
                        Some(batch.previous.as_ref()?)
                    } else {
                        None
                    },
                })
            })
            .collect()
    }

    /// Reuses the latest buffers while they hold `total` records, otherwise allocates new
    /// ones; a grown buffer is never an in-place copy target.
    fn gather_buffers(
        device: &wgpu::Device,
        latest: &Resident,
        total: u32,
        previous: bool,
    ) -> Option<Buffers> {
        let bytes = u64::from(total) * STRIDE;
        match latest.buffers.as_ref().filter(|b| b.holds(bytes, previous)) {
            Some(existing) => Some(existing.clone()),
            None => Buffers::allocate(device, bytes, previous),
        }
    }

    fn stage_gather(
        context: &mut PrepareContext<'_>,
        plan: &Plan,
        sources: &[Source<'_>],
        buffers: &Buffers,
    ) {
        for (copy, buffer) in plan.copies.iter().zip(sources) {
            let bytes = u64::from(copy.source.end - copy.source.start) * STRIDE;
            let source = u64::from(copy.source.start) * STRIDE;
            let destination = u64::from(copy.destination) * STRIDE;
            context.copy_buffer_region(
                (buffer.current, source),
                (&buffers.current, destination),
                bytes,
            );
            if let (Some(previous), Some(target)) = (buffer.previous, &buffers.previous) {
                context.copy_buffer_region((previous, source), (target, destination), bytes);
            }
        }
    }

    /// Records a view's selection: one draw per (batch, LOD) group from the view buffer when
    /// it holds exactly this selection, otherwise one draw per selected range.
    pub(super) fn record_selection(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipelines: &(impl std::ops::Index<usize, Output = wgpu::RenderPipeline> + ?Sized),
        view: (Option<(u32, Mat4)>, &FrameUniform),
        ranges: &[SelectedRange],
    ) -> DrawCount {
        let (shadow_matrix, frame) = view;
        let key = self.compaction_key(shadow_matrix, frame);
        let compaction = &self.views[selection_reuse::view_index(shadow_matrix)].compaction;
        let mut count = DrawCount::default();
        let mut state = super::lod_material::BoundState::default();
        let Some(resident) = compaction.usable(key) else {
            for range in ranges {
                count.add(self.record_lod_range(pass, pipelines, range, &mut state));
            }
            return count;
        };
        #[cfg(test)]
        compaction
            .observed
            .compacted_draws
            .set(compaction.observed.compacted_draws.get() + 1);
        let Some(buffers) = resident
            .buffers
            .as_ref()
            .filter(|_| !resident.groups.is_empty())
        else {
            return count;
        };
        pass.set_vertex_buffer(1, buffers.current.slice(..));
        // Only velocity pipelines declare this buffer; the others ignore the extra binding.
        if let Some(previous) = &buffers.previous {
            pass.set_vertex_buffer(2, previous.slice(..));
        }
        for group in resident.groups.iter() {
            count.add(self.record_lod_draws(pass, pipelines, group, &mut state));
        }
        count
    }
}

#[cfg(test)]
impl SceneGpu {
    /// Selected ranges and (batch, LOD) groups of the view's last published selection.
    pub(crate) fn published_ranges_and_groups(&self, view: usize) -> (usize, usize) {
        let selection = self.views[view].published.borrow();
        let groups = Plan::new(&selection.ranges).map_or(0, |plan| plan.groups.len());
        (selection.ranges.len(), groups)
    }
}

#[cfg(test)]
mod tests;
