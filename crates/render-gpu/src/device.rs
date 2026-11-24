//! Explicit requirements and observable optional capability negotiation.
use thiserror::Error;
use wgpu::{Device, Features, Limits, Queue};

#[derive(Clone, Debug, Default)]
pub struct DeviceRequirements {
    pub required_features: Features,
    pub optional_features: Features,
    pub required_limits: Limits,
    /// An all-or-nothing preferred profile; rejection is reported, never hidden.
    pub optional_limits: Option<Limits>,
}

impl DeviceRequirements {
    /// What one device must offer to serve both `self`'s user and `other`'s: every feature
    /// either asks for, and each limit at the more demanding of the two values. A side without a
    /// preferred profile contributes its required limits to the other's profile, so the combined
    /// profile still meets both sides' requirements.
    #[must_use]
    pub fn union(self, other: &Self) -> Self {
        let optional_limits = match (&self.optional_limits, &other.optional_limits) {
            (None, None) => None,
            (mine, theirs) => Some(
                mine.as_ref()
                    .unwrap_or(&self.required_limits)
                    .clone()
                    .or_better_values_from(theirs.as_ref().unwrap_or(&other.required_limits)),
            ),
        };
        Self {
            required_features: self.required_features | other.required_features,
            optional_features: self.optional_features | other.optional_features,
            required_limits: self
                .required_limits
                .or_better_values_from(&other.required_limits),
            optional_limits,
        }
    }
}

/// The most storage images a stage of an engine compute program binds: a single-pass downsampler
/// binds its source and six levels, and a full twelve-level chain thirteen.
const ENGINE_STORAGE_IMAGES_PER_STAGE: u32 = 16;

/// The storage images per shader stage every engine device asks of an adapter offering
/// `available`: the adapter's own count up to sixteen, and never fewer than wgpu's default of four,
/// so [`negotiate`] refuses an adapter offering fewer by name. The scene's device and the player's
/// headless device both ask this, so the devices they make on one adapter agree. A program binding
/// more than the device's count is refused by name when it is prepared.
#[must_use]
pub fn storage_images_per_stage(available: &Limits) -> u32 {
    available
        .max_storage_textures_per_shader_stage
        .min(ENGINE_STORAGE_IMAGES_PER_STAGE)
        .max(Limits::default().max_storage_textures_per_shader_stage)
}

#[derive(Clone, Debug)]
pub struct NegotiatedDevice {
    pub enabled_features: Features,
    pub unavailable_optional: Features,
    pub limits: Limits,
    pub unavailable_optional_limits: Vec<String>,
}

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("required GPU features are unavailable: {missing:?}")]
    MissingFeatures { missing: Features },
    #[error("required GPU limits exceed adapter capabilities: {failures:?}")]
    LimitsExceeded { failures: Vec<String> },
    #[error("GPU device request failed: {0}")]
    Request(#[from] wgpu::RequestDeviceError),
}

pub fn negotiate(
    available_features: Features,
    available_limits: &Limits,
    requirements: &DeviceRequirements,
) -> Result<NegotiatedDevice, DeviceError> {
    let missing = requirements
        .required_features
        .difference(available_features);
    if !missing.is_empty() {
        return Err(DeviceError::MissingFeatures { missing });
    }
    let mut failures = Vec::new();
    requirements.required_limits.check_limits_with_fail_fn(
        available_limits,
        false,
        |name, requested, available| {
            failures.push(format!(
                "{name}: requested {requested}, available {available}"
            ));
        },
    );
    if !failures.is_empty() {
        return Err(DeviceError::LimitsExceeded { failures });
    }
    let mut limits = requirements.required_limits.clone();
    let mut unavailable_optional_limits = Vec::new();
    if let Some(optional) = &requirements.optional_limits {
        requirements
            .required_limits
            .check_limits_with_fail_fn(optional, false, |name, _, _| {
                unavailable_optional_limits.push(format!(
                    "optional profile does not preserve required {name}"
                ));
            });
        optional.check_limits_with_fail_fn(
            available_limits,
            false,
            |name, requested, available| {
                unavailable_optional_limits.push(format!(
                    "{name}: preferred {requested}, available {available}"
                ));
            },
        );
        if unavailable_optional_limits.is_empty() {
            limits = optional.clone();
        }
    }
    Ok(NegotiatedDevice {
        enabled_features: requirements.required_features
            | (requirements.optional_features & available_features),
        unavailable_optional: requirements
            .optional_features
            .difference(available_features),
        limits,
        unavailable_optional_limits,
    })
}

pub async fn request_device(
    adapter: &wgpu::Adapter,
    requirements: &DeviceRequirements,
) -> Result<(Device, Queue, NegotiatedDevice), DeviceError> {
    let negotiated = negotiate(adapter.features(), &adapter.limits(), requirements)?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("render-gpu"),
            required_features: negotiated.enabled_features,
            required_limits: negotiated.limits.clone(),
            ..Default::default()
        })
        .await?;
    Ok((device, queue, negotiated))
}

/// Uses the native primary APIs by default, respecting WGPU_* diagnostic overrides.
pub fn headless_instance() -> wgpu::Instance {
    wgpu::Instance::new(headless_descriptor(wgpu::InstanceFlags::default()))
}

/// The headless descriptor starting from `flags`; WGPU_* variables still override each flag.
pub(crate) fn headless_descriptor(flags: wgpu::InstanceFlags) -> wgpu::InstanceDescriptor {
    let mut descriptor = wgpu::InstanceDescriptor {
        flags,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    }
    .with_env();
    if std::env::var_os("WGPU_BACKEND").is_none() {
        descriptor.backends = wgpu::Backends::PRIMARY;
    }
    descriptor
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
