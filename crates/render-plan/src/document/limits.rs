//! Explicit mandatory operational limits for one authored render contract.
//!
//! There is deliberately no `Default`: embedding population assumptions
//! would smuggle dataset facts into a universal document, so every bound is
//! authored explicitly per document. These bounds are operational inputs for
//! later runtime planning; the stream bounds are not summed against authored
//! asset bindings here, because bindings carry no byte lengths.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::validate::{require_positive_u64, require_range};

/// Explicit streaming budget for later runtime planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamLimits {
    /// Maximum byte length one streamed binding may occupy during runtime
    /// planning.
    pub max_stream_bytes: u64,
    /// Maximum total bytes all streamed bindings may occupy during runtime
    /// planning.
    pub max_total_bytes: u64,
}

impl StreamLimits {
    /// Returns the per-binding maximum stream byte length.
    #[must_use]
    pub const fn max_stream_bytes(&self) -> u64 {
        self.max_stream_bytes
    }

    /// Returns the aggregate maximum stream byte length.
    #[must_use]
    pub const fn max_total_bytes(&self) -> u64 {
        self.max_total_bytes
    }

    /// Validates strict positivity and aggregate dominance.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when either bound is zero or the
    /// aggregate bound is smaller than the per-binding bound.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_positive_u64("limits max_stream_bytes", self.max_stream_bytes)?;
        require_positive_u64("limits max_total_bytes", self.max_total_bytes)?;
        require_range(
            "limits max_stream_bytes",
            1,
            i128::from(self.max_total_bytes),
            i128::from(self.max_stream_bytes),
        )
    }
}

/// Explicit mandatory limits governing one authored render contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractLimits {
    /// Maximum scene mesh component count.
    pub max_mesh_components: u32,
    /// Maximum level count of one LOD policy.
    pub max_lod_levels_per_policy: u32,
    /// Explicit streaming budget for later runtime planning.
    pub stream: StreamLimits,
}

impl ContractLimits {
    /// Returns the maximum scene mesh component count.
    #[must_use]
    pub const fn max_mesh_components(&self) -> u32 {
        self.max_mesh_components
    }

    /// Returns the maximum level count of one LOD policy.
    #[must_use]
    pub const fn max_lod_levels_per_policy(&self) -> u32 {
        self.max_lod_levels_per_policy
    }

    /// Returns the explicit streaming budget.
    #[must_use]
    pub const fn stream(&self) -> &StreamLimits {
        &self.stream
    }

    /// Validates every bound and the streaming budget.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated bound.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_range(
            "limits max_mesh_components",
            1,
            i128::from(u32::MAX),
            i128::from(self.max_mesh_components),
        )?;
        require_range(
            "limits max_lod_levels_per_policy",
            1,
            i128::from(u32::MAX),
            i128::from(self.max_lod_levels_per_policy),
        )?;
        self.stream.validate()
    }
}
