//! Counts what the retained viewer draws in place of what the game asked for.
//!
//! A mesh renderer whose first material slot has no texture the viewer can use draws with the
//! 1x1 white texture, a renderer whose mesh has not arrived is left out of the window, and a
//! renderer's slots after the first are not drawn, because the viewer's object holds one material.
//! All three happen silently, so the tables count them each frame and the run report says how
//! many.

use std::collections::BTreeMap;

use crate::counts_line::top_counts_line;

/// More names than any census line has reasons, so no line ends in `and N more`.
const REASONS: usize = 8;

/// Why a drawn renderer uses the white texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WhiteReason {
    /// The renderer lists no material slot.
    NoMaterialSlot,
    /// The slot's live material has no asset main texture, so the world gives the slot texture
    /// identity 0: the material is unset or its main texture is not a pixel-backed texture.
    TextureZero,
    /// The managed host has not stored the texels of the texture the slot names.
    TexelsNotStored,
    /// The managed host stored the texture and its conversion to scene texels failed.
    ConversionFailed,
    /// The viewer's texture table has no handle left for the texture.
    HandleOverflow,
}

impl WhiteReason {
    /// The name the run report uses for the reason.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::NoMaterialSlot => "no-material-slot",
            Self::TextureZero => "texture-zero",
            Self::TexelsNotStored => "texels-not-stored",
            Self::ConversionFailed => "conversion-failed",
            Self::HandleOverflow => "handle-overflow",
        }
    }
}

/// Why a renderer the frame lists is not in the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeftOut {
    /// The renderer's mesh has no converted content yet.
    MeshNotArrived,
    /// The viewer's mesh table has no handle left for the mesh.
    MeshHandleOverflow,
}

impl LeftOut {
    /// The name the run report uses for the reason.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::MeshNotArrived => "mesh-not-arrived",
            Self::MeshHandleOverflow => "mesh-handle-overflow",
        }
    }
}

/// What one frame's renderers drew with, as the tables place them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FrameCensus {
    drawn: usize,
    white: BTreeMap<WhiteReason, usize>,
    left_out: BTreeMap<LeftOut, usize>,
    slots_not_drawn: usize,
}

impl FrameCensus {
    /// Counts a renderer the frame draws, with the reason it uses the white texture if it does.
    pub fn drawn(&mut self, white: Option<WhiteReason>) {
        self.drawn += 1;
        if let Some(reason) = white {
            *self.white.entry(reason).or_default() += 1;
        }
    }

    /// Counts the material slots of a drawn renderer that the frame does not draw.
    pub fn slots_not_drawn(&mut self, slots: usize) {
        self.slots_not_drawn += slots;
    }

    /// Counts a renderer the frame leaves out.
    pub fn left_out(&mut self, reason: LeftOut) {
        *self.left_out.entry(reason).or_default() += 1;
    }
}

/// The white substitutions and left-out renderers of the last frame, and the worst frame of the
/// run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubstitutionCensus {
    /// The renderers the last frame drew: the denominator of the white counts.
    pub drawn: usize,
    /// The renderers the last frame drew with the white texture, by reason.
    pub last_frame: BTreeMap<WhiteReason, usize>,
    /// The most renderers any one frame drew with the white texture for each reason.
    pub most_in_one_frame: BTreeMap<WhiteReason, usize>,
    /// The renderers the last frame left out, by reason.
    pub left_out: BTreeMap<LeftOut, usize>,
    /// The material slots after the first, over the renderers the last frame drew, which the
    /// viewer does not draw.
    pub slots_not_drawn: usize,
    /// The UI draws the last frame left out because their material is a game's own, by the
    /// name of the material's shader.
    pub ui_game_materials: BTreeMap<String, usize>,
}

/// `counts` under the names `label` gives their keys, for the ranked lines.
fn by_name<K: Copy>(
    counts: &BTreeMap<K, usize>,
    label: fn(K) -> &'static str,
) -> BTreeMap<String, usize> {
    counts
        .iter()
        .map(|(reason, count)| (label(*reason).to_owned(), *count))
        .collect()
}

impl SubstitutionCensus {
    /// Takes `frame` as the last frame.
    pub fn end_frame(&mut self, frame: FrameCensus) {
        for (reason, count) in &frame.white {
            let most = self.most_in_one_frame.entry(*reason).or_default();
            *most = (*most).max(*count);
        }
        self.drawn = frame.drawn;
        self.last_frame = frame.white;
        self.left_out = frame.left_out;
        self.slots_not_drawn = frame.slots_not_drawn;
    }

    /// The report lines for the census, without the `[gpu] ` prefix. A reason with no renderers
    /// has no line.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let white: usize = self.last_frame.values().sum();
        let mut lines = vec![format!(
            "retained renderers with texture 0 (last frame) {white} of {}",
            self.drawn
        )];
        let ranked = [
            (
                "retained renderers with texture 0 by reason (last frame)",
                by_name(&self.last_frame, WhiteReason::label),
            ),
            (
                "retained renderers with texture 0 by reason (most in one frame)",
                by_name(&self.most_in_one_frame, WhiteReason::label),
            ),
            (
                "retained objects left out",
                by_name(&self.left_out, LeftOut::label),
            ),
            (
                "UI draws with a game material by shader (last frame)",
                self.ui_game_materials.clone(),
            ),
        ];
        lines.extend(
            ranked
                .iter()
                .filter_map(|(title, counts)| top_counts_line(title, counts, REASONS)),
        );
        if self.slots_not_drawn > 0 {
            lines.push(format!(
                "retained renderer material slots not drawn (last frame) {}",
                self.slots_not_drawn
            ));
        }
        lines
    }
}

#[cfg(test)]
#[path = "scene_census_tests.rs"]
mod tests;
