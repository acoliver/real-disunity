//! Process-lifetime GPU handles for tests, with exact negotiated device profiles.
use crate::device::{DeviceRequirements, NegotiatedDevice, negotiate, request_device};
use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, Mutex, OnceLock, PoisonError},
};
use thiserror::Error;
use wgpu::{Adapter, Device, Features, Instance, Limits, Queue};

#[derive(Debug, Error)]
pub enum TestGpuError {
    #[error("no test GPU adapter is available: {0}")]
    NoAdapter(String),
    #[error(transparent)]
    Device(#[from] crate::device::DeviceError),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DeviceProfile {
    features: Features,
    limits: Limits,
}

impl From<&NegotiatedDevice> for DeviceProfile {
    fn from(negotiated: &NegotiatedDevice) -> Self {
        Self {
            features: negotiated.enabled_features,
            limits: negotiated.limits.clone(),
        }
    }
}

struct ProfileCache<T> {
    entries: HashMap<DeviceProfile, T>,
}

impl<T> Default for ProfileCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
}

impl<T: Clone> ProfileCache<T> {
    fn get_or_try_insert_with<E>(
        &mut self,
        profile: DeviceProfile,
        create: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        match self.entries.entry(profile) {
            Entry::Occupied(entry) => Ok(entry.get().clone()),
            Entry::Vacant(entry) => Ok(entry.insert(create()?).clone()),
        }
    }
}

struct SharedGpu {
    instance: Instance,
    adapter: Adapter,
    profiles: Mutex<ProfileCache<(Device, Queue)>>,
}

static SHARED: OnceLock<Result<SharedGpu, String>> = OnceLock::new();
static TEARDOWN_FAILURE: OnceLock<String> = OnceLock::new();

/// Permanently rejects shared GPU reuse after the first failed session teardown.
pub fn record_shared_gpu_failure(reason: String) {
    let _ = TEARDOWN_FAILURE.set(reason);
}

/// Panics with the original teardown failure before accessing cached GPU state.
pub fn assert_shared_gpu_usable() {
    if let Some(reason) = TEARDOWN_FAILURE.get() {
        panic!("shared GPU state unusable: {reason}");
    }
}

/// The headless instance for GPU tests: [`crate::device::headless_instance`] without
/// `InstanceFlags::DEBUG`, so the Vulkan backend does not load `VK_EXT_debug_utils`.
///
/// Parallel test devices crashed inside the Vulkan loader while naming objects through
/// `VK_EXT_debug_utils` (hub issue 12ae16c68a94). Discarding labels is not enough: wgpu-hal
/// names every command buffer it begins, with an empty name when there is no label, whenever
/// the extension is loaded. wgpu validation messages keep their labels. `WGPU_DEBUG=1` loads
/// the extension again for a debugging run.
pub fn test_instance() -> Instance {
    Instance::new(crate::device::headless_descriptor(
        wgpu::InstanceFlags::default().difference(wgpu::InstanceFlags::DEBUG),
    ))
}

fn shared() -> Result<&'static SharedGpu, TestGpuError> {
    assert_shared_gpu_usable();
    SHARED
        .get_or_init(|| {
            let instance = test_instance();
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                }))
                .map_err(|error| error.to_string())?;
            Ok(SharedGpu {
                instance,
                adapter,
                profiles: Mutex::new(ProfileCache::default()),
            })
        })
        .as_ref()
        .map_err(|error| TestGpuError::NoAdapter(error.clone()))
}

/// Cloned handles only. Every caller owns its resources, scopes and submission state.
#[derive(Clone, Debug)]
pub struct TestGpu {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
    pub negotiated: NegotiatedDevice,
}

impl TestGpu {
    /// Returns the process's high-performance headless adapter, respecting WGPU_*.
    /// Adapter policy is fixed on first use; fallback/selection tests stay dedicated.
    pub fn shared_adapter() -> Result<Adapter, TestGpuError> {
        Ok(shared()?.adapter.clone())
    }

    /// Negotiates before lookup, then serializes exact-profile cache misses.
    /// Never destroys cached devices or replaces their callbacks outside a session.
    /// Panics with the original failure if any shared session teardown failed.
    pub fn shared(requirements: &DeviceRequirements) -> Result<Self, TestGpuError> {
        let shared = shared()?;
        let negotiated = negotiate(
            shared.adapter.features(),
            &shared.adapter.limits(),
            requirements,
        )?;
        // Creation finishes before insertion; unwinding leaves existing cached handles intact.
        let (device, queue) = shared
            .profiles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_try_insert_with(DeviceProfile::from(&negotiated), || {
                let (device, queue, _) =
                    pollster::block_on(request_device(&shared.adapter, requirements))?;
                install_fail_fast_handlers(&device);
                Ok::<_, crate::device::DeviceError>((device, queue))
            })?;
        Ok(Self {
            instance: shared.instance.clone(),
            adapter: shared.adapter.clone(),
            device,
            queue,
            negotiated,
        })
    }
}

/// Cached devices cannot recover per test: asynchronous errors fail the process.
pub fn install_fail_fast_handlers(device: &Device) {
    device.set_device_lost_callback(|reason, message| {
        eprintln!("shared test GPU lost: {reason:?}: {message}");
        std::process::abort();
    });
    device.on_uncaptured_error(Arc::new(|error: wgpu::Error| {
        eprintln!("shared test GPU uncaptured error: {error}");
        std::process::abort();
    }));
}

/// A second thread polling `device` that is held inside a mapping callback. wgpu-core fires a
/// poll's mapping callbacks before the submission callbacks it collected in the same poll, so
/// work submitted before [`HeldPoller::start`] is complete on the GPU while its completion
/// bookkeeping has not run. The device must not be polled elsewhere before `start` returns.
pub struct HeldPoller {
    release: std::sync::mpsc::Sender<()>,
    poller: std::thread::JoinHandle<()>,
    _held: wgpu::Buffer,
}

impl HeldPoller {
    /// Starts the poller and returns once it is held.
    ///
    /// # Panics
    /// When the mapping or the poll fails.
    #[must_use]
    pub fn start(device: &Device) -> Self {
        use std::sync::mpsc;
        let held = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("held mapping"),
            size: 4,
            usage: wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let (entered, entered_signal) = mpsc::channel();
        let (release, release_signal) = mpsc::channel::<()>();
        held.slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                result.unwrap();
                entered.send(()).unwrap();
                release_signal.recv().unwrap();
            });
        let poller = {
            let device = device.clone();
            std::thread::spawn(move || {
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            })
        };
        entered_signal.recv().unwrap();
        Self {
            release,
            poller,
            _held: held,
        }
    }

    /// Lets the held poll run its remaining callbacks after `delay`, from another thread, and
    /// returns that thread. Join it, then call [`Self::join`].
    #[must_use]
    pub fn release_after(&self, delay: std::time::Duration) -> std::thread::JoinHandle<()> {
        let release = self.release.clone();
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            release.send(()).unwrap();
        })
    }

    /// Waits for the held poll to finish.
    ///
    /// # Panics
    /// When the poller thread panicked.
    pub fn join(self) {
        self.poller.join().unwrap();
    }
}

/// The largest count in `0..=cap` for which `submits` succeeds, with its refusal of the count
/// after it, or `None` when `cap` itself succeeds.
///
/// Counts the command buffers a recorded unit costs on Metal: wgpu-hal's Metal queue refuses, as
/// a lost device, the command buffer after its 4096 created and not submitted
/// (`MAX_COMMAND_BUFFERS`), so the largest number of units one frame of a fresh device submits
/// falls as each unit costs more. Other backends have no such cap. `submits` must succeed for
/// every count below one it succeeds for.
pub fn largest_submitting_count<E>(
    cap: usize,
    mut submits: impl FnMut(usize) -> Result<(), E>,
) -> (usize, Option<E>) {
    let Err(mut refusal) = submits(cap) else {
        return (cap, None);
    };
    let (mut submitted, mut refused) = (0, cap);
    while refused - submitted > 1 {
        let middle = usize::midpoint(submitted, refused);
        match submits(middle) {
            Ok(()) => submitted = middle,
            Err(error) => (refused, refusal) = (middle, error),
        }
    }
    (submitted, Some(refusal))
}

// Loss, foreign-queue and retirement tests must own their device's polling lifecycle.
#[cfg(test)]
pub(crate) fn fresh_device() -> (Device, Queue) {
    let adapter = TestGpu::shared_adapter().unwrap();
    let (device, queue, _) =
        pollster::block_on(request_device(&adapter, &DeviceRequirements::default())).unwrap();
    (device, queue)
}

#[cfg(test)]
mod gpu_tests;
#[cfg(test)]
mod tests;
