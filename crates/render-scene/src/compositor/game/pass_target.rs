//! What a feature needs to know to draw into a pass: its attachments and its depth rules.
use crate::renderer::depth::DepthMode;
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, FrameError, PassDeclaration, PassEntry, PassSlot,
    PrepareContext,
};
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{PreparedProgramDraw, TargetSignature};
use worldrunner_render_program::state::{ColorState, RasterState, RenderState};

/// The depth attachment of a pass and the rules a draw into it follows.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PassDepth {
    format: wgpu::TextureFormat,
    mode: DepthMode,
    writes: bool,
}

/// The exact attachments and depth state of one entry of the frame being prepared. A program
/// draw prepared against [`signature`](Self::signature) and [`render_state`](Self::render_state)
/// records into that entry without a mismatch.
///
/// The depth rules come from the same source as the stock draws': the entry's depth attachment
/// decides the mode. A pass that completes the depth for a later Opaque entry to reuse marks the
/// stencil as the stock prepass does, an Opaque entry that loads such depth tests it
/// for equality and writes none, and every other pass tests `LessEqual` and writes depth unless
/// it attaches it read-only. A pass that attaches no depth tests none.
#[derive(Clone, Debug, PartialEq)]
pub struct PassTarget {
    entry: PassEntry,
    colors: Vec<wgpu::TextureFormat>,
    depth: Option<PassDepth>,
    samples: u32,
    extent: Option<[u32; 2]>,
}

impl PassTarget {
    /// The target of `entry` in the frame being prepared.
    ///
    /// # Errors
    /// [`FrameError::MissingPass`] when the frame has no such entry.
    pub fn of(context: &PrepareContext<'_>, entry: PassEntry) -> Result<Self, FrameError> {
        Self::in_plan(context.passes(), entry)
    }

    /// The target of `entry` in an authored pass list, for a feature that checks a plan before
    /// the frame prepares.
    ///
    /// # Errors
    /// [`FrameError::MissingPass`] when the list has no such entry.
    pub fn in_plan(passes: &[PassDeclaration], entry: PassEntry) -> Result<Self, FrameError> {
        let index = passes
            .iter()
            .position(|pass| pass.entry() == entry)
            .ok_or(FrameError::MissingPass(entry))?;
        let pass = &passes[index];
        let attachments = pass
            .color
            .iter()
            .chain(&pass.additional_colors)
            .chain(&pass.depth);
        let extent = attachments.clone().map(|attachment| attachment.size).next();
        Ok(Self {
            entry,
            colors: pass
                .color
                .iter()
                .chain(&pass.additional_colors)
                .map(|color| color.format)
                .collect(),
            depth: pass.depth.as_ref().map(|depth| PassDepth {
                format: depth.format,
                mode: depth_mode(passes, index),
                writes: !depth.read_only,
            }),
            samples: attachments
                .map(|attachment| attachment.samples)
                .next()
                .unwrap_or(1),
            extent,
        })
    }

    pub fn entry(&self) -> PassEntry {
        self.entry
    }

    /// The colour attachment formats in location order.
    pub fn color_formats(&self) -> &[wgpu::TextureFormat] {
        &self.colors
    }

    pub fn depth_format(&self) -> Option<wgpu::TextureFormat> {
        self.depth.map(|depth| depth.format)
    }

    pub fn samples(&self) -> u32 {
        self.samples
    }

    /// The extent in texels of every attachment of the pass, which is also the viewport and
    /// scissor of a draw into it: the mip's extent for a pass that renders into a lower mip of a
    /// target.
    pub fn extent(&self) -> Option<[u32; 2]> {
        self.extent
    }

    /// Whether a draw into the pass writes depth.
    pub fn writes_depth(&self) -> bool {
        self.depth.is_some_and(|depth| depth.writes)
    }

    /// The attachments of the pass as the program runtime names them.
    ///
    /// # Errors
    /// The runtime's refusal of a signature it cannot draw into, such as more colour
    /// attachments than it supports.
    pub fn signature(&self) -> Result<TargetSignature, ProgramError> {
        let depth = self.depth_format();
        if self.colors.is_empty() {
            let depth = depth.ok_or(ProgramError::UnsupportedState {
                what: "a pass with no attachment",
            })?;
            TargetSignature::depth_attachment(depth, self.samples)
        } else {
            TargetSignature::attachments(&self.colors, depth, self.samples)
        }
    }

    /// The state of a draw into the pass that writes every colour attachment without blending,
    /// under the pass's depth and stencil rules, with `raster` as its raster state. `colors` has
    /// one entry for each colour attachment, in location order; a draw that blends sets its
    /// entries. Particles blended over the frame's colour at
    /// [`GamePlace::BeforePost`](worldrunner_render_post::GamePlace::BeforePost), for example,
    /// set `wgpu::BlendState::ALPHA_BLENDING` with `wgpu::ColorWrites::COLOR`, which keeps the
    /// colour's alpha.
    pub fn render_state(&self, raster: RasterState) -> RenderState {
        match self.depth {
            Some(depth) => depth
                .mode
                .render_state_writing(depth.writes, self.colors.len(), raster),
            None => RenderState {
                raster,
                depth: None,
                colors: vec![
                    Some(ColorState {
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    });
                    self.colors.len()
                ],
                stencil: None,
            },
        }
    }

    /// The frame's depth that `entry` attaches read-only, for the pass to sample as well as
    /// test, as a pass planned with
    /// [`DepthAccess::ReadOnlySampled`](super::DepthAccess::ReadOnlySampled) does. Bind
    /// [`AttachmentView::sampled_view`], which selects the depth aspect of a combined
    /// depth-stencil format, with [`AttachmentView::texture`]; it is multisampled when the frame
    /// is. Read it in `prepare` and `draw` of the frame that lends it and never keep it.
    ///
    /// # Errors
    /// [`FrameError::MissingPass`] when the frame has no such entry,
    /// [`FrameError::AttachmentContract`] when the entry attaches no depth,
    /// [`FrameError::AttachmentHazard`] naming the depth when the entry writes it or clears it,
    /// and [`FrameError::AttachmentView`] when the frame lends no view of it.
    pub fn sampled_depth<'a>(
        context: &'a PrepareContext<'_>,
        entry: PassEntry,
    ) -> Result<&'a AttachmentView, FrameError> {
        let depth = context
            .pass(entry)?
            .depth
            .as_ref()
            .ok_or(FrameError::AttachmentContract(entry))?;
        if !depth.read_only || depth.load != AttachmentLoad::Load {
            return Err(FrameError::AttachmentHazard(depth.resource));
        }
        context.resource(depth.resource)
    }

    /// Refuses a draw prepared for other attachments than this pass opens, by the name of the
    /// program and the entry, before anything records.
    ///
    /// # Errors
    /// [`FrameError::FeaturePreparation`] naming `program`, the entry and both signatures.
    pub fn admit(
        &self,
        program: &'static str,
        draw: &PreparedProgramDraw,
    ) -> Result<(), FrameError> {
        let expected = self
            .signature()
            .map_err(|error| refusal(program, self.entry, &error.to_string()))?;
        let prepared = draw.signature();
        if prepared == expected {
            return Ok(());
        }
        Err(refusal(
            program,
            self.entry,
            &format!("its draw targets {prepared:?}, but the pass opens {expected:?}"),
        ))
    }
}

fn refusal(program: &'static str, entry: PassEntry, why: &str) -> FrameError {
    FrameError::FeaturePreparation {
        feature: program,
        reason: format!("pass {entry}: {why}"),
    }
}

/// The depth mode the plan gives the pass at `index`.
fn depth_mode(passes: &[PassDeclaration], index: usize) -> DepthMode {
    let pass = &passes[index];
    let Some(depth) = pass.depth.as_ref() else {
        return DepthMode::Independent;
    };
    if !depth.format.has_stencil_aspect() {
        return DepthMode::Independent;
    }
    let completes = |earlier: &PassDeclaration| {
        earlier.slot == PassSlot::DepthNormalPrepass
            && earlier
                .depth
                .as_ref()
                .is_some_and(|written| written.resource == depth.resource && !written.read_only)
    };
    let reused = |later: &PassDeclaration| {
        later.slot == PassSlot::Opaque
            && later.depth.as_ref().is_some_and(|loaded| {
                loaded.resource == depth.resource && loaded.load == AttachmentLoad::Load
            })
    };
    match pass.slot {
        PassSlot::DepthNormalPrepass if passes[index + 1..].iter().any(reused) => {
            DepthMode::Prepass
        }
        PassSlot::Opaque
            if depth.load == AttachmentLoad::Load && passes[..index].iter().any(completes) =>
        {
            DepthMode::Reuse
        }
        _ => DepthMode::Shared,
    }
}
