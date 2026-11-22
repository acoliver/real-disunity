//! Instances a game shader reads from a raw buffer: the DOTS instancing form.
//!
//! A game that draws many instances of one mesh with a batch renderer fills a raw buffer with the
//! per-instance values (matrices, colours), tells each property where its values start with a
//! metadata word, and names the instances to draw. The shader's `DOTS_INSTANCING_ON` variant reads
//! them through four things, each found here by the name the variant declares:
//!
//! - the raw buffer `unity_DOTSInstanceData`, a read-only storage buffer;
//! - the metadata blocks, whose members `unity_DOTSInstancing<type><size>_Metadata<property>` each
//!   hold one word: the game's word for `<property>`, copied as it is;
//! - the visibility block, a struct array `unity_DOTSVisibleInstances` whose element `k` holds in
//!   `VisibleData.x` the index of the instance the draw's instance `k` shows.
//!
//! The host does not read the raw buffer or interpret a metadata word.

use crate::properties::{PropertySet, PropertyValue};
use crate::resources::BufferHandle;
use crate::unity::buffers::BufferLayout;

use crate::ids::{MaterialId, RendererId};

use super::error::OffscreenError;
use super::frame::{DotsBatch, OffscreenFrame};
use super::plan::Services;
use super::steps::{CameraDraw, Look, planned_draw};

/// The keyword of the variant that reads instances from a raw buffer.
pub(super) const KEYWORD: &str = "DOTS_INSTANCING_ON";

const INSTANCE_DATA: &str = "unity_DOTSInstanceData";
const VISIBILITY_BLOCK: &str = "UnityInstancingDOTS_InstanceVisibility";
const VISIBLE_MEMBER: &str = "VisibleData";
const METADATA_BLOCKS: [&str; 2] = [
    "UnityDOTSInstancing_BuiltinPropertyMetadata",
    "UnityDOTSInstancing_MaterialPropertyMetadata",
];
const MEMBER_PREFIX: &str = "unity_DOTSInstancing";
const METADATA_TAG: &str = "_Metadata";

/// Adds the keyword of the DOTS variant to a draw's keywords.
pub(super) fn select_variant(keywords: &mut Vec<&str>) {
    if !keywords.contains(&KEYWORD) {
        keywords.push(KEYWORD);
    }
}

/// A batch and the instances one draw of it makes: the visible instances from index `from` on.
#[derive(Clone, Copy)]
pub(super) struct DotsDraw<'a> {
    pub(super) batch: &'a DotsBatch,
    pub(super) from: usize,
}

impl DotsDraw<'_> {
    /// The buffer the variant's raw-buffer slot `name` reads, if `name` is that slot.
    pub(super) fn buffer(&self, name: &str) -> Option<BufferHandle> {
        (name == INSTANCE_DATA).then_some(self.batch.buffer)
    }

    /// The values the members of the constant block `layout` take from the batch: the game's
    /// words for the metadata members, and the draw's visible indexes for the visibility block.
    ///
    /// # Errors
    /// Fails on a member whose name is an index no table lists.
    pub(super) fn values(&self, layout: &BufferLayout) -> Result<PropertySet, OffscreenError> {
        let mut set = PropertySet::new();
        let block = layout.name.resolve(&[])?;
        if METADATA_BLOCKS.contains(&block) {
            for member in &layout.members {
                let name = member.name.resolve(&[])?;
                if let Some(word) = self.word(name) {
                    set.set(name, PropertyValue::Int(word.cast_signed()));
                }
            }
        } else if block == VISIBILITY_BLOCK {
            let indexes = self.batch.visible[self.from..]
                .iter()
                .map(|index| [index.cast_signed(), 0, 0, 0])
                .collect();
            set.set(VISIBLE_MEMBER, PropertyValue::IntVectorArray(indexes));
        }
        Ok(set)
    }

    /// The word the game gave the property that metadata member `member` is named for.
    fn word(&self, member: &str) -> Option<u32> {
        let (_, property) = member
            .strip_prefix(MEMBER_PREFIX)?
            .split_once(METADATA_TAG)?;
        self.batch
            .metadata
            .iter()
            .find_map(|(name, word)| (name == property).then_some(*word))
    }
}

/// The draws of a batch: one, whose instances are the batch's visible instances.
///
/// # Errors
/// Fails with the refusal that planning the draw gives. A draw left out for a counted cause gives
/// none.
pub(super) fn draws(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    look: &Look<'_>,
    ids: (MaterialId, RendererId, usize),
    batch: &DotsBatch,
) -> Result<Vec<CameraDraw>, OffscreenError> {
    let Ok(count) = u32::try_from(batch.visible.len()) else {
        return Err(OffscreenError::Unknown(
            "a batch of fewer instances".to_owned(),
        ));
    };
    if count == 0 {
        return Ok(Vec::new());
    }
    let dots = DotsDraw { batch, from: 0 };
    let look = Look {
        dots: Some(dots),
        ..*look
    };
    let planned = planned_draw(svc, frame, &look, ids)?;
    Ok(planned
        .into_iter()
        .map(|mut draw| {
            draw.geometry.instances = 0..count;
            draw
        })
        .collect())
}
