use super::{CheckedView, ContractError};
use worldrunner_render_gpu::submission::{Publications, SubmitError};

impl CheckedView {
    pub(super) fn retain_lease(&self, encoder: &wgpu::CommandEncoder) -> Result<(), ContractError> {
        if let Some(lease) = &self.lease {
            lease.check().map_err(ContractError::Lease)?;
            Publications::retain_lease(encoder, lease).map_err(|error| match error {
                SubmitError::Lease(error) => ContractError::Lease(error),
                SubmitError::Transaction(error) => ContractError::Transaction(error),
                _ => unreachable!("lease retention does not submit work"),
            })?;
        }
        Ok(())
    }
}
