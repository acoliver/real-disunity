//! Placing a game's passes in the frame's pass list.
use super::snapshots::SnapshotReads;
use super::targets::GameTargets;
use std::collections::BTreeSet;
use worldrunner_render_post::{
    AttachmentDeclaration, AttachmentLoad, AttachmentView, FeaturePass, FrameError, GamePlace,
    PassDeclaration, PassEntry, PassSlot, ResourceRef,
};

/// One colour output of a game pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorOutput {
    /// One layer of a declared target. The pass clears it to zero, unless the layer already holds
    /// contents (written earlier in the frame, or kept from a submitted frame), which it keeps.
    Target(ResourceRef),
    /// The frame's colour. The first pass to write it clears it to the frame's clear colour; Opaque
    /// then loads what the game's passes left, so the stock objects draw over it.
    Frame,
}

/// How a game pass attaches the frame's depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthAccess {
    /// Tests and writes the frame's depth. The first pass to write it clears it; Opaque then
    /// loads it, so stock objects are depth-tested against what the game drew.
    ReadWrite,
    /// Tests against depth that an earlier pass completed, writing none.
    ReadOnly,
}

/// A game pass: where it sits, what it writes and which declared targets it samples.
#[derive(Clone, Debug, PartialEq)]
pub struct GamePassSpec {
    pub place: GamePlace,
    /// Tells apart the game's passes in one place; they run in this order.
    pub occurrence: u32,
    /// The colour attachments in location order. The first is the pass's colour, the rest its
    /// additional colours.
    pub colors: Vec<ColorOutput>,
    pub depth: Option<DepthAccess>,
    /// Layers of declared targets the pass samples. A read of a layer is a read of every mip of
    /// it, which the target's sampled view covers. Each mip must be written by an earlier pass or
    /// the preparation of the frame, or hold kept contents; a read of a mip nothing writes is
    /// refused at planning by the target's name and the mip.
    pub reads: Vec<ResourceRef>,
    /// The frame's opaque snapshots the pass samples. Each must be one the factory declared in
    /// [`GameFeatureFactory::snapshot_reads`](super::GameFeatureFactory::snapshot_reads). The frame
    /// records the snapshots the passes declare and orders every reader after their writers, so
    /// a pass that samples them belongs at or after [`GamePlace::AfterOpaqueSnapshots`].
    pub snapshots: SnapshotReads,
}

impl GamePassSpec {
    pub fn new(place: GamePlace, occurrence: u32) -> Self {
        Self {
            place,
            occurrence,
            colors: Vec::new(),
            depth: None,
            reads: Vec::new(),
            snapshots: SnapshotReads::NONE,
        }
    }

    pub fn entry(&self) -> PassEntry {
        PassEntry::nth(PassSlot::Game(self.place), self.occurrence)
    }

    #[must_use]
    pub fn writing(mut self, colors: impl IntoIterator<Item = ColorOutput>) -> Self {
        self.colors.extend(colors);
        self
    }

    #[must_use]
    pub fn with_depth(mut self, depth: DepthAccess) -> Self {
        self.depth = Some(depth);
        self
    }

    #[must_use]
    pub fn reading(mut self, reads: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.reads.extend(reads);
        self
    }

    /// Declares that the pass samples the frame's opaque snapshots `reads`.
    #[must_use]
    pub fn reading_snapshots(mut self, reads: SnapshotReads) -> Self {
        self.snapshots = self.snapshots.union(reads);
        self
    }
}

impl GameTargets {
    /// Adds the pass `spec` describes to the frame's pass list and the views it names to the
    /// binding table, and returns the declaration of the feature that draws in it.
    ///
    /// Call this from [`FrameFeature::plan`](worldrunner_render_post::FrameFeature::plan), once
    /// for each pass of the feature, and return the passes' declarations together. The frame
    /// orders the list, and loads what an earlier pass left in an attachment that a later pass
    /// of the game or Opaque would otherwise clear.
    ///
    /// # Errors
    /// [`FrameError::MissingPass`] when the frame has no Opaque pass to take the frame's
    /// attachments from, [`FrameError::AttachmentContract`] for a pass with no attachment,
    /// [`FrameError::AttachmentView`] for a target or layer that is not declared and for a copy
    /// target written as a colour output, and [`FrameError::AttachmentHazard`] for a target that
    /// shares its name with another attachment of the frame.
    pub fn plan_pass(
        &self,
        spec: &GamePassSpec,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeaturePass, FrameError> {
        let entry = spec.entry();
        let opaque = passes
            .iter()
            .find(|pass| pass.slot == PassSlot::Opaque)
            .ok_or(FrameError::MissingPass(PassEntry::new(PassSlot::Opaque)))?;
        let mut pass = PassDeclaration::nth(PassSlot::Game(spec.place), spec.occurrence);
        let mut colors = Vec::new();
        let mut added = Vec::new();
        for output in &spec.colors {
            colors.push(match output {
                ColorOutput::Target(resource) => {
                    let view = self.view(*resource)?;
                    if !view
                        .texture()
                        .usage()
                        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
                    {
                        return Err(FrameError::AttachmentView(*resource));
                    }
                    let declaration = AttachmentDeclaration {
                        resource: *resource,
                        format: view.format(),
                        size: view.size(),
                        samples: 1,
                        load: if view.retained() {
                            AttachmentLoad::Load
                        } else {
                            AttachmentLoad::ClearColor(wgpu::Color::TRANSPARENT)
                        },
                        store: wgpu::StoreOp::Store,
                        read_only: false,
                    };
                    added.push(view);
                    declaration
                }
                ColorOutput::Frame => opaque
                    .color
                    .clone()
                    .ok_or(FrameError::AttachmentContract(entry))?,
            });
        }
        let mut colors = colors.into_iter();
        pass.color = colors.next();
        pass.additional_colors = colors.collect();
        if let Some(access) = spec.depth {
            let frame = opaque
                .depth
                .clone()
                .ok_or(FrameError::AttachmentContract(entry))?;
            pass.depth = Some(match access {
                DepthAccess::ReadWrite => AttachmentDeclaration {
                    load: AttachmentLoad::ClearDepth(1.0),
                    store: wgpu::StoreOp::Store,
                    read_only: false,
                    ..frame
                },
                DepthAccess::ReadOnly => AttachmentDeclaration {
                    load: AttachmentLoad::Load,
                    store: wgpu::StoreOp::Store,
                    read_only: true,
                    ..frame
                },
            });
        }
        let mut reads = self.layer_reads(&spec.reads);
        for resource in &reads {
            added.push(self.view(*resource)?);
        }
        if !self.snapshot_reads().covers(spec.snapshots) {
            return Err(FrameError::FeaturePreparation {
                feature: "frame snapshots",
                reason: format!(
                    "pass {entry} reads {:?}, but its factory declared {:?} in snapshot_reads",
                    spec.snapshots,
                    self.snapshot_reads()
                ),
            });
        }
        let feature = feature_pass(&pass)?;
        let fresh = new_views(views, added)?;
        views.extend(fresh);
        passes.push(pass);
        reads.extend(spec.snapshots.resources());
        Ok(FeaturePass { reads, ..feature })
    }

    /// The subresources a pass's reads sample: every mip of each layer read, once each, in the
    /// order the reads name them. A read of a mip past the chain, or of a name no declaration
    /// made, is left as it is for the view check to refuse.
    fn layer_reads(&self, reads: &[ResourceRef]) -> Vec<ResourceRef> {
        let mut expanded: Vec<ResourceRef> = Vec::new();
        for resource in reads {
            let mips = match self.mips(resource.name) {
                Some(mips) if resource.mip < mips => 0..mips,
                _ => resource.mip..resource.mip + 1,
            };
            for mip in mips {
                let read = ResourceRef { mip, ..*resource };
                if !expanded.contains(&read) {
                    expanded.push(read);
                }
            }
        }
        expanded
    }
}

impl GameTargets {
    /// Adds to the binding table the subresources (one mip of one layer each, named with
    /// [`TargetDeclaration::mip`](super::TargetDeclaration::mip)) a preparation operation of the
    /// frame reads and writes, and marks the written ones prepared: the frame's preparation
    /// writes them before its first pass, so a pass of the frame may read them, and the frame
    /// counts them written when it is submitted. A view the table already holds for a written
    /// subresource is marked in place, so a pass that reads it, planned before or after, sees the
    /// same view.
    ///
    /// Call this from [`FrameFeature::plan`](worldrunner_render_post::FrameFeature::plan), then
    /// stage the operations that write the layers with
    /// [`PrepareContext::record_prepared`](worldrunner_render_post::PrepareContext::record_prepared)
    /// in the feature's `prepare`, each written layer exactly once.
    ///
    /// # Errors
    /// [`FrameError::AttachmentView`] for an undeclared target, layer or mip;
    /// [`FrameError::MissingProducer`] for a read of a subresource no submitted frame wrote;
    /// [`FrameError::AttachmentHazard`] for a subresource both read and written, and for a target
    /// that shares its name with another view of the frame;
    /// [`FrameError::RepeatedPreparedWrite`] for a subresource the frame's preparation already
    /// writes. Nothing is added on any refusal.
    pub fn plan_prepared(
        &self,
        reads: &[ResourceRef],
        writes: &[ResourceRef],
        views: &mut Vec<AttachmentView>,
    ) -> Result<(), FrameError> {
        let mut added = Vec::new();
        for resource in reads {
            let view = self.view(*resource)?;
            if writes.contains(resource) {
                return Err(FrameError::AttachmentHazard(*resource));
            }
            if !view.retained() {
                return Err(FrameError::MissingProducer(*resource));
            }
            added.push(view);
        }
        let written = self.prepared_views(writes, views)?;
        views.extend(new_views(views, added)?);
        for view in written {
            match views
                .iter_mut()
                .find(|known| known.resource() == view.resource())
            {
                Some(known) => *known = known.clone().with_prepared_contents(),
                None => views.push(view.with_prepared_contents()),
            }
        }
        Ok(())
    }

    /// The views of `writes`, checked against each other and against the table.
    fn prepared_views(
        &self,
        writes: &[ResourceRef],
        views: &[AttachmentView],
    ) -> Result<Vec<AttachmentView>, FrameError> {
        let mut written = Vec::new();
        for (index, resource) in writes.iter().enumerate() {
            let view = self.view(*resource)?;
            match views.iter().find(|known| known.resource() == *resource) {
                Some(known) if known.texture() != view.texture() => {
                    return Err(FrameError::AttachmentHazard(*resource));
                }
                Some(known) if known.prepared_contents() => {
                    return Err(FrameError::RepeatedPreparedWrite(*resource));
                }
                _ if writes[..index].contains(resource) => {
                    return Err(FrameError::RepeatedPreparedWrite(*resource));
                }
                _ => written.push(view),
            }
        }
        Ok(written)
    }
}

/// The views of `added` the table lacks. A view the table already holds for the same resource is
/// left out when it is the same texture, and refused when it is not: a game's target that shares
/// a name with another attachment of the frame, such as a stock one, would otherwise stand in
/// for it and the pass would use the wrong texture.
fn new_views(
    views: &[AttachmentView],
    added: Vec<AttachmentView>,
) -> Result<Vec<AttachmentView>, FrameError> {
    let mut fresh: Vec<AttachmentView> = Vec::new();
    for view in added {
        match views
            .iter()
            .chain(&fresh)
            .find(|known| known.resource() == view.resource())
        {
            Some(known) if known.texture() != view.texture() => {
                return Err(FrameError::AttachmentHazard(view.resource()));
            }
            Some(_) => {}
            None => fresh.push(view),
        }
    }
    Ok(fresh)
}

/// The declaration of a feature that draws in `entry` of a planned pass list, taking the formats
/// and sample count of the attachments the entry is opened with. A feature that draws into a
/// stock entry, such as Opaque with its velocity attachment, declares exactly what the stock
/// plan opened and need not know the colour layout.
///
/// # Errors
/// [`FrameError::MissingPass`] when the list has no such entry.
pub fn join_pass(passes: &[PassDeclaration], entry: PassEntry) -> Result<FeaturePass, FrameError> {
    let pass = passes
        .iter()
        .find(|pass| pass.entry() == entry)
        .ok_or(FrameError::MissingPass(entry))?;
    feature_pass(pass)
}

fn feature_pass(pass: &PassDeclaration) -> Result<FeaturePass, FrameError> {
    let samples = pass
        .color
        .iter()
        .chain(&pass.additional_colors)
        .chain(&pass.depth)
        .map(|attachment| attachment.samples)
        .next()
        .ok_or(FrameError::AttachmentContract(pass.entry()))?;
    Ok(FeaturePass {
        slot: pass.slot,
        occurrence: pass.occurrence,
        reads: Vec::new(),
        color_format: pass.color.as_ref().map(|color| color.format),
        additional_color_formats: pass
            .additional_colors
            .iter()
            .map(|color| color.format)
            .collect(),
        depth_format: pass.depth.as_ref().map(|depth| depth.format),
        requires_no_depth: false,
        samples,
    })
}

fn is_game(pass: &PassDeclaration) -> bool {
    matches!(pass.slot, PassSlot::Game(_))
}

/// Loads what an earlier pass left, where the authored plan would clear it.
///
/// A game pass that first writes the frame's colour or depth declares a clear, and cannot know
/// what the stock passes before it did. Once the list is ordered, a game pass loads an
/// attachment that any earlier pass wrote, and an Opaque entry loads one that a game pass wrote,
/// so the game's output survives to the stock draws. A frame with no game pass is left as it
/// was.
pub(crate) fn reconcile_loads(passes: &mut [PassDeclaration]) {
    if !passes.iter().any(is_game) {
        return;
    }
    let mut written = BTreeSet::new();
    let mut written_by_game = BTreeSet::new();
    for pass in passes {
        let game = is_game(pass);
        let opaque = pass.slot == PassSlot::Opaque;
        let attachments = pass
            .color
            .iter_mut()
            .chain(&mut pass.additional_colors)
            .chain(&mut pass.depth);
        for attachment in attachments {
            let clears = matches!(
                attachment.load,
                AttachmentLoad::ClearColor(_) | AttachmentLoad::ClearDepth(_)
            );
            let kept = (game && written.contains(&attachment.resource))
                || (opaque && written_by_game.contains(&attachment.resource));
            if clears && kept {
                attachment.load = AttachmentLoad::Load;
            }
            if !attachment.read_only {
                written.insert(attachment.resource);
                if game {
                    written_by_game.insert(attachment.resource);
                }
            }
        }
    }
}
