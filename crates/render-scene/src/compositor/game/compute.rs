//! Placing a game's compute passes in the frame's pass list.
//!
//! A compute pass is an entry of the frame like a game's render pass: it takes a [`GamePlace`]
//! and an occurrence, which it shares with the game's render passes in that place, and the frame
//! orders and validates it with every other pass. It attaches nothing. It writes single (layer,
//! mip) subresources of declared targets through storage images, and reads what earlier passes
//! wrote, so a later pass may read what it writes. The feature that plans it stages its prepared
//! dispatches for it with
//! [`PrepareContext::record_in_compute_pass`](worldrunner_render_post::PrepareContext::record_in_compute_pass),
//! and the frame records them in one compute pass at its place.
use super::plan::new_views;
use super::snapshots::SnapshotReads;
use super::targets::GameTargets;
use worldrunner_render_post::{
    AttachmentView, FeaturePass, FrameError, GamePlace, PassDeclaration, PassEntry, PassSlot,
    ResourceRef, StorageWrite,
};

/// A game compute pass: where it sits, what it reads and which levels of declared targets it
/// writes.
#[derive(Clone, Debug, PartialEq)]
pub struct GameComputeSpec {
    pub place: GamePlace,
    /// Tells apart the game's passes in one place, render and compute alike; they run in this
    /// order.
    pub occurrence: u32,
    /// Layers of declared targets the pass samples, each a read of every mip of the layer, as for
    /// [`GamePassSpec::reads`](super::GamePassSpec::reads).
    pub reads: Vec<ResourceRef>,
    /// Single (layer, mip) subresources of declared targets the pass samples.
    pub mip_reads: Vec<ResourceRef>,
    /// Stock products of the frame the pass samples, such as the post-water depth;
    /// frame validation refuses the frame when no earlier pass produces them.
    pub frame_reads: Vec<ResourceRef>,
    /// The frame's opaque snapshots the pass samples, which its factory must have declared.
    pub snapshots: SnapshotReads,
    /// Single (layer, mip) subresources of declared targets the pass writes, named with
    /// [`TargetDeclaration::mip`](super::TargetDeclaration::mip). Each target needs
    /// `STORAGE_BINDING`. The pass may not read what it writes: a dispatch of the pass reads what
    /// an earlier dispatch of it wrote without declaring it.
    pub writes: Vec<ResourceRef>,
}

impl GameComputeSpec {
    pub fn new(place: GamePlace, occurrence: u32) -> Self {
        Self {
            place,
            occurrence,
            reads: Vec::new(),
            mip_reads: Vec::new(),
            frame_reads: Vec::new(),
            snapshots: SnapshotReads::NONE,
            writes: Vec::new(),
        }
    }

    pub fn entry(&self) -> PassEntry {
        PassEntry::nth(PassSlot::Game(self.place), self.occurrence)
    }

    /// Writes exactly these (layer, mip) subresources.
    #[must_use]
    pub fn writing_mips(mut self, writes: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.writes.extend(writes);
        self
    }

    /// Samples every mip of these layers.
    #[must_use]
    pub fn reading(mut self, reads: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.reads.extend(reads);
        self
    }

    /// Samples exactly these (layer, mip) subresources.
    #[must_use]
    pub fn reading_mips(mut self, mips: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.mip_reads.extend(mips);
        self
    }

    /// Samples these stock products of the frame; see [`Self::frame_reads`].
    #[must_use]
    pub fn sampling_frame(mut self, reads: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.frame_reads.extend(reads);
        self
    }

    /// Samples the frame's opaque snapshots `reads`.
    #[must_use]
    pub fn reading_snapshots(mut self, reads: SnapshotReads) -> Self {
        self.snapshots = self.snapshots.union(reads);
        self
    }
}

impl GameTargets {
    /// Adds the compute pass `spec` describes to the frame's pass list and the views it names to
    /// the binding table, and returns the declaration of the feature that stages work in it.
    ///
    /// Call this from [`FrameFeature::plan`](worldrunner_render_post::FrameFeature::plan), as
    /// [`Self::plan_pass`] for a render pass, and return the declarations together. The frame
    /// orders the pass by its place and occurrence and validates its reads and writes with the
    /// other passes: a read needs an earlier writer
    /// ([`FrameError::MissingProducer`] otherwise), and a pass after it may read what it writes.
    ///
    /// # Errors
    /// [`FrameError::UnsupportedEffect`] at [`GamePlace::AfterDepthPrepass`] and from a
    /// depth-prefix feature, whose prefix is encoded apart;
    /// [`FrameError::AttachmentView`] for a write of an undeclared target, layer or mip, or of a
    /// target without `STORAGE_BINDING`, and for a read of an undeclared one;
    /// [`FrameError::AttachmentHazard`] for a subresource written twice or read and written, and
    /// for a target that shares its name with another view of the frame;
    /// [`FrameError::FeaturePreparation`] for snapshots the factory did not declare. Nothing is
    /// planned on any refusal.
    pub fn plan_compute_pass(
        &self,
        spec: &GameComputeSpec,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeaturePass, FrameError> {
        if spec.place == GamePlace::AfterDepthPrepass || self.depth_prefix() {
            return Err(FrameError::UnsupportedEffect(
                "a game compute pass runs only after the depth prefix, from the ordinary feature",
            ));
        }
        let mut added = Vec::new();
        let mut writes = Vec::new();
        for (index, &resource) in spec.writes.iter().enumerate() {
            let view = self.view(resource)?;
            if !view
                .texture()
                .usage()
                .contains(wgpu::TextureUsages::STORAGE_BINDING)
            {
                return Err(FrameError::AttachmentView(resource));
            }
            if spec.writes[..index].contains(&resource) {
                return Err(FrameError::AttachmentHazard(resource));
            }
            writes.push(StorageWrite::new(resource, view.format(), view.size()));
            added.push(view);
        }
        let mut reads = self.subresource_reads(&spec.reads, &spec.mip_reads);
        for &read in &reads {
            if spec.writes.contains(&read) {
                return Err(FrameError::AttachmentHazard(read));
            }
            added.push(self.view(read)?);
        }
        self.check_snapshot_reads(spec.entry(), spec.snapshots)?;
        views.extend(new_views(views, added)?);
        passes.push(PassDeclaration::compute_pass(
            PassSlot::Game(spec.place),
            spec.occurrence,
            writes,
        ));
        reads.extend(spec.frame_reads.iter().copied());
        reads.extend(spec.snapshots.resources());
        Ok(FeaturePass {
            reads,
            ..FeaturePass::nth(PassSlot::Game(spec.place), spec.occurrence)
        })
    }
}

#[cfg(test)]
#[path = "compute_tests.rs"]
mod tests;
