//! The auxiliary skin stream of a batch whose mesh is skinned.
//!
//! One 16-byte descriptor per slot, `(current offset, previous offset, joints, flags)` as
//! little-endian `u32`s, read at vertex location 15 by skinned programs. The stream mirrors
//! the batch's instance buffer slot for slot, so every range the instance upload writes is
//! written here too and a gathered view buffer can copy both with the same slot ranges.

use std::cell::RefCell;
use std::rc::Rc;

use crate::skinning::SKIN_INSTANCE_BYTES as STRIDE;

#[derive(Debug, Default)]
pub(crate) struct SkinStream {
    pub bytes: Vec<u8>,
    pub uploaded: Rc<RefCell<Vec<u8>>>,
    pub buffer: Option<worldrunner_render_gpu::census::Tracked<wgpu::Buffer>>,
}

impl SkinStream {
    /// Stores the descriptor of `slot`, growing the stream to hold it.
    pub fn set(&mut self, slot: usize, descriptor: [u32; 4]) {
        if self.bytes.len() < (slot + 1) * STRIDE {
            self.bytes.resize((slot + 1) * STRIDE, 0);
        }
        for (lane, word) in descriptor.into_iter().enumerate() {
            let start = slot * STRIDE + lane * 4;
            self.bytes[start..start + 4].copy_from_slice(&word.to_le_bytes());
        }
    }

    /// Whether the descriptor of `slot` differs from the last published one.
    pub fn differs(&self, slot: usize) -> bool {
        let range = slot * STRIDE..(slot + 1) * STRIDE;
        self.uploaded.borrow().get(range.clone()) != self.bytes.get(range)
    }

    /// The bytes of the slots `range` of the stream.
    pub fn slots(&self, range: &std::ops::Range<u32>) -> &[u8] {
        &self.bytes[range.start as usize * STRIDE..range.end as usize * STRIDE]
    }

    /// Stages the descriptors of the slot `ranges` for upload, and returns what a published
    /// frame records as uploaded.
    pub fn stage(
        &self,
        context: &mut worldrunner_render_post::PrepareContext<'_>,
        ranges: &[std::ops::Range<u32>],
    ) -> (Rc<RefCell<Vec<u8>>>, Vec<u8>) {
        let buffer = self.buffer.as_deref().expect("allocated skin buffer");
        for range in ranges {
            context.upload_buffer(
                buffer,
                u64::from(range.start) * STRIDE as u64,
                self.slots(range),
            );
        }
        (self.uploaded.clone(), self.bytes.clone())
    }

    /// Whether the buffer can hold every slot.
    pub fn fits(&self) -> bool {
        self.buffer
            .as_deref()
            .is_some_and(|buffer| buffer.size() >= self.bytes.len() as u64)
    }
}
