//! Wires the TAA resolve into the HDR post path.
//!
//! `HdrPost` builds one stage set per frame slot, but the history texture must see every
//! frame, so one texture per render size is shared by all slots. Each slot owns its output
//! texture and its uniform buffer.
use std::{cell::RefCell, rc::Rc};

use wgpu::util::DeviceExt;

use crate::{StageSet, TAA_UNIFORM_BYTES, TaaSettings, history_key::HistoryKey, taa_gpu};

/// The colour and linear-depth history textures of one render size. They are created, replaced
/// and released together, and both are reset by the CPU's `history_valid`.
#[derive(Debug)]
struct TaaHistory {
    key: HistoryKey,
    texture: wgpu::Texture,
    depth: wgpu::Texture,
}
impl TaaHistory {
    fn new(device: &wgpu::Device, key: HistoryKey) -> Self {
        let size = key.render();
        Self {
            key,
            texture: taa_gpu::history(device, size),
            depth: taa_gpu::depth_history(device, size),
        }
    }
}

/// The committed bank, which the live stage sets use, and the candidate bank a replacement under
/// construction has asked for. A candidate becomes the bank only when the owner of the replacement
/// has validated every slot's schedule.
#[derive(Debug, Default)]
struct Banks {
    committed: Option<TaaHistory>,
    candidate: Option<TaaHistory>,
}

/// History shared by the stage sets of every slot. Cloning shares it.
#[derive(Clone, Debug, Default)]
pub(crate) struct SharedHistory(Rc<RefCell<Banks>>);
impl SharedHistory {
    /// The colour and depth histories for `key`: the committed bank when it matches, otherwise a
    /// candidate shared by every slot of the same build. The committed bank is not touched.
    fn acquire(&self, device: &wgpu::Device, key: HistoryKey) -> (wgpu::Texture, wgpu::Texture) {
        let mut banks = self.0.borrow_mut();
        let Banks {
            committed,
            candidate,
        } = &mut *banks;
        let bank = match committed {
            Some(history) if history.key == key => history,
            _ => candidate.get_or_insert_with(|| TaaHistory::new(device, key)),
        };
        if bank.key != key {
            *bank = TaaHistory::new(device, key);
        }
        (bank.texture.clone(), bank.depth.clone())
    }

    /// Ends a build: an accepted build promotes its candidate, a rejected one drops it.
    pub fn settle(&self, accepted: bool) {
        let mut banks = self.0.borrow_mut();
        let candidate = banks.candidate.take();
        if accepted && candidate.is_some() {
            banks.committed = candidate;
        }
    }

    /// Drops the history, for settings that no longer use TAA.
    pub fn release(&self) {
        *self.0.borrow_mut() = Banks::default();
    }

    #[cfg(test)]
    pub fn size(&self) -> Option<[u32; 2]> {
        let banks = self.0.borrow();
        banks.committed.as_ref().map(|history| history.key.render())
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

/// Schedules "taa resolve", "taa history" and "taa depth history" at the end of `set` and returns the resolved
/// color. `set` owns the output; the shared history is also kept alive by it.
pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    targets: &crate::FrameTargets,
    (settings, history, uniform): (TaaSettings, &SharedHistory, &wgpu::Buffer),
) -> wgpu::TextureView {
    let config = targets.config();
    let (history_texture, depth_history) =
        history.acquire(device, HistoryKey::new(config.size, None, config.samples));
    let output = taa_gpu::output(device, config.size);
    let depth_output = taa_gpu::depth_output(device, config.size);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    taa_gpu::append(
        device,
        set,
        &taa_gpu::Inputs {
            color: targets.resolved(),
            depth: targets.depth(),
            depth_samples: config.samples,
            velocity: targets.velocity(),
            velocity_samples: config.samples,
            uniform,
            history: &history_texture,
            output: &output,
            depth_history: &depth_history,
            depth_output: &depth_output,
        },
        settings,
    );
    set.resources.textures.push(output);
    set.resources.textures.push(history_texture);
    set.resources.textures.push(depth_output);
    set.resources.textures.push(depth_history);
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
