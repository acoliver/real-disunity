//! The renderers a camera render draws, as the world fixed them when the script called it.

use std::collections::BTreeSet;
use std::sync::Arc;

use worldrunner_render_program::ids::RendererId;

use super::frame::{Mapper, RendererSkip};

/// A set of renderers, with the index the snapshot lists it under so that records which share a
/// set share a camera of the frame.
#[derive(Clone)]
pub(super) struct RecordSet {
    pub(super) id: u32,
    pub(super) renderers: Arc<BTreeSet<RendererId>>,
}

impl Mapper<'_> {
    /// The set of the camera render at `record` of the snapshot's work, when the world fixed one.
    /// A renderer's id is its index in the snapshot's renderers, as `Mapper::renderers` names it.
    pub(super) fn record_set(&mut self, record: usize) -> Option<RecordSet> {
        let id = *self
            .snapshot
            .record_sets
            .get(&u32::try_from(record).ok()?)?;
        let listed = self.snapshot.renderer_sets.get(usize::try_from(id).ok()?)?;
        let renderers = self.sets.entry(id).or_insert_with(|| {
            Arc::new(
                listed
                    .iter()
                    .map(|index| RendererId(u64::from(*index)))
                    .collect(),
            )
        });
        Some(RecordSet {
            id,
            renderers: Arc::clone(renderers),
        })
    }
}

impl Mapper<'_> {
    /// Counts the skinned renderers the world left out of every set because their skins name no
    /// root bone to place them. The snapshot gives each once for the frame.
    pub(super) fn skinned_renderers(&mut self) {
        let count = self.snapshot.skinned_not_drawn as usize;
        if count > 0 {
            *self
                .report
                .renderers_skipped
                .entry(RendererSkip::SkinnedNotDrawn)
                .or_default() += count;
        }
    }
}
