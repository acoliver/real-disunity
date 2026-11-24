//! Compute passes: entries of the pass list with no attachments, where the frame records the
//! compute work its features stage for them, between the passes before and after them.
//!
//! A compute pass is planned, ordered and validated with the render passes: its features declare
//! what they read (`FeaturePass::reads`), which an earlier pass must have produced, and the pass
//! declares the subresources it writes ([`StorageWrite`]), which later passes then read. Its
//! features stage prepared compute work for it with [`PrepareContext::record_in_compute_pass`]
//! while they prepare, and the executor records that work, in staging order, in one compute pass
//! at the entry's place with [`PreparedCompute::record_in_pass`](super::PreparedCompute), the same
//! call the frame's preparation records a run of compute work with.
use super::{
    AttachmentView, FeatureDeclaration, FeaturePass, FrameError, PassDeclaration, PassEntry,
    PassSlot, PrepareContext, PreparedAccess, PreparedCommands, Produced, Product, ResourceRef,
    prepared, require_producer,
};
use crate::plan_hash::PlanMap;

/// What a compute pass writes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComputeDeclaration {
    pub writes: Vec<StorageWrite>,
}

/// One (layer, mip) subresource a compute pass writes through a storage image, with the format
/// and extent of that subresource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageWrite {
    pub resource: ResourceRef,
    pub format: wgpu::TextureFormat,
    pub size: [u32; 2],
}

impl StorageWrite {
    pub const fn new(resource: ResourceRef, format: wgpu::TextureFormat, size: [u32; 2]) -> Self {
        Self {
            resource,
            format,
            size,
        }
    }

    /// The write as the producer later passes read: stored, single-sampled.
    const fn produced(self) -> Product {
        Product {
            format: self.format,
            size: self.size,
            samples: 1,
            store: wgpu::StoreOp::Store,
        }
    }
}

impl PassDeclaration {
    /// The compute pass `occurrence` of `slot`, writing `writes`. It shares its role's
    /// occurrences with the render passes of the role: an entry names one pass of either kind.
    pub fn compute_pass(slot: PassSlot, occurrence: u32, writes: Vec<StorageWrite>) -> Self {
        let mut pass = Self::nth(slot, occurrence);
        pass.compute = Some(ComputeDeclaration { writes });
        pass
    }

    /// The subresources a compute pass writes; none for a render pass.
    pub fn storage_writes(&self) -> &[StorageWrite] {
        self.compute
            .as_ref()
            .map_or(&[], |compute| compute.writes.as_slice())
    }

    /// What the pass leaves written, in order: each attachment it writes, with whether it
    /// stores, and each subresource a compute pass writes, which it stores.
    pub fn written(&self) -> Vec<(ResourceRef, bool)> {
        self.color
            .iter()
            .chain(&self.additional_colors)
            .chain(&self.depth)
            .filter(|attachment| !attachment.read_only)
            .map(|attachment| {
                (
                    attachment.resource,
                    attachment.store == wgpu::StoreOp::Store,
                )
            })
            .chain(
                self.storage_writes()
                    .iter()
                    .map(|write| (write.resource, true)),
            )
            .collect()
    }
}

/// Checks compute pass `pass` against what the passes before it `produced`, and the features
/// that declare it, then adds what it writes to `produced`.
///
/// # Errors
/// [`FrameError::AttachmentContract`] for an attachment of the pass, an empty write or a feature
/// that declares attachment formats in it; [`FrameError::AttachmentHazard`] for a write named
/// twice, a write of a stock product written once per frame, and a read of a write of the pass;
/// [`FrameError::MissingProducer`] and [`FrameError::LoadAfterDiscard`] for a read no earlier
/// pass left.
pub(super) fn validate_compute_pass<'a>(
    pass: &PassDeclaration,
    compute: &ComputeDeclaration,
    declared: impl Iterator<Item = &'a FeaturePass>,
    produced: &mut Produced,
) -> Result<(), FrameError> {
    let entry = pass.entry();
    if pass.color.is_some() || !pass.additional_colors.is_empty() || pass.depth.is_some() {
        return Err(FrameError::AttachmentContract(entry));
    }
    for (index, write) in compute.writes.iter().enumerate() {
        if write.size.contains(&0) {
            return Err(FrameError::AttachmentContract(entry));
        }
        let once = super::WRITTEN_ONCE.contains(&write.resource.name)
            && produced.contains_key(&write.resource);
        if once
            || compute.writes[..index]
                .iter()
                .any(|other| other.resource == write.resource)
        {
            return Err(FrameError::AttachmentHazard(write.resource));
        }
    }
    for feature in declared {
        if feature.color_format.is_some()
            || !feature.additional_color_formats.is_empty()
            || feature.depth_format.is_some()
        {
            return Err(FrameError::AttachmentContract(entry));
        }
        for &read in &feature.reads {
            if compute.writes.iter().any(|write| write.resource == read) {
                return Err(FrameError::AttachmentHazard(read));
            }
            require_producer(produced, read)?;
        }
    }
    produced.extend(
        compute
            .writes
            .iter()
            .map(|write| (write.resource, write.produced())),
    );
    Ok(())
}

/// Refuses a storage write whose view is missing, or is not a single-sampled view of a storage
/// texture with the write's format and extent.
pub(super) fn validate_storage_views(
    passes: &[PassDeclaration],
    views: &PlanMap<ResourceRef, &AttachmentView>,
) -> Result<(), FrameError> {
    for write in passes.iter().flat_map(PassDeclaration::storage_writes) {
        let suits = views.get(&write.resource).is_some_and(|view| {
            let texture = view.texture();
            view.format() == write.format
                && view.size() == write.size
                && texture.sample_count() == 1
                && texture
                    .usage()
                    .contains(wgpu::TextureUsages::STORAGE_BINDING)
        });
        if !suits {
            return Err(FrameError::AttachmentView(write.resource));
        }
    }
    Ok(())
}

/// Commands staged for a compute pass, with the label they were staged as.
type Staged = (&'static str, Box<dyn PreparedCommands>);

/// One compute pass the frame records, and what its features staged for it.
struct StagedPass {
    entry: PassEntry,
    /// What the pass's features declared they read, which earlier passes produced.
    reads: Vec<ResourceRef>,
    /// What the pass declares it writes.
    writes: Vec<ResourceRef>,
    /// What the commands staged so far write.
    written: Vec<ResourceRef>,
    staged: Vec<Staged>,
}

impl StagedPass {
    /// Refuses `access` for commands staged next in this pass.
    fn check(&self, access: PreparedAccess<'_>) -> Result<(), FrameError> {
        for (index, &write) in access.writes.iter().enumerate() {
            if !self.writes.contains(&write) {
                return Err(FrameError::UnplannedWrite(write));
            }
            if access.writes[..index].contains(&write) {
                return Err(FrameError::RepeatedPreparedWrite(write));
            }
        }
        for &read in access.reads {
            if access.writes.contains(&read) {
                return Err(FrameError::AttachmentHazard(read));
            }
            if !self.reads.contains(&read) && !self.written.contains(&read) {
                return Err(FrameError::MissingProducer(read));
            }
        }
        Ok(())
    }
}

/// The compute passes a frame records and the work staged for each, from the features'
/// preparation to the executor's pass loop.
#[derive(Default)]
pub(crate) struct ComputeStaging {
    passes: Vec<StagedPass>,
}

impl ComputeStaging {
    /// Records the work staged for `entry` into `pass`, in staging order.
    ///
    /// # Errors
    /// [`FrameError::PreparedRefused`] with the owner's refusal to record commands.
    pub(crate) fn record(
        &self,
        entry: PassEntry,
        pass: &mut wgpu::ComputePass<'_>,
    ) -> Result<(), FrameError> {
        let staged = self
            .passes
            .iter()
            .filter(|staged| staged.entry == entry)
            .flat_map(|staged| &staged.staged);
        for &(label, ref commands) in staged {
            let compute = commands
                .compute()
                .ok_or(FrameError::NotComputeWork { label, entry })?;
            prepared::record_in_pass(pass, label, compute)?;
        }
        Ok(())
    }
}

impl PrepareContext<'_> {
    /// Stages `commands` in the compute pass `entry`, recorded after the commands staged for it
    /// before them and labelled `label` ([`PreparedCompute::record_in_pass`](super::PreparedCompute)).
    /// Calls `commands.retain` first.
    ///
    /// `access` names the views of the frame the commands read and write. A write must be one the
    /// pass declares; several commands of the pass may write one subresource. A read must be one
    /// the pass's features declared, which an earlier pass produced, or a subresource that
    /// commands staged earlier in the same pass write: each dispatch of a compute pass reads what
    /// the dispatches before it wrote.
    ///
    /// # Errors
    /// [`FrameError::PreparedOutsidePreparation`] outside the features' `prepare`;
    /// [`FrameError::MissingPass`] for an entry the plan lacks; [`FrameError::NotComputePass`] for
    /// a render pass and for a compute pass the frame's completed geometry prefix encoded;
    /// [`FrameError::NotComputeWork`] for commands with no compute form;
    /// [`FrameError::UnplannedWrite`] for a write the pass does not declare;
    /// [`FrameError::RepeatedPreparedWrite`] for a write `access` names twice;
    /// [`FrameError::AttachmentHazard`] for a read the commands also write;
    /// [`FrameError::MissingProducer`] for a read neither declared nor written earlier in the
    /// pass; [`FrameError::AttachmentView`] for a read the frame's views lack;
    /// [`FrameError::PreparedWriteMismatch`] when the views of the frame `commands` write are not
    /// the views `access` names; [`FrameError::PreparedRefused`] with the owner's refusal of the
    /// retention. Nothing is staged on any refusal.
    pub fn record_in_compute_pass(
        &mut self,
        entry: PassEntry,
        label: &'static str,
        access: PreparedAccess<'_>,
        commands: Box<dyn PreparedCommands>,
    ) -> Result<(), FrameError> {
        if !self.preparing {
            return Err(FrameError::PreparedOutsidePreparation);
        }
        let Some(index) = self
            .compute
            .passes
            .iter()
            .position(|staged| staged.entry == entry)
        else {
            return Err(if self.passes.iter().any(|pass| pass.entry() == entry) {
                FrameError::NotComputePass { label, entry }
            } else {
                FrameError::MissingPass(entry)
            });
        };
        if commands.compute().is_none() {
            return Err(FrameError::NotComputeWork { label, entry });
        }
        self.compute.passes[index].check(access)?;
        for &read in access.reads {
            self.resource(read)?;
        }
        self.check_writes(label, access.writes, &commands.writes())?;
        commands
            .retain(self.encoder)
            .map_err(|refusal| FrameError::PreparedRefused { label, refusal })?;
        let staged = &mut self.compute.passes[index];
        staged.written.extend_from_slice(access.writes);
        staged.staged.push((label, commands));
        Ok(())
    }

    /// Opens the compute passes of the plan the frame records, each with the reads its features
    /// declare in `declarations`. A pass the frame's completed geometry prefix encoded is not
    /// recorded again.
    pub(crate) fn open_compute_passes(&mut self, declarations: &[FeatureDeclaration]) {
        self.compute.passes = self
            .passes
            .iter()
            .filter(|pass| !self.completed.contains(&pass.entry()))
            .filter_map(|pass| {
                let compute = pass.compute.as_ref()?;
                let entry = pass.entry();
                let reads = declarations
                    .iter()
                    .flat_map(|feature| &feature.passes)
                    .filter(|feature| feature.entry() == entry)
                    .flat_map(|feature| feature.reads.iter().copied())
                    .collect();
                Some(StagedPass {
                    entry,
                    reads,
                    writes: compute.writes.iter().map(|write| write.resource).collect(),
                    written: Vec::new(),
                    staged: Vec::new(),
                })
            })
            .collect();
    }

    /// Refuses a write a compute pass declares that no commands staged for it write.
    pub(crate) fn close_compute_passes(&self) -> Result<(), FrameError> {
        for staged in &self.compute.passes {
            if let Some(&write) = staged
                .writes
                .iter()
                .find(|write| !staged.written.contains(write))
            {
                return Err(FrameError::UnstagedWrite(write));
            }
        }
        Ok(())
    }

    /// Takes the work staged for the frame's compute passes, for the executor to record at
    /// their places.
    pub(crate) fn take_compute_passes(&mut self) -> ComputeStaging {
        std::mem::take(&mut self.compute)
    }
}

#[cfg(test)]
#[path = "compute_tests.rs"]
mod tests;
