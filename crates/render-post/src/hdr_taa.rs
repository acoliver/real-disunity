//! Wires the TAA resolve into the HDR post path.
//!
//! `HdrPost` builds one stage set per frame slot, but the history texture must see every
//! frame, so one texture per render size is shared by all slots. Each slot owns its output
//! texture and its uniform buffer.
use std::{cell::RefCell, rc::Rc};

use wgpu::util::DeviceExt;

use crate::{StageSet, TAA_UNIFORM_BYTES, TaaSettings, taa_gpu};

/// The history texture of one render size.
#[derive(Debug)]
struct TaaHistory {
    size: [u32; 2],
    texture: wgpu::Texture,
}

/// History shared by the stage sets of every slot. Cloning shares it.
#[derive(Clone, Debug, Default)]
pub(crate) struct SharedHistory(Rc<RefCell<Option<TaaHistory>>>);
impl SharedHistory {
    /// The history for `size`, created on first use and replaced when the size changes.
    fn acquire(&self, device: &wgpu::Device, size: [u32; 2]) -> wgpu::Texture {
        let mut slot = self.0.borrow_mut();
        if slot.as_ref().is_none_or(|history| history.size != size) {
            *slot = Some(TaaHistory {
                size,
                texture: taa_gpu::history(device, size),
            });
        }
        slot.as_ref()
            .expect("history was just created")
            .texture
            .clone()
    }

    /// Drops the history, for settings that no longer use TAA.
    pub fn release(&self) {
        *self.0.borrow_mut() = None;
    }

    #[cfg(test)]
    pub fn size(&self) -> Option<[u32; 2]> {
        self.0.borrow().as_ref().map(|history| history.size)
    }
}

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

/// Schedules "taa resolve" and "taa history" at the end of `set` and returns the resolved
/// color. `set` owns the output; the shared history is also kept alive by it.
pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    targets: &crate::FrameTargets,
    (settings, history, uniform): (TaaSettings, &SharedHistory, &wgpu::Buffer),
) -> wgpu::TextureView {
    let config = targets.config();
    let history_texture = history.acquire(device, config.size);
    let output = taa_gpu::output(device, config.size);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    taa_gpu::append(
        device,
        set,
        &taa_gpu::Inputs {
            color: targets.resolved(),
            depth: targets.depth(),
            depth_samples: config.samples,
            velocity: None,
            velocity_samples: config.samples,
            uniform,
            history: &history_texture,
            output: &output,
        },
        settings,
    );
    set.resources.textures.push(output);
    set.resources.textures.push(history_texture);
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
