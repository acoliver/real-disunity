//! Driver allocation failures must be resolved before publishing a configuration.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DriverAllocationError {
    #[error("GPU out of memory: {0}")]
    OutOfMemory(String),
    #[error("GPU validation failed: {0}")]
    Validation(String),
    #[error("GPU internal failure: {0}")]
    Internal(String),
}

pub struct AllocationScope {
    oom: wgpu::ErrorScopeGuard,
    validation: wgpu::ErrorScopeGuard,
    internal: wgpu::ErrorScopeGuard,
}
impl AllocationScope {
    pub fn new(device: &wgpu::Device) -> Self {
        Self {
            oom: device.push_error_scope(wgpu::ErrorFilter::OutOfMemory),
            validation: device.push_error_scope(wgpu::ErrorFilter::Validation),
            internal: device.push_error_scope(wgpu::ErrorFilter::Internal),
        }
    }
    pub fn finish(self) -> Result<(), DriverAllocationError> {
        let internal = pollster::block_on(self.internal.pop());
        let validation = pollster::block_on(self.validation.pop());
        let oom = pollster::block_on(self.oom.pop());
        resolve([oom, internal, validation])
    }
}
fn resolve(errors: [Option<wgpu::Error>; 3]) -> Result<(), DriverAllocationError> {
    match errors.into_iter().flatten().next() {
        Some(wgpu::Error::OutOfMemory { source }) => {
            Err(DriverAllocationError::OutOfMemory(source.to_string()))
        }
        Some(wgpu::Error::Validation { description, .. }) => {
            Err(DriverAllocationError::Validation(description))
        }
        Some(wgpu::Error::Internal { description, .. }) => {
            Err(DriverAllocationError::Internal(description))
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::{
        AllocationBudget, AllocationSet, Configuration, ConfigurationError,
    };
    use crate::submission::{SubmissionAuthority, SubmitError};

    #[test]
    fn scoped_failures_and_loss_never_publish() {
        let instance = crate::device::headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let authority = SubmissionAuthority::new(queue);
        let mut config = Configuration::new(
            &authority,
            7,
            AllocationSet::default(),
            &device.limits(),
            AllocationBudget(0),
        )
        .unwrap();
        for (index, error) in injected_errors().into_iter().enumerate() {
            let failed = config.replace(&device.limits(), AllocationSet::default(), || {
                resolve([Some(error), None, None])?;
                Ok::<_, DriverAllocationError>(9)
            });
            assert!(matches!(
                (&failed, index),
                (
                    Err(ConfigurationError::Allocation(
                        DriverAllocationError::OutOfMemory(_)
                    )),
                    0
                ) | (
                    Err(ConfigurationError::Allocation(
                        DriverAllocationError::Internal(_)
                    )),
                    1
                ) | (
                    Err(ConfigurationError::Allocation(
                        DriverAllocationError::Validation(_)
                    )),
                    2
                )
            ));
            assert_eq!((*config.current(), config.generation()), (7, 0));
            assert!(authority.is_idle());
        }
        let failed = config.replace(&device.limits(), AllocationSet::default(), || {
            authority.report_device_loss("injected loss".into());
            resolve([injected_errors().pop(), None, None])?;
            Ok::<_, DriverAllocationError>(9)
        });
        assert!(matches!(
            failed,
            Err(ConfigurationError::Submission(SubmitError::DeviceLost(_)))
        ));
        assert_eq!((*config.current(), config.generation()), (7, 0));
    }
    fn injected_errors() -> Vec<wgpu::Error> {
        vec![
            wgpu::Error::OutOfMemory {
                source: std::io::Error::other("injected OOM").into(),
            },
            wgpu::Error::Internal {
                source: std::io::Error::other("driver").into(),
                description: "injected driver failure".into(),
            },
            wgpu::Error::Validation {
                source: std::io::Error::other("validation").into(),
                description: "injected validation failure".into(),
            },
        ]
    }
    #[test]
    fn actual_validation_is_captured_by_production_scope() {
        let instance = crate::device::headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let authority = SubmissionAuthority::new(queue);
        let mut config = Configuration::new(
            &authority,
            7,
            AllocationSet::default(),
            &device.limits(),
            AllocationBudget(0),
        )
        .unwrap();
        let result = config.replace(&device.limits(), AllocationSet::default(), || {
            let scope = AllocationScope::new(&device);
            let _invalid = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("invalid allocation"),
                size: 4,
                usage: wgpu::BufferUsages::empty(),
                mapped_at_creation: false,
            });
            scope.finish()?;
            Ok::<_, DriverAllocationError>(9)
        });
        assert!(matches!(
            result,
            Err(ConfigurationError::Allocation(
                DriverAllocationError::Validation(_)
            ))
        ));
        assert_eq!((*config.current(), config.generation()), (7, 0));
        assert!(authority.is_idle());
        assert!(AllocationScope::new(&device).finish().is_ok());
    }
}
