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

fn shared() -> Result<&'static SharedGpu, TestGpuError> {
    assert_shared_gpu_usable();
    SHARED
        .get_or_init(|| {
            let instance = crate::device::headless_instance();
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
