//! The device the player's offscreen work runs on: what the player asks of an adapter, the storage
//! budget it gets there, and where a window gets a device it shares with the viewer.
//!
//! One function, [`player_device`], says what the player needs. The headless player makes its own
//! device from it, and the window hands the same requirements to the viewer, which makes the one
//! device both of them draw on.

use std::path::Path;

use worldrunner_render_gpu::configuration::AllocationBudget;
use worldrunner_render_gpu::device::{
    DeviceRequirements, request_device, storage_images_per_stage,
};
use worldrunner_render_gpu::submission::SubmissionAuthority;
use worldrunner_render_program::runtime::ProgramRuntime;

use crate::storage_budget::{self, Choice, STORAGE_BUDGET_ENV, host_available_bytes};

/// Inputs sampled once when a player starts, so its adapter requests use one storage policy.
#[derive(Clone, Debug)]
pub(crate) struct PlayerStartupInputs {
    storage_budget_mb: Option<String>,
    host_available_bytes: Option<u64>,
}

impl PlayerStartupInputs {
    /// Takes the process inputs that determine a software adapter's adaptive storage budget.
    pub(crate) fn capture() -> Self {
        Self::from_snapshot(
            std::env::var(STORAGE_BUDGET_ENV).ok(),
            host_available_bytes(),
        )
    }

    /// Holds a startup snapshot, also used to make parity checks independent of host memory.
    pub(crate) fn from_snapshot(
        storage_budget_mb: Option<String>,
        host_available_bytes: Option<u64>,
    ) -> Self {
        Self {
            storage_budget_mb,
            host_available_bytes,
        }
    }
}

/// What the player asks of an adapter and the storage budget it runs with there.
pub(crate) struct PlayerDevice {
    pub requirements: DeviceRequirements,
    pub choice: Choice,
}

/// What the player asks of `adapter`: the requirements of its device, and the storage budget the
/// runtime on it may hold, whose largest buffer those requirements allow.
pub(crate) fn player_device(adapter: &wgpu::Adapter, inputs: &PlayerStartupInputs) -> PlayerDevice {
    let choice = storage_choice(adapter, inputs);
    PlayerDevice {
        requirements: device_requirements(choice.bytes, &adapter.limits()),
        choice,
    }
}

/// Where a window gets the device its offscreen work shares with the viewer. The viewer owns the
/// adapter choice and the device, so the player asks it for both in this order: the adapter, to
/// size its requirements, then the device made for those requirements and the viewer's own.
pub(crate) trait DeviceSource {
    /// The adapter the device will be made on.
    fn adapter(&mut self) -> Result<wgpu::Adapter, String>;

    /// The one device, with the submission authority every user of it records and submits
    /// through, made to satisfy `requirements` and the viewer's own.
    fn device(
        &mut self,
        requirements: &DeviceRequirements,
    ) -> Result<(wgpu::Device, SubmissionAuthority), String>;
}

/// The storage budget for `adapter`: see [`storage_budget`].
fn storage_choice(adapter: &wgpu::Adapter, inputs: &PlayerStartupInputs) -> Choice {
    storage_budget::choose(
        inputs.storage_budget_mb.as_deref(),
        adapter.get_info().device_type,
        inputs.host_available_bytes,
    )
}

/// The device the player asks for, headless or in a window: the default limits and block
/// compression where offered, with buffers as large as the storage budget allows (the adapter's
/// own limit when that is less, since the budget does not need one buffer to hold it). It asks
/// for the storage images in a stage that the scene's device asks for, [`storage_images_per_stage`],
/// so a window's device and a headless one agree; two of the Old World's compute variants bind
/// five, one more than the default limit of four, and an adapter offering only four leaves those
/// two variants out by name.
pub(crate) fn device_requirements(budget: u64, available: &wgpu::Limits) -> DeviceRequirements {
    let default = wgpu::Limits::default();
    DeviceRequirements {
        // The player stores some textures block-compressed; a device without the feature leaves
        // them out by name. Unity filters its 32-bit float render textures; without
        // FLOAT32_FILTERABLE they are read at the nearest texel instead.
        optional_features: wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::FLOAT32_FILTERABLE,
        required_limits: wgpu::Limits {
            max_buffer_size: budget
                .max(default.max_buffer_size)
                .min(available.max_buffer_size),
            max_storage_textures_per_shader_stage: storage_images_per_stage(available),
            ..default
        },
        ..DeviceRequirements::default()
    }
}

/// The instance of the headless player's device.
#[cfg(not(test))]
pub(crate) fn headless_gpu_instance() -> wgpu::Instance {
    worldrunner_render_gpu::device::headless_instance()
}

/// The play tests' instance is render-gpu's test instance: parallel test devices on the
/// production instance crashed inside the Vulkan loader (hub issue 12ae16c68a94).
#[cfg(test)]
pub(crate) fn headless_gpu_instance() -> wgpu::Instance {
    worldrunner_render_gpu::test_support::test_instance()
}

/// Builds every compute kernel variant of the player in `data` as a compute program on the
/// device the headless player asks for, and counts what the device cannot hold by cause.
///
/// # Errors
/// Fails with the reason when the machine has no adapter or device, or the player cannot be read.
pub fn compute_census(data: &Path) -> Result<(String, crate::offscreen::ComputeCensus), String> {
    let inputs = PlayerStartupInputs::capture();
    let instance = headless_gpu_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|error| format!("no adapter: {error}"))?;
    let wanted = player_device(&adapter, &inputs);
    let budget = wanted.choice.bytes;
    let (device, queue, _) = pollster::block_on(request_device(&adapter, &wanted.requirements))
        .map_err(|error| format!("no device: {error}"))?;
    let name = format!(
        "{}, {} storage images in a stage",
        adapter.get_info().name,
        device.limits().max_storage_textures_per_shader_stage
    );
    let runtime = ProgramRuntime::new(
        device,
        SubmissionAuthority::new(queue),
        AllocationBudget(budget),
    );
    crate::offscreen::compute_census(data, &runtime)
        .map(|census| (name, census))
        .map_err(|error| error.to_string())
}

/// A viewer with no window as the device source, for tests of the window's start.
#[cfg(test)]
pub(crate) mod headless {
    use std::collections::BTreeSet;

    use worldrunner_scene_viewer::{SharedGpu, ViewerOptions};

    use super::*;
    use crate::player::GpuWork;
    use crate::retained_driver::{PlayerWindow, viewer_options};

    pub(crate) struct HeadlessViewer {
        pub gpu: SharedGpu,
        pub options: ViewerOptions,
        /// What the player asked of the device, in order.
        pub asked: Vec<DeviceRequirements>,
    }

    impl HeadlessViewer {
        pub(crate) fn new(size: [u32; 2]) -> Self {
            let window = PlayerWindow {
                title: "windowless".into(),
                size,
                fixed_dt: None,
                max_frames: None,
                capture_frames: BTreeSet::new(),
                keys: Vec::new(),
                gpu_work: GpuWork::Auto,
            };
            Self {
                gpu: SharedGpu::headless_for_test(),
                options: viewer_options(&window),
                asked: Vec::new(),
            }
        }

        /// The authority of the device, which the window's frames and the player both submit
        /// through.
        pub(crate) fn authority(&mut self) -> SubmissionAuthority {
            self.gpu
                .device(&self.options, &DeviceRequirements::default())
                .expect("the viewer's device")
                .1
        }
    }

    impl DeviceSource for HeadlessViewer {
        fn adapter(&mut self) -> Result<wgpu::Adapter, String> {
            self.gpu.adapter().map_err(|error| error.to_string())
        }

        fn device(
            &mut self,
            requirements: &DeviceRequirements,
        ) -> Result<(wgpu::Device, SubmissionAuthority), String> {
            self.asked.push(requirements.clone());
            self.gpu
                .device(&self.options, requirements)
                .map_err(|error| error.to_string())
        }
    }
}
