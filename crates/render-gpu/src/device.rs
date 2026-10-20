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
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle().with_env();
    if std::env::var_os("WGPU_BACKEND").is_none() {
        descriptor.backends = wgpu::Backends::PRIMARY;
    }
    wgpu::Instance::new(descriptor)
}
