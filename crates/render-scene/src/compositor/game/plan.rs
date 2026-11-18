//! Placing a game's passes in the frame's pass list.
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
    /// Layers of declared targets the pass samples. Each must be written by an earlier pass of
    /// the frame or hold kept contents; a read nothing writes is refused at planning by the
    /// target's name.
    pub reads: Vec<ResourceRef>,
    /// Stock products of the frame the pass samples through a view a frame handle lends, such as
    /// [`FrameOcclusion::read`](super::FrameOcclusion::read). They attach nothing; frame
    /// validation refuses the frame when no earlier pass of it produces them.
    pub frame_reads: Vec<ResourceRef>,
}

impl GamePassSpec {
    pub fn new(place: GamePlace, occurrence: u32) -> Self {
        Self {
            place,
            occurrence,
            colors: Vec::new(),
            depth: None,
            reads: Vec::new(),
            frame_reads: Vec::new(),
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

    /// Adds stock products of the frame the pass samples; see [`Self::frame_reads`].
    #[must_use]
    pub fn sampling_frame(mut self, reads: impl IntoIterator<Item = ResourceRef>) -> Self {
        self.frame_reads.extend(reads);
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
    /// [`FrameError::AttachmentView`] for a target or layer that is not declared, and
    /// [`FrameError::AttachmentHazard`] for a target that shares its name with another
    /// attachment of the frame.
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
        for resource in &spec.reads {
            added.push(self.view(*resource)?);
        }
        let feature = feature_pass(&pass)?;
        let fresh = new_views(views, added)?;
        views.extend(fresh);
        passes.push(pass);
        Ok(FeaturePass {
            reads: spec
                .reads
                .iter()
                .chain(&spec.frame_reads)
                .copied()
                .collect(),
            ..feature
        })
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
