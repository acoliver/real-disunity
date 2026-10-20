//! Explicit GPU acquisition with caller-selected feature requirements.
use wgpu::{Adapter, Device, Features, Instance, PowerPreference, Queue, RequestAdapterOptions};

use thiserror::Error;

pub const REQUIRED_FEATURES: Features = Features::TEXTURE_COMPRESSION_BC;

/// A GPU device could not be acquired with the requested requirements.
#[derive(Debug, Error)]
pub enum DeviceError {
    /// No adapter at all was available.
    #[error("no GPU adapter is available on this host")]
    NoAdapter,
    /// An adapter exists but lacks features this renderer requires.
    #[error("adapter {adapter} lacks required features {missing:?}")]
    MissingFeatures {
        /// The adapter's reported name, so the message names a real device.
        adapter: String,
        /// The features that were required but absent.
        missing: Features,
    },
    /// The adapter refused to yield a device.
    #[error("the adapter refused to create a device: {0}")]
    DeviceRefused(String),
}

/// A device, its queue, and the adapter they came from.
///
/// The adapter is retained because it is the only thing that can answer what a
/// format supports, which the resource layer needs when it decides how to
/// upload.
#[derive(Debug)]
pub struct GpuContext {
    adapter: Adapter,
    device: Device,
    queue: Queue,
}

impl GpuContext {
    /// Acquires a device able to sample block-compressed textures.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError`] when no adapter exists, when the best available
    /// adapter cannot sample compressed textures, or when device creation
    /// fails.
    pub async fn request() -> Result<Self, DeviceError> {
        Self::request_with_features(REQUIRED_FEATURES).await
    }

    /// Acquires a headless device enabling exactly the requested optional features.
    ///
    /// Use [`Features::empty()`] for scenes containing only raw textures.
    /// Returns [`DeviceError`] if no adapter exists or any requirement is unmet.
    pub async fn request_with_features(required_features: Features) -> Result<Self, DeviceError> {
        let instance = worldrunner_render_gpu::device::headless_instance();
        Self::from_instance_with_features(&instance, required_features).await
    }

    /// Acquires a device from a caller-supplied instance.
    ///
    /// # Errors
    ///
    /// As [`GpuContext::request`].
    pub async fn from_instance(instance: &Instance) -> Result<Self, DeviceError> {
        Self::from_instance_with_features(instance, REQUIRED_FEATURES).await
    }

    /// Acquires a device with explicit features from a caller-configured instance.
    ///
    /// Returns [`DeviceError`] rather than dropping unsupported requirements.
    pub async fn from_instance_with_features(
        instance: &Instance,
        required_features: Features,
    ) -> Result<Self, DeviceError> {
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                ..RequestAdapterOptions::default()
            })
            .await
            .map_err(|_| DeviceError::NoAdapter)?;

        Self::from_adapter_with_features(adapter, required_features).await
    }

    /// Acquires a device from an explicitly selected adapter, enabling exactly the
    /// requested features. Desktop hosts can select a surface-compatible adapter
    /// without adding window-system dependencies to this pathless GPU crate.
    pub async fn from_adapter_with_features(
        adapter: Adapter,
        required_features: Features,
    ) -> Result<Self, DeviceError> {
        let available = adapter.features();
        let missing = required_features.difference(available);
        if !missing.is_empty() {
            return Err(DeviceError::MissingFeatures {
                adapter: adapter.get_info().name,
                missing,
            });
        }

        let (device, queue, _) = worldrunner_render_gpu::device::request_device(
            &adapter,
            &worldrunner_render_gpu::device::DeviceRequirements {
                required_features,
                ..Default::default()
            },
        )
        .await
        .map_err(|error| DeviceError::DeviceRefused(error.to_string()))?;

        Ok(Self {
            adapter,
            device,
            queue,
        })
    }

    /// Returns the device, for creating resources and pipelines.
    #[must_use]
    pub const fn device(&self) -> &Device {
        &self.device
    }

    /// Returns the queue, for submitting uploads and command buffers.
    #[must_use]
    pub const fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Returns the adapter, the only source of truth about format support.
    #[must_use]
    pub const fn adapter(&self) -> &Adapter {
        &self.adapter
    }
}

#[cfg(test)]
mod tests;
