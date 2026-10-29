//! Wires the TAA resolve into the HDR post path.
//!
//! `HdrPost` builds one stage set per frame slot. Each slot's resolve writes its own output
//! texture and reads the output of the slot encoded before it as history, so no copy pass is
//! needed. The outputs of one render size are created once and shared by every slot build.
//! Hosts encode slots in the order 0, 1, 2, 0, ...; [`SharedOutputs::admit`] checks that on
//! every upload and drops the history of a frame that breaks it.
use std::{cell::RefCell, rc::Rc};

use wgpu::util::DeviceExt;

use crate::{StageSet, TAA_UNIFORM_BYTES, TaaSettings, taa_gpu};

/// Frame slots of an HDR post path, and so the number of resolve outputs.
pub(crate) const SLOTS: usize = 3;

/// The resolve outputs of one render size, one per frame slot.
#[derive(Debug)]
struct TaaOutputs {
    size: [u32; 2],
    textures: [wgpu::Texture; SLOTS],
    views: [wgpu::TextureView; SLOTS],
    /// The slot whose resolve was uploaded last, or none since the outputs were created.
    last_slot: Option<usize>,
}

/// The slot encoded before `slot` in the rotation.
fn predecessor(slot: usize) -> usize {
    (slot + SLOTS - 1) % SLOTS
}

/// Outputs shared by the stage sets of every slot. Cloning shares them.
#[derive(Clone, Debug, Default)]
pub(crate) struct SharedOutputs(Rc<RefCell<Option<TaaOutputs>>>);
impl SharedOutputs {
    /// The output of `slot` and the history it reads (the output of the previous slot), as
    /// texture and view pairs. The outputs for `size` are created on first use and replaced
    /// when the size changes.
    fn acquire(&self, device: &wgpu::Device, size: [u32; 2], slot: usize) -> [Pair; 2] {
        let mut state = self.0.borrow_mut();
        if state.as_ref().is_none_or(|outputs| outputs.size != size) {
            let textures: [wgpu::Texture; SLOTS] =
                std::array::from_fn(|_| taa_gpu::output(device, size));
            let views = std::array::from_fn(|index| {
                textures[index].create_view(&wgpu::TextureViewDescriptor::default())
            });
            *state = Some(TaaOutputs {
                size,
                textures,
                views,
                last_slot: None,
            });
        }
        let outputs = state.as_ref().expect("outputs were just created");
        [slot, predecessor(slot)].map(|index| {
            (
                outputs.textures[index].clone(),
                outputs.views[index].clone(),
            )
        })
    }

    /// Records that `slot` is about to encode its resolve and returns the block to upload. A
    /// block that claims valid history loses the claim unless the previous resolve was the
    /// one of the preceding slot, because only then does the history view hold the previous
    /// frame.
    pub fn admit(
        &self,
        slot: usize,
        block: Option<[u8; TAA_UNIFORM_BYTES]>,
    ) -> Option<[u8; TAA_UNIFORM_BYTES]> {
        let mut state = self.0.borrow_mut();
        let outputs = state.as_mut()?;
        let in_order = outputs.last_slot == Some(predecessor(slot));
        outputs.last_slot = Some(slot);
        block.map(|block| {
            if in_order {
                block
            } else {
                crate::taa::without_history(block)
            }
        })
    }

    /// Drops the outputs, for settings that no longer use TAA.
    pub fn release(&self) {
        *self.0.borrow_mut() = None;
    }

    #[cfg(test)]
    pub fn size(&self) -> Option<[u32; 2]> {
        self.0.borrow().as_ref().map(|outputs| outputs.size)
    }
}

type Pair = (wgpu::Texture, wgpu::TextureView);

/// The scene color the HDR path reads, and the frame it belongs to.
pub(crate) struct Scene<'a> {
    pub targets: &'a crate::FrameTargets,
    pub color: &'a wgpu::TextureView,
}

/// A slot's uniform buffer, zeroed: no history until the first upload.
pub(crate) fn uniform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("slot TAA block"),
        size: block_size(),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Schedules "taa resolve" at the end of `set` and returns the resolved color. The resolve
/// writes the output of `slot` and reads the output of the previous slot as history. `set`
/// keeps both textures alive.
pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    targets: &crate::FrameTargets,
    (settings, outputs, uniform, slot): (TaaSettings, &SharedOutputs, &wgpu::Buffer, usize),
) -> wgpu::TextureView {
    let config = targets.config();
    let [(output, output_view), (history, history_view)] =
        outputs.acquire(device, config.size, slot);
    taa_gpu::append(
        device,
        set,
        &taa_gpu::Inputs {
            color: targets.resolved(),
            depth: targets.depth(),
            depth_samples: config.samples,
            uniform,
            history: &history_view,
            output: &output_view,
        },
        settings,
    );
    set.resources.textures.push(output);
    set.resources.textures.push(history);
    output_view
}

/// Copies `block` (zeros for `None`) into `destination` through an immutable staging buffer.
pub(crate) fn upload(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Buffer,
    block: Option<&[u8; TAA_UNIFORM_BYTES]>,
) {
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("immutable TAA block"),
        contents: block.unwrap_or(&[0; TAA_UNIFORM_BYTES]),
        usage: wgpu::BufferUsages::COPY_SRC,
    });
    encoder.copy_buffer_to_buffer(&staging, 0, destination, 0, block_size());
}

fn block_size() -> wgpu::BufferAddress {
    wgpu::BufferAddress::try_from(TAA_UNIFORM_BYTES).expect("the block size fits a buffer address")
}

#[cfg(test)]
#[path = "hdr_taa_order_tests.rs"]
mod order_tests;
