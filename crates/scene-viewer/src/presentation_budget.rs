use crate::ViewerError;

/// Presentation allocation inventory limit in bytes, including CPU capture storage
/// and concurrently resident renderer generations. Zero fails option validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentationBudget(pub u64);

impl Default for PresentationBudget {
    fn default() -> Self {
        Self(1024 * 1024 * 1024)
    }
}

impl PresentationBudget {
    pub(crate) fn validate(self) -> Result<(), ViewerError> {
        if self.0 == 0 {
            return Err(ViewerError::InvalidOptions(
                "presentation allocation budget must be positive",
            ));
        }
        Ok(())
    }
}
