use crate::{CaptureRequestError, RetainedCaptureRecipe, ViewerError};
use worldrunner_render_gpu::configuration::{AllocationBudgetPolicy, ConfigurationError};

/// Retained recipe storage capacity, fixed for the host's lifetime.
#[derive(Clone, Debug, PartialEq)]
pub enum CapturePoolPolicy {
    /// Positive byte capacity. The default is 256 MiB.
    Fixed(u64),
    /// Largest declared recipe at `max_extent`, plus rounded and capped headroom.
    /// Declarations describe individual requests, not concurrent reservations.
    Derived {
        recipes: Vec<RetainedCaptureRecipe>,
        /// Nonzero maximum physical surface extent used for sizing and queued reservations.
        max_extent: [u32; 2],
        headroom_bytes: u64,
        granularity_bytes: u64,
        cap_bytes: Option<u64>,
    },
}

impl Default for CapturePoolPolicy {
    fn default() -> Self {
        Self::Fixed(256 * 1024 * 1024)
    }
}

impl CapturePoolPolicy {
    pub(crate) fn validate(&self) -> Result<(), ViewerError> {
        match self {
            Self::Fixed(0) => Err(ViewerError::InvalidOptions("capture pool must be positive")),
            Self::Derived {
                recipes,
                max_extent,
                granularity_bytes,
                cap_bytes,
                ..
            } if recipes.is_empty()
                || max_extent.contains(&0)
                || *granularity_bytes == 0
                || *cap_bytes == Some(0) =>
            {
                Err(ViewerError::InvalidOptions(
                    "derived capture pool needs recipes, nonzero maximum extent, positive granularity and a positive cap",
                ))
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn reservation_bytes(
        &self,
        recipe: &RetainedCaptureRecipe,
        size: [u32; 2],
        limit: u64,
        reserved: u64,
    ) -> Result<u64, CaptureRequestError> {
        let current = recipe.pool_bytes(size, limit, reserved)?;
        match self {
            Self::Fixed(_) => Ok(current),
            Self::Derived { max_extent, .. } => RetainedCaptureRecipe::pool_reservation_bytes(
                current.max(recipe.storage_requirement(*max_extent)?),
                limit,
                reserved,
            ),
        }
    }

    pub(crate) fn effective_bytes(
        &self,
        samples: u32,
        ao_format: wgpu::TextureFormat,
    ) -> Result<u64, CaptureRequestError> {
        let (recipes, size, policy) = match self {
            Self::Fixed(bytes) => return Ok(*bytes),
            Self::Derived {
                recipes,
                max_extent,
                headroom_bytes,
                granularity_bytes,
                cap_bytes,
            } => (
                recipes,
                *max_extent,
                AllocationBudgetPolicy::Derived {
                    headroom_bytes: *headroom_bytes,
                    granularity_bytes: *granularity_bytes,
                    cap_bytes: *cap_bytes,
                },
            ),
        };
        let mut required = 0;
        for recipe in recipes {
            let mut recipe = recipe.clone();
            recipe.frame_samples = samples;
            recipe.ao_format = ao_format;
            required = required.max(recipe.storage_bytes(size)?);
        }
        match policy.resolve::<std::convert::Infallible>(required) {
            Ok(budget) => Ok(budget.0),
            Err(ConfigurationError::Budget { budget, .. }) => Ok(budget),
            Err(_) => Err(CaptureRequestError::InvalidRecipe),
        }
    }
}
