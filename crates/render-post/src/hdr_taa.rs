//! Wires the TAA resolve into the HDR post path.
//!
//! `HdrPost` builds one stage set per frame slot, but the history texture must see every
//! frame, so one texture per render size is shared by all slots. Each slot owns its output
//! texture and its uniform buffer.
use std::{cell::RefCell, rc::Rc};

use wgpu::util::DeviceExt;

use crate::{
    StageSet, TAA_UNIFORM_BYTES, TEMPORAL_CAMERA_BYTES, TaaSettings, TemporalBlock, TemporalCamera,
    history_key::HistoryKey, taa_gpu, taa_upscale_gpu,
};

/// The history textures of one bank: colour and linear depth on the render grid for native TAA,
/// colour alone on the output grid for temporal reconstruction. They are created, replaced and
/// released together, and reset by the CPU's `history_valid`.
#[derive(Clone, Debug)]
struct TaaHistory {
    key: HistoryKey,
    texture: wgpu::Texture,
    depth: Option<wgpu::Texture>,
}
impl TaaHistory {
    fn new(device: &wgpu::Device, key: HistoryKey) -> Self {
        if key.temporal() {
            Self {
                key,
                texture: taa_upscale_gpu::history(device, key.history_extent()),
                depth: None,
            }
        } else {
            let size = key.render();
            Self {
                key,
                texture: taa_gpu::history(device, size),
                depth: Some(taa_gpu::depth_history(device, size)),
            }
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
    /// Settings that no longer use TAA asked to drop the committed bank when their build is accepted.
    release_pending: bool,
}

/// History shared by the stage sets of every slot. Cloning shares it.
#[derive(Clone, Debug, Default)]
pub(crate) struct SharedHistory(Rc<RefCell<Banks>>);
impl SharedHistory {
    /// The bank for `key`: the committed one when it matches, otherwise a candidate shared by
    /// every slot of the same build. The committed bank is not touched.
    fn acquire(&self, device: &wgpu::Device, key: HistoryKey) -> TaaHistory {
        let mut banks = self.0.borrow_mut();
        let Banks {
            committed,
            candidate,
            ..
        } = &mut *banks;
        let bank = match committed {
            Some(history) if history.key == key => history,
            _ => candidate.get_or_insert_with(|| TaaHistory::new(device, key)),
        };
        if bank.key != key {
            *bank = TaaHistory::new(device, key);
        }
        bank.clone()
    }

    /// Ends a build: an accepted build promotes its candidate (or drops the committed bank when
    /// a release was requested), a rejected one drops the candidate and the request.
    pub fn settle(&self, accepted: bool) {
        let mut banks = self.0.borrow_mut();
        let candidate = banks.candidate.take();
        let release = std::mem::take(&mut banks.release_pending);
        if !accepted {
            return;
        }
        if release {
            banks.committed = None;
        }
        if candidate.is_some() {
            banks.committed = candidate;
        }
    }

    /// Asks to drop the history, for settings that no longer use TAA. The bank stays until the
    /// build of those settings is accepted, because the live stage sets still read it.
    pub fn release(&self) {
        self.0.borrow_mut().release_pending = true;
    }

    #[cfg(test)]
    pub fn size(&self) -> Option<[u32; 2]> {
        let banks = self.0.borrow();
        banks.committed.as_ref().map(|history| history.key.render())
    }
}

/// The scene color the HDR path reads, and the frame it belongs to. `extent` is the extent of
/// `color`: the render extent, or the output extent after temporal reconstruction.
pub(crate) struct Scene<'a> {
    pub targets: &'a crate::FrameTargets,
    pub color: &'a wgpu::TextureView,
    pub extent: [u32; 2],
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

/// A slot's camera buffer of the temporal stage, zeroed: no camera term until the first upload.
pub(crate) fn camera_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("slot temporal camera"),
        size: camera_size(),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn camera_size() -> wgpu::BufferAddress {
    wgpu::BufferAddress::try_from(TEMPORAL_CAMERA_BYTES)
        .expect("the block size fits a buffer address")
}

/// Copies `camera` into a slot's camera buffer through an immutable staging buffer.
pub(crate) fn upload_camera(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Buffer,
    camera: TemporalCamera,
) {
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("immutable temporal camera"),
        contents: &camera.to_bytes(),
        usage: wgpu::BufferUsages::COPY_SRC,
    });
    encoder.copy_buffer_to_buffer(&staging, 0, destination, 0, camera_size());
}

/// Schedules the TAA stages at the end of `set` and returns the resolved color: "taa resolve",
/// "taa history" and "taa depth history" on the render grid, or, for `temporal`, "temporal
/// upscale" and "temporal upscale history" on the output grid. `set` owns the output; the shared
/// history is also kept alive by it.
pub(crate) fn append(
    device: &wgpu::Device,
    set: &mut StageSet,
    targets: &crate::FrameTargets,
    (settings, history, uniform): (TaaSettings, &SharedHistory, &wgpu::Buffer),
    temporal: Option<&wgpu::Buffer>,
) -> wgpu::TextureView {
    let config = targets.config();
    let output = temporal.map(|_| config.output_extent());
    let bank = history.acquire(device, HistoryKey::new(config.size, output, config.samples));
    if let (Some(output), Some(camera)) = (output, temporal) {
        append_temporal(
            device,
            set,
            targets,
            (&bank.texture, output),
            (uniform, camera),
        )
    } else {
        append_native(device, set, targets, (settings, &bank, uniform))
    }
}

fn append_temporal(
    device: &wgpu::Device,
    set: &mut StageSet,
    targets: &crate::FrameTargets,
    (history, output_extent): (&wgpu::Texture, [u32; 2]),
    (uniform, camera): (&wgpu::Buffer, &wgpu::Buffer),
) -> wgpu::TextureView {
    let output = taa_upscale_gpu::output(device, output_extent);
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let reconstructed = taa_upscale_gpu::append(
        device,
        set,
        &taa_upscale_gpu::Inputs {
            color: targets.resolved(),
            motion: targets
                .velocity()
                .expect("temporal reconstruction needs the velocity attachment"),
            depth: targets.depth(),
            render: targets.config().size,
            uniform,
            camera,
            history,
            output: &output,
        },
    );
    set.resources.textures.push(output);
    set.resources.textures.push(history.clone());
    set.resources.textures.push(reconstructed.depth);
    set.resources.textures.push(reconstructed.motion);
    set.resources.textures.push(reconstructed.confidence);
    set.resources.buffers.push(reconstructed.scatter);
    output_view
}

fn append_native(
    device: &wgpu::Device,
    set: &mut StageSet,
    targets: &crate::FrameTargets,
    (settings, bank, uniform): (TaaSettings, &TaaHistory, &wgpu::Buffer),
) -> wgpu::TextureView {
    let config = targets.config();
    let depth_history = bank
        .depth
        .as_ref()
        .expect("a native bank has a depth history");
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
            history: &bank.texture,
            output: &output,
            depth_history,
            depth_output: &depth_output,
        },
        settings,
    );
    set.resources.textures.push(output);
    set.resources.textures.push(bank.texture.clone());
    set.resources.textures.push(depth_output);
    set.resources.textures.push(depth_history.clone());
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

/// Copies `block` into the head of `destination`, a slot's uniform buffer. A temporal block is
/// always supplied: its extents divide, so an all-zero block is not a "no history" block.
///
/// # Errors
///
/// [`crate::PostError::UnsupportedProfile`] for a block that fails [`TemporalBlock::validate`];
/// nothing is recorded then.
pub(crate) fn upload_temporal(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Buffer,
    block: &TemporalBlock,
) -> Result<(), crate::PostError> {
    let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("immutable temporal block"),
        contents: &block.to_bytes()?,
        usage: wgpu::BufferUsages::COPY_SRC,
    });
    let size = wgpu::BufferAddress::try_from(crate::TEMPORAL_BLOCK_BYTES)
        .expect("the block size fits a buffer address");
    encoder.copy_buffer_to_buffer(&staging, 0, destination, 0, size);
    Ok(())
}
