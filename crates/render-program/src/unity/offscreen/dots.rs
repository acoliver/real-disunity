//! Instances a game shader reads from a raw buffer: the DOTS instancing form.
//!
//! A game that draws many instances of one mesh with a batch renderer fills a raw buffer with the
//! per-instance values (matrices, colours), tells each property where its values start with a
//! metadata word, and names the instances to draw. The shader's `DOTS_INSTANCING_ON` variant reads
//! them through these things, each found here by the name the variant declares:
//!
//! - the raw buffer `unity_DOTSInstanceData`, a read-only storage buffer;
//! - the metadata blocks, whose members `unity_DOTSInstancing<type><size>_Metadata<property>` each
//!   hold one word: the game's word for `<property>`, copied as it is;
//! - the visibility block, a struct array `unity_DOTSVisibleInstances` whose element `k` holds in
//!   `VisibleData.x` the index of the instance the draw's instance `k` shows;
//! - the block `unity_DOTSInstanceGlobalValues`, whose member `unity_DOTS_<name>` holds the value
//!   the variant reads for the builtin `unity_<name>` (the ambient spherical harmonics and the
//!   reflection decode) when the batch gives no per-instance value for it: the value the
//!   builtin has for any other draw.
//!
//! The host does not read the raw buffer or interpret a metadata word.

use crate::properties::{PropertySet, PropertyValue, lookup};
use crate::resources::BufferHandle;
use crate::unity::buffers::BufferLayout;

use std::ops::Range;

use crate::ids::{MaterialId, RendererId};

use super::defaults::DefaultBuffers;
use super::error::OffscreenError;
use super::frame::{DotsBatch, Instances, OffscreenFrame, Record};
use super::plan::{ProgramEntry, Services};
use super::steps::{CameraDraw, Look, planned_draw};

/// The keyword of the variant that reads instances from a raw buffer.
pub(super) const KEYWORD: &str = "DOTS_INSTANCING_ON";

const INSTANCE_DATA: &str = "unity_DOTSInstanceData";
const VISIBILITY_BLOCK: &str = "UnityInstancingDOTS_InstanceVisibility";
const INDIRECT_VISIBILITY: &str = "unity_DOTSInstancing_IndirectInstanceVisibility";
const VISIBLE_MEMBER: &str = "VisibleData";
const METADATA_BLOCKS: [&str; 2] = [
    "UnityDOTSInstancing_BuiltinPropertyMetadata",
    "UnityDOTSInstancing_MaterialPropertyMetadata",
];
/// The block of the values a variant reads for a builtin when no instance gives one, and the
/// prefix of its members: `unity_DOTS_<name>` is the value of the builtin `unity_<name>`.
const GLOBAL_VALUES_BLOCK: &str = "unity_DOTSInstanceGlobalValues";
const GLOBAL_VALUE_PREFIX: &str = "unity_DOTS_";
const MEMBER_PREFIX: &str = "unity_DOTSInstancing";
const METADATA_TAG: &str = "_Metadata";

/// Adds the keyword of the DOTS variant to a draw's keywords.
pub(super) fn select_variant(keywords: &mut Vec<&str>) {
    if !keywords.contains(&KEYWORD) {
        keywords.push(KEYWORD);
    }
}

/// Whether a list of the frame draws a batch.
pub(super) fn frame_has_batch(frame: &OffscreenFrame) -> bool {
    frame.records.iter().any(|record| match record {
        Record::Draws(list) => list
            .draws
            .iter()
            .any(|item| matches!(item.instances, Instances::Dots(_))),
        _ => false,
    })
}

/// A batch and the instances one draw of it makes: the visible instances from index `from` on.
#[derive(Clone, Copy)]
pub(super) struct DotsDraw<'a> {
    pub(super) batch: &'a DotsBatch,
    pub(super) from: usize,
}

impl DotsDraw<'_> {
    /// The buffer and the bytes of it the variant's raw-buffer slot `name` reads, if the batch
    /// gives that slot its data.
    pub(super) fn buffer(&self, name: &str) -> Option<(BufferHandle, Range<u64>)> {
        (name == INSTANCE_DATA).then(|| (self.batch.buffer, self.batch.window.clone()))
    }

    /// The buffer the slot `name` reads when nothing gives it one: the indirect visibility the
    /// batch has none of reads zeros.
    pub(super) fn placeholder(name: &str, defaults: DefaultBuffers) -> Option<BufferHandle> {
        if name == INDIRECT_VISIBILITY {
            defaults.indirect_visibility()
        } else {
            None
        }
    }

    /// The visible indexes this draw shows: those from `from` on, as many as `capacity` holds.
    pub(super) fn indexes(&self, capacity: usize) -> &[u32] {
        let rest = &self.batch.visible[self.from..];
        &rest[..rest.len().min(capacity)]
    }

    /// The values the members of the constant block `layout` take from the batch: the game's
    /// words for the metadata members, the draw's visible indexes for the visibility block, and
    /// for the block of global values the value `layers` give the builtin each member is the
    /// default of.
    ///
    /// # Errors
    /// Fails on a member whose name is an index no table lists.
    pub(super) fn values(
        &self,
        layout: &BufferLayout,
        layers: &[&PropertySet],
    ) -> Result<PropertySet, OffscreenError> {
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
            let indexes = self
                .indexes(capacity(layout)?)
                .iter()
                .map(|index| [index.cast_signed(), 0, 0, 0])
                .collect();
            set.set(VISIBLE_MEMBER, PropertyValue::IntVectorArray(indexes));
        } else if block == GLOBAL_VALUES_BLOCK {
            for member in &layout.members {
                let name = member.name.resolve(&[])?;
                let builtin = name
                    .strip_prefix(GLOBAL_VALUE_PREFIX)
                    .map(|rest| format!("unity_{rest}"));
                if let Some(value) = builtin.and_then(|builtin| lookup(layers, &builtin)) {
                    set.set(name, value.clone());
                }
            }
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

/// How many instances one draw of the variant `entry` can show: the length of the visibility
/// block's struct array. A variant with no such block shows as many as it is given.
fn block_capacity(entry: &ProgramEntry) -> Result<usize, OffscreenError> {
    for target in &entry.constants {
        if target.buffer.name.resolve(&[])? == VISIBILITY_BLOCK {
            return capacity(&target.buffer);
        }
    }
    Ok(usize::MAX)
}

fn capacity(layout: &BufferLayout) -> Result<usize, OffscreenError> {
    let placement = layout.struct_placement(&[])?;
    Ok(placement.map_or(usize::MAX, |(_, placement)| placement.count))
}

/// The draws of a batch: one for each run of visible instances that fits the variant's
/// visibility block, in order. The instance index of a draw starts at 0, so each draw's block
/// holds the indexes of its own run.
///
/// # Errors
/// Fails with the refusal that planning a draw gives. A draw left out for a counted cause ends
/// the batch's draws there.
pub(super) fn draws(
    svc: &mut Services<'_>,
    frame: &OffscreenFrame,
    look: &Look<'_>,
    ids: (MaterialId, RendererId, usize),
    batch: &DotsBatch,
) -> Result<Vec<CameraDraw>, OffscreenError> {
    let mut draws = Vec::new();
    let mut from = 0;
    while from < batch.visible.len() {
        let dots = DotsDraw { batch, from };
        let look = Look {
            dots: Some(dots),
            ..*look
        };
        let Some(mut draw) = planned_draw(svc, frame, &look, ids)? else {
            break;
        };
        let taken = dots.indexes(block_capacity(&draw.plan.entry)?).len();
        let count = u32::try_from(taken).map_err(|_| {
            OffscreenError::Unknown("fewer than 2^32 instances in one draw".to_owned())
        })?;
        draw.geometry.instances = 0..count;
        draws.push(draw);
        from += taken;
    }
    Ok(draws)
}
