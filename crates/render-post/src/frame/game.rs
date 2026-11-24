//! The places a game's passes take in the pass list, and what a feature reads of the passes
//! it records into.
use super::{FrameError, PassDeclaration, PassEntry, PrepareContext};

/// Where a game's own pass sits among the stock passes of a forward frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GamePlace {
    /// Immediately after the depth/normal prepass, before the depth pyramid, the occlusion
    /// metadata and SSAO read what it wrote. These passes load and write the prepass's depth and
    /// colour products, and their depth writes complete the depth with the prepass's, so every
    /// reader of the depth prefix sees what the game drew here. In a frame whose depth prefix is
    /// submitted ahead of the rest, they run in that prefix.
    AfterDepthPrepass,
    /// Immediately before the first Opaque entry. Opaque loads what these passes left in the
    /// attachments they share.
    BeforeOpaque,
    /// Immediately after the last Opaque entry, before the contour IDs and the opaque snapshots,
    /// so the snapshots include what these passes drew.
    AfterOpaque,
    /// After the opaque snapshot passes (the contour IDs, the colour snapshot and the depth
    /// snapshots) and before the water, the temporal-mask producers and every other transparent
    /// pass. The frame has the snapshots of the opaque scene by now, so a pass here samples them
    /// and draws over the lit opaque colour that the transparent passes then draw over. It needs
    /// no snapshot of its own to be placed here; it orders after the snapshot writers only when
    /// it declares a read of one.
    AfterOpaqueSnapshots,
    /// After the transparent passes and the post-water depth, before the reflection and
    /// screen-space passes that follow transparency.
    AfterTransparent,
    /// The last place of the frame: after every pass that writes the lit scene colour (Opaque,
    /// the water and transparent passes, the stock reflections and screen-space pass after
    /// transparency, and the game's passes at every other place), and
    /// before the post chain (resolve, bloom, tone mapping) reads that colour. A pass here
    /// draws over the finished scene colour, for example blended particles, and nothing in the
    /// frame draws over it. A frame without HDR post copies the colour to its output as it is,
    /// so the place is still the colour's last writer.
    BeforePost,
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
