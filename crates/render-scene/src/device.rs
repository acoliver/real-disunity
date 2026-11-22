//! Explicit GPU acquisition with caller-selected feature requirements.
use wgpu::{Adapter, Device, Features, Instance, PowerPreference, Queue, RequestAdapterOptions};

use thiserror::Error;

mod adapter;
pub use adapter::{AdapterClass, AdapterIdentity};
#[cfg(any(test, feature = "test-support"))]
mod test_support;

/// The scene device request against an adapter offering `available` limits: the one place that
/// says what every scene device (production and test) asks for.
fn requirements(
    available: &wgpu::Limits,
    required_features: Features,
    timing: worldrunner_render_post::TimingMode,
) -> worldrunner_render_gpu::device::DeviceRequirements {
    worldrunner_render_gpu::device::DeviceRequirements {
        required_features,
        // Full SSR depth chains share the trace stage with receiver metadata.
        required_limits: wgpu::Limits {
            max_texture_array_layers: available.max_texture_array_layers,
            max_color_attachment_bytes_per_sample: available
                .max_color_attachment_bytes_per_sample
                .min(64),
            max_sampled_textures_per_shader_stage: available
                .max_sampled_textures_per_shader_stage
                .min(32),
            // The adapter's count up to 16, which the player's headless device asks too. An
            // adapter offering fewer than 4 gets no device: the negotiation refuses the limit and
            // the request reports `DeviceError::DeviceRefused`.
            max_storage_textures_per_shader_stage:
                worldrunner_render_gpu::device::storage_images_per_stage(available),
            ..Default::default()
        },
        // Lets frame validation use adapter-reported sample counts (2x and 8x). wgpu filters
        // 32-bit float textures only on a device with FLOAT32_FILTERABLE, whatever the adapter's
        // format flags say, so a game pass sampling an R32Float copy linearly needs it on.
        optional_features: Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES
            | Features::DEPTH32FLOAT_STENCIL8
            | Features::FLOAT32_FILTERABLE
            | if timing == worldrunner_render_post::TimingMode::Optional {
                timing.features()
            } else {
                Features::empty()
            },
        ..Default::default()
    }
}

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
    submission: worldrunner_render_gpu::submission::SubmissionAuthority,
    unavailable_optional_limits: Vec<String>,
}

/// What a scene device is asked to provide.
#[derive(Clone, Debug, Default)]
pub struct DeviceRequest {
    /// Features the adapter must offer. Use [`Features::empty()`] for scenes containing only raw
    /// textures; block-compressed textures need [`REQUIRED_FEATURES`].
    pub features: Features,
    /// Optional timing enables supported timestamp queries; required timing rejects an adapter
    /// without them.
    pub timing: worldrunner_render_post::TimingMode,
    /// For a device another user shares, such as a player's offscreen work: the device also meets
    /// these requirements, each limit at the more demanding of the scene's value and this one,
    /// and every feature either asks for.
    pub shared: worldrunner_render_gpu::device::DeviceRequirements,
}

impl GpuContext {
    /// Acquires a headless device meeting `request`.
    ///
    /// # Errors
    ///
    /// As [`GpuContext::from_instance`].
    pub async fn request(request: &DeviceRequest) -> Result<Self, DeviceError> {
        let instance = worldrunner_render_gpu::device::headless_instance();
        Self::from_instance(&instance, request).await
    }

    /// Acquires a device meeting `request` from the high-performance adapter of a
    /// caller-configured instance.
    ///
    /// # Errors
    ///
    /// Returns [`DeviceError::NoAdapter`] when the instance offers no adapter, otherwise as
    /// [`GpuContext::from_adapter`].
    pub async fn from_instance(
        instance: &Instance,
        request: &DeviceRequest,
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

        Self::from_adapter(adapter, request).await
    }

    /// Acquires a device meeting `request` from an explicitly selected adapter. Desktop hosts can
    /// select a surface-compatible adapter without adding window-system dependencies to this
    /// pathless GPU crate.
    ///
    /// # Errors
    /// Returns [`DeviceError`] rather than dropping unsupported requirements: when the adapter
    /// lacks a required feature or limit, or refuses to create the device.
    pub async fn from_adapter(
        adapter: Adapter,
        request: &DeviceRequest,
    ) -> Result<Self, DeviceError> {
        let required_features = request.features
            | if request.timing == worldrunner_render_post::TimingMode::Required {
                request.timing.features()
            } else {
                Features::empty()
            };
        let requirements = requirements(&adapter.limits(), required_features, request.timing)
            .union(&request.shared);
        let missing = requirements
            .required_features
            .difference(adapter.features());
        if !missing.is_empty() {
            return Err(DeviceError::MissingFeatures {
                adapter: adapter.get_info().name,
                missing,
            });
        }

        let (device, queue, negotiated) =
            worldrunner_render_gpu::device::request_device(&adapter, &requirements)
                .await
                .map_err(|error| DeviceError::DeviceRefused(error.to_string()))?;

        let submission =
            worldrunner_render_gpu::submission::SubmissionAuthority::new(queue.clone());
        let report_loss = submission.device_loss_reporter();
        device.set_device_lost_callback(move |reason, message| {
            report_loss(format!("{reason:?}: {message}"));
        });
        Ok(Self {
            adapter,
            device,
            submission,
            unavailable_optional_limits: negotiated.unavailable_optional_limits,
        })
    }

    /// Why a preferred limit profile was not granted, one line per limit the adapter could not
    /// meet; empty when it was granted or none was asked for. The device then has the required
    /// limits, and work that needs the preferred ones is left out by name.
    #[must_use]
    pub fn unavailable_optional_limits(&self) -> &[String] {
        &self.unavailable_optional_limits
    }

    /// Returns the device, for creating resources and pipelines.
    #[must_use]
    pub const fn device(&self) -> &Device {
        &self.device
    }

    /// Returns the queue, for submitting uploads and command buffers.
    #[must_use]
    pub fn queue(&self) -> &Queue {
        self.submission.queue()
    }

    pub const fn submission(&self) -> &worldrunner_render_gpu::submission::SubmissionAuthority {
        &self.submission
    }

    /// Identity of the adapter used by this device.
    pub fn adapter_identity(&self) -> AdapterIdentity {
        self.adapter.get_info().into()
    }

    /// Returns the adapter, the only source of truth about format support.
    #[must_use]
    pub const fn adapter(&self) -> &Adapter {
        &self.adapter
    }
}

#[cfg(test)]
mod storage_images_tests;
#[cfg(test)]
mod tests;
