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
    /// One mip of one layer of a declared target, named with
    /// [`TargetDeclaration::mip`](super::TargetDeclaration::mip) (`layer(n)` is mip 0, which is the
    /// only mip of a target without a chain). The pass attaches that single mip, opens with its
    /// extent (the target's extent halved `mip` times, at least one texel) and clears it to zero,
    /// unless the mip already holds contents (written earlier in the frame, or kept from a submitted
    /// frame), which it keeps.
    Target(ResourceRef),
    /// The frame's colour. The first pass to write it clears it to the frame's clear colour; Opaque
    /// then loads what the game's passes left, so the stock objects draw over it. A pass after
    /// Opaque loads it, and at [`GamePlace::BeforePost`] it holds everything the frame lit, which
    /// the pass draws over with the blend state of its draws. Not available at
    /// [`GamePlace::AfterDepthPrepass`], which runs before anything draws the colour.
    Frame,
    /// A colour product of the frame's depth/normal prepass, named by its resource, such as
    /// [`FrameProduct::normals`](crate::frame::metadata::FrameProduct::normals) (view-space
    /// normal in `rgb`, coverage in `a`), the reflection material when reflections are installed,
    /// or a shadow receiver output. The pass loads what the prepass wrote and stores what it
    /// adds, with the prepass's own attachment (format, extent, samples), so the stock passes
    /// that read the product see it. Only at [`GamePlace::AfterDepthPrepass`]; a product the
    /// frame's prepass does not write is refused by its name.
    Prepass(ResourceRef),
}

/// How a game pass attaches the frame's depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DepthAccess {
    /// Tests and writes the frame's depth. The first pass to write it clears it; Opaque then
    /// loads it, so stock objects are depth-tested against what the game drew. At
    /// [`GamePlace::AfterDepthPrepass`] the depth is the prepass's, which the pass loads, and
    /// what it writes completes the depth prefix with the prepass's.
    ReadWrite,
    /// Tests against depth that an earlier pass completed, writing none.
    ReadOnly,
    /// As [`Self::ReadOnly`], and the pass also samples the same depth as a texture, as soft
    /// particles and decals do: they test against the depth and fade with the distance to it.
    /// The plan lists the depth among the pass's reads, so the frame orders it after the depth's
    /// writers, and [`PassTarget::sampled_depth`](super::PassTarget::sampled_depth) lends the
    /// sampled view. A pass that writes depth cannot sample it: listing the frame's depth in
    /// [`GamePassSpec::sampling_frame`] of a [`Self::ReadWrite`] pass is refused at planning with
    /// [`FrameError::AttachmentHazard`] naming the depth.
    ReadOnlySampled,
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
    /// How the pass attaches the frame's depth, and whether it also samples it
    /// ([`DepthAccess::ReadOnlySampled`]). `None` attaches no depth.
    pub depth: Option<DepthAccess>,
    /// Layers of declared targets the pass samples. A read of a layer is a read of every mip of
    /// it, which the target's sampled view covers. Each mip must be written by an earlier pass or
    /// the preparation of the frame, or hold kept contents; a read of a mip nothing writes is
    /// refused at planning by the target's name and the mip. A pass that renders into a mip of a
    /// target may not read the layer: it reads the mips it samples with [`Self::mip_reads`].
    pub reads: Vec<ResourceRef>,
    /// Single mips of declared targets the pass samples, each a read of exactly that
    /// (layer, mip): the frame orders the pass after the writer of that mip and of no other, so
    /// the pass may write another mip of the same texture.
    pub mip_reads: Vec<ResourceRef>,
    /// Stock products of the frame the pass samples through a view a frame handle lends, such as
    /// [`FrameOcclusion::read`](super::FrameOcclusion::read). They attach nothing; frame
    /// validation refuses the frame when no earlier pass of it produces them.
    pub frame_reads: Vec<ResourceRef>,
    /// The frame's opaque snapshots the pass samples. Each must be one the factory declared in
    /// [`GameFeatureFactory::snapshot_reads`](super::GameFeatureFactory::snapshot_reads). The frame
    /// records the snapshots the passes declare and orders every reader after their writers, so
    /// a pass that samples them belongs at or after [`GamePlace::AfterOpaqueSnapshots`], and one that samples
    /// the post-water depth belongs at [`GamePlace::AfterTransparent`], where a read at any earlier
    /// place is refused as [`FrameError::MissingProducer`](worldrunner_render_post::FrameError::MissingProducer).
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
            mip_reads: Vec::new(),
            frame_reads: Vec::new(),
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

    /// Samples exactly these (layer, mip) subresources, named with
    /// [`TargetDeclaration::mip`](super::TargetDeclaration::mip), instead of every mip of their
    /// layers.
    #[must_use]
    pub fn reading_mips(mut self, mips: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.mip_reads.extend(mips);
        self
    }

    /// Adds stock products of the frame the pass samples; see [`Self::frame_reads`].
    #[must_use]
    pub fn sampling_frame(mut self, reads: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.frame_reads.extend(reads);
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
    /// A pass at [`GamePlace::AfterDepthPrepass`] takes the frame depth and the products of the
    /// depth/normal prepass from the prepass's own attachments, and is planned only by the
    /// feature a factory returns from
    /// [`GameFeatureFactory::build_depth_prefix`](super::GameFeatureFactory::build_depth_prefix);
    /// every other place takes the frame's attachments from Opaque and is planned by the
    /// factory's ordinary feature.
    ///
    /// # Errors
    /// [`FrameError::MissingPass`] when the frame has no Opaque pass to take the frame's
    /// attachments from (no depth prepass, at [`GamePlace::AfterDepthPrepass`]),
    /// [`FrameError::AttachmentContract`] for a pass with no attachment,
    /// [`FrameError::AttachmentView`] for a target or layer that is not declared, a copy target
    /// written as a colour output or a prepass product the frame lacks,
    /// [`FrameError::AttachmentHazard`] for a target that shares its name with another
    /// attachment of the frame or a pass that writes the frame's depth and samples it, and
    /// [`FrameError::UnsupportedEffect`] for a pass planned by the wrong feature, a prepass
    /// product outside [`GamePlace::AfterDepthPrepass`] or the frame's colour inside it,
    /// and, when the planned frame is validated, [`FrameError::AttachmentHazard`] for a pass that
    /// samples a mip it also renders into and [`FrameError::MissingProducer`] for a sampled mip
    /// nothing wrote.
    pub fn plan_pass(
        &self,
        spec: &GamePassSpec,
        passes: &mut Vec<PassDeclaration>,
        views: &mut Vec<AttachmentView>,
    ) -> Result<FeaturePass, FrameError> {
        let source = self.source_pass(spec, passes)?;
        let mut pass = PassDeclaration::nth(PassSlot::Game(spec.place), spec.occurrence);
        let mut added = Vec::new();
        let mut colors = Vec::new();
        for output in &spec.colors {
            colors.push(self.color_attachment(spec, *output, &source, &mut added)?);
        }
        let mut colors = colors.into_iter();
        pass.color = colors.next();
        pass.additional_colors = colors.collect();
        let depth = spec
            .depth
            .map(|access| depth_attachment(spec, access, &source))
            .transpose()?;
        let sampled_depth = sampled_depth(spec, depth.as_ref())?;
        pass.depth = depth;
        let mut reads = self.subresource_reads(&spec.reads, &spec.mip_reads);
        for resource in &reads {
            added.push(self.view(*resource)?);
        }
        self.check_snapshot_reads(spec.entry(), spec.snapshots)?;
        let feature = feature_pass(&pass)?;
        let fresh = new_views(views, added)?;
        views.extend(fresh);
        passes.push(pass);
        reads.extend(sampled_depth);
        reads.extend(spec.frame_reads.iter().copied());
        reads.extend(spec.snapshots.resources());
        Ok(FeaturePass { reads, ..feature })
    }

    /// The subresources a pass samples: every mip of each layer of `layers`, then the single
    /// mips of `mips`, once each.
    pub(super) fn subresource_reads(
        &self,
        layers: &[ResourceRef],
        mips: &[ResourceRef],
    ) -> Vec<ResourceRef> {
        let mut reads = self.layer_reads(layers);
        for mip in mips {
            if !reads.contains(mip) {
                reads.push(*mip);
            }
        }
        reads
    }

    /// Refuses a pass `entry` that reads snapshots its factory did not declare.
    pub(super) fn check_snapshot_reads(
        &self,
        entry: PassEntry,
        reads: SnapshotReads,
    ) -> Result<(), FrameError> {
        if self.snapshot_reads().covers(reads) {
            return Ok(());
        }
        Err(FrameError::FeaturePreparation {
            feature: "frame snapshots",
            reason: format!(
                "pass {entry} reads {reads:?}, but its factory declared {:?} in snapshot_reads",
                self.snapshot_reads()
            ),
        })
    }

    /// The stock pass whose attachments a pass in `spec`'s place takes: the depth/normal prepass
    /// after it, Opaque everywhere else.
    fn source_pass(
        &self,
        spec: &GamePassSpec,
        passes: &[PassDeclaration],
    ) -> Result<PassDeclaration, FrameError> {
        let in_prefix = spec.place == GamePlace::AfterDepthPrepass;
        if in_prefix != self.depth_prefix() {
            return Err(FrameError::UnsupportedEffect(if in_prefix {
                "a game pass after the depth prepass belongs to the factory's depth-prefix feature"
            } else {
                "a game's depth-prefix feature plans passes only after the depth prepass"
            }));
        }
        let slot = if in_prefix {
            PassSlot::DepthNormalPrepass
        } else {
            PassSlot::Opaque
        };
        passes
            .iter()
            .find(|pass| pass.slot == slot)
            .cloned()
            .ok_or(FrameError::MissingPass(PassEntry::new(slot)))
    }

    fn color_attachment(
        &self,
        spec: &GamePassSpec,
        output: ColorOutput,
        source: &PassDeclaration,
        added: &mut Vec<AttachmentView>,
    ) -> Result<AttachmentDeclaration, FrameError> {
        let in_prefix = spec.place == GamePlace::AfterDepthPrepass;
        match output {
            ColorOutput::Target(resource) => {
                let view = self.view(resource)?;
                if !view
                    .texture()
                    .usage()
                    .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
                {
                    return Err(FrameError::AttachmentView(resource));
                }
                let declaration = AttachmentDeclaration {
                    resource,
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
                Ok(declaration)
            }
            ColorOutput::Frame if in_prefix => Err(FrameError::UnsupportedEffect(
                "a game pass after the depth prepass cannot write the frame's colour",
            )),
            ColorOutput::Frame => source
                .color
                .clone()
                .ok_or(FrameError::AttachmentContract(spec.entry())),
            ColorOutput::Prepass(_) if !in_prefix => Err(FrameError::UnsupportedEffect(
                "a game pass writes the prepass's products only after the depth prepass",
            )),
            ColorOutput::Prepass(resource) => source
                .color
                .iter()
                .chain(&source.additional_colors)
                .find(|attachment| attachment.resource == resource)
                .map(|attachment| AttachmentDeclaration {
                    load: AttachmentLoad::Load,
                    store: wgpu::StoreOp::Store,
                    read_only: false,
                    ..attachment.clone()
                })
                .ok_or(FrameError::AttachmentView(resource)),
        }
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

/// The frame's depth as a pass in `spec`'s place attaches it: loaded after the depth prepass,
/// which wrote it, and cleared by the first writer anywhere else.
fn depth_attachment(
    spec: &GamePassSpec,
    access: DepthAccess,
    source: &PassDeclaration,
) -> Result<AttachmentDeclaration, FrameError> {
    let frame = source
        .depth
        .clone()
        .ok_or(FrameError::AttachmentContract(spec.entry()))?;
    Ok(match access {
        DepthAccess::ReadWrite => AttachmentDeclaration {
            load: if spec.place == GamePlace::AfterDepthPrepass {
                AttachmentLoad::Load
            } else {
                AttachmentLoad::ClearDepth(1.0)
            },
            store: wgpu::StoreOp::Store,
            read_only: false,
            ..frame
        },
        DepthAccess::ReadOnly | DepthAccess::ReadOnlySampled => AttachmentDeclaration {
            load: AttachmentLoad::Load,
            store: wgpu::StoreOp::Store,
            read_only: true,
            ..frame
        },
    })
}

/// The depth `spec`'s pass samples in the same pass it attaches `depth`: the depth itself for
/// [`DepthAccess::ReadOnlySampled`], nothing otherwise.
///
/// # Errors
/// [`FrameError::AttachmentHazard`] naming the depth when a pass that writes it also lists it
/// in its frame reads: one pass cannot write a depth and sample it.
fn sampled_depth(
    spec: &GamePassSpec,
    depth: Option<&AttachmentDeclaration>,
) -> Result<Option<ResourceRef>, FrameError> {
    let Some(depth) = depth else {
        return Ok(None);
    };
    if !depth.read_only && spec.frame_reads.contains(&depth.resource) {
        return Err(FrameError::AttachmentHazard(depth.resource));
    }
    Ok((spec.depth == Some(DepthAccess::ReadOnlySampled)).then_some(depth.resource))
}

/// Whether the frame's depth prepass writes the depth Opaque draws with, whether Opaque then
/// writes it or only tests it (a reused prepass depth with relief cards attaches it read-only).
/// When it does, a game pass after the prepass that writes depth completes the frame's depth,
/// and a later game pass can test it with [`DepthAccess::ReadOnly`]. When it does not (a frame
/// whose prepass keeps a depth of its own, or one with no prepass), what such a pass writes
/// reaches only the readers of the prepass, and the frame's depth needs its own writer.
pub fn prepass_writes_frame_depth(passes: &[PassDeclaration]) -> bool {
    let depth = |slot| {
        passes
            .iter()
            .find(|pass| pass.slot == slot)
            .and_then(|pass| pass.depth.as_ref())
    };
    let prepass = depth(PassSlot::DepthNormalPrepass).filter(|depth| !depth.read_only);
    matches!((prepass, depth(PassSlot::Opaque)), (Some(prepass), Some(opaque)) if prepass.resource == opaque.resource)
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
pub(super) fn new_views(
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

/// Places the game's passes of a separately encoded depth prefix, the ones `passes` lists from
/// index `stock` on, in forward order immediately after the depth prepass and before the depth
/// levels that read it, then reconciles the loads of the whole prefix as a frame drawn whole
/// does. A later pass that writes what an earlier one wrote loads it, so the prefix's game
/// passes declare the same attachments as the frame that lists them as completed.
///
/// # Errors
/// [`FrameError::MissingPass`] naming the depth prepass when the game planned passes into a
/// prefix whose stock passes, before index `stock`, list none. `passes` is then left as it was.
pub(crate) fn place_after_prepass(
    passes: &mut Vec<PassDeclaration>,
    stock: usize,
) -> Result<(), FrameError> {
    if passes.len() == stock {
        return Ok(());
    }
    let after = 1 + passes[..stock]
        .iter()
        .rposition(|pass| pass.slot == PassSlot::DepthNormalPrepass)
        .ok_or(FrameError::MissingPass(PassEntry::new(
            PassSlot::DepthNormalPrepass,
        )))?;
    let mut game: Vec<_> = passes.drain(stock..).collect();
    crate::compositor::forward_order::forward_order(&mut game);
    passes.splice(after..after, game);
    reconcile_loads(passes);
    Ok(())
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
        // A compute pass leaves what it writes through storage images stored, as an attachment
        // that stores does, so a pass after it that attaches the subresource keeps it.
        for write in pass.storage_writes() {
            written.insert(write.resource);
            if game {
                written_by_game.insert(write.resource);
            }
        }
    }
}
