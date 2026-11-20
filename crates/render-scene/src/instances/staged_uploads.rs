//! Sparse physical bytes encoded by one still-preparing transaction.

use std::{collections::BTreeMap, ops::Range};

use worldrunner_render_gpu::submission::{Publications, TransactionStatus};

use crate::{geometry::INSTANCE_BYTES, skinning::SKIN_INSTANCE_BYTES};

use super::skin_stream::SkinStream;

#[derive(Debug)]
struct Slot {
    current: [u8; INSTANCE_BYTES],
    skin: Option<[u8; SKIN_INSTANCE_BYTES]>,
}

#[derive(Debug, Default)]
pub(crate) struct StagedUploads {
    owner: Option<Publications>,
    slots: BTreeMap<usize, Slot>,
}

impl StagedUploads {
    fn belongs_to(&self, owner: Option<&Publications>) -> bool {
        owner.is_some_and(|owner| {
            owner.status() == TransactionStatus::Preparing
                && self
                    .owner
                    .as_ref()
                    .is_some_and(|staged| staged.same_transaction(owner))
        })
    }

    /// Later copies replace earlier bytes only for the slots they actually write.
    pub fn record(
        &mut self,
        owner: &Publications,
        chunks: &[(Range<usize>, Vec<u8>)],
        skin: Option<&SkinStream>,
    ) {
        if !self.belongs_to(Some(owner)) {
            self.slots.clear();
            self.owner = Some(owner.clone());
        }
        for (range, bytes) in chunks {
            for (offset, current) in bytes.chunks_exact(INSTANCE_BYTES).enumerate() {
                let position = range.start / INSTANCE_BYTES + offset;
                let skin = skin.map(|stream| {
                    let start = position * SKIN_INSTANCE_BYTES;
                    stream.bytes[start..start + SKIN_INSTANCE_BYTES]
                        .try_into()
                        .expect("skin descriptor stride")
                });
                self.slots.insert(
                    position,
                    Slot {
                        current: current.try_into().expect("instance stride"),
                        skin,
                    },
                );
            }
        }
    }

    /// Unwritten slots and closed or foreign transactions use the submitted baseline.
    pub fn differs(
        &self,
        owner: Option<&Publications>,
        position: usize,
        current: &[u8],
        skin: Option<&[u8]>,
    ) -> Option<bool> {
        if !self.belongs_to(owner) {
            return None;
        }
        self.slots.get(&position).map(|slot| {
            slot.current.as_slice() != current
                || slot
                    .skin
                    .as_ref()
                    .map(<[u8; SKIN_INSTANCE_BYTES]>::as_slice)
                    != skin
        })
    }
}
