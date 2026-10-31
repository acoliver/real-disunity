use super::{AllocationSet, Configuration, ConfigurationError, total};
use std::sync::Arc;

/// Admission owned by external work or a resident resource, independent of target generations.
#[derive(Debug)]
pub struct AllocationReservation(Arc<AllocationSet>);

impl<T> Configuration<T> {
    /// Admits storage without replacing the current value or changing its generation.
    pub fn reserve<E>(
        &mut self,
        limits: &wgpu::Limits,
        allocations: AllocationSet,
    ) -> Result<AllocationReservation, ConfigurationError<E>> {
        self.collect();
        let retained: Vec<_> = self
            .reservations
            .iter()
            .filter_map(std::sync::Weak::upgrade)
            .collect();
        let required = total(
            std::iter::once(&allocations)
                .chain(std::iter::once(&self.allocations))
                .chain(self.retired.iter().map(|old| &old.allocations))
                .chain(retained.iter().map(AsRef::as_ref)),
            limits,
        )
        .map_err(ConfigurationError::Invalid)?;
        self.budget = self.policy.resolve(required)?;
        let reservation = AllocationReservation(Arc::new(allocations));
        self.reservations.push(Arc::downgrade(&reservation.0));
        Ok(reservation)
    }
}
