//! The places a game's passes take in the pass list, and what a feature reads of the passes
//! it records into.
use super::{FrameError, PassDeclaration, PassEntry, PrepareContext};

/// Where a game's own pass sits among the stock passes of a forward frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GamePlace {
    /// Immediately after the depth/normal prepass, before the depth pyramid, the occlusion
    /// metadata and SSAO read what it wrote. These passes load and write the prepass's depth and
    /// colour products, and their depth writes complete the depth with the prepass's, so every
    /// reader of the depth prefix sees what the game drew here. In a frame whose depth prefix is
    /// submitted ahead of the rest, they run in that prefix.
    AfterDepthPrepass,
    /// Immediately before the first Opaque entry, after the G-buffer and its lighting when the
    /// frame has them. Opaque loads what these passes left in the attachments they share.
    BeforeOpaque,
    /// Immediately after the last Opaque entry, before the contour IDs and the opaque snapshots,
    /// so the snapshots include what these passes drew.
    AfterOpaque,
    /// After the transparent passes and the post-water depth, before the reflection and
    /// screen-space passes that follow transparency.
    AfterTransparent,
}

impl PrepareContext<'_> {
    /// The frame's validated passes in the order they run, with the attachments each one is
    /// opened with. Empty outside a frame's own preparation.
    pub fn passes(&self) -> &[PassDeclaration] {
        &self.passes
    }
    /// The validated declaration of `entry`, for a feature that must match the pass it records
    /// into.
    ///
    /// # Errors
    /// [`FrameError::MissingPass`] when the frame has no such entry.
    pub fn pass(&self, entry: PassEntry) -> Result<&PassDeclaration, FrameError> {
        self.passes
            .iter()
            .find(|pass| pass.entry() == entry)
            .ok_or(FrameError::MissingPass(entry))
    }
    /// The frame's encoder, for calls that take one only to name the frame they belong to, such
    /// as retaining a prepared draw's resources until the frame completes. Features record
    /// nothing through it.
    pub fn encoder(&self) -> &wgpu::CommandEncoder {
        self.encoder
    }
}
