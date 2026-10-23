//! Governed discovery of one authored render contract from selected world bindings.

use thiserror::Error;
use worldrunner_world_runtime::{
    BoundResourceReadError, ContentAddress, VerifiedNormalPublication,
};

use crate::document::{ContractDocument, ContractDocumentError};

/// Exact capability selecting the governed authored render contract.
pub const RENDER_CONTRACT_CAPABILITY_URI: &str = "urn:real-disunity:capability:render-contract:v4";

/// Explicit bound for authored render-contract discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderContractDiscoveryLimits {
    pub max_contract_bytes: u64,
}

/// One validated authored render contract discovered through the verified world closure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredRenderContract {
    document: ContractDocument,
    binding_id: String,
    address: ContentAddress,
}

impl DiscoveredRenderContract {
    /// Returns the validated authored document.
    #[must_use]
    pub const fn document(&self) -> &ContractDocument {
        &self.document
    }

    /// Returns the exact selected-world binding that governed discovery.
    #[must_use]
    pub fn binding_id(&self) -> &str {
        &self.binding_id
    }

    /// Returns the complete content address of the governed contract bytes.
    #[must_use]
    pub const fn address(&self) -> ContentAddress {
        self.address
    }
}

/// Failure while discovering or decoding the governed authored render contract.
#[derive(Debug, Error)]
pub enum RenderContractDiscoveryError {
    #[error("render-contract maximum byte bound must be positive")]
    InvalidLimit,
    #[error(
        "verified world must contain exactly one {RENDER_CONTRACT_CAPABILITY_URI} binding; observed {observed}"
    )]
    MatchCount { observed: usize },
    #[error("render-contract binding {binding_id} must have no element URI")]
    ElementNotBare { binding_id: String },
    #[error("render-contract binding could not be read: {source}")]
    Read { source: BoundResourceReadError },
    #[error("render-contract document rejected: {source}")]
    Document { source: ContractDocumentError },
}

/// Discovers exactly one governed authored render contract from a verified normal publication.
///
/// Selection is exclusively by the exact capability URI in the selected world
/// binding closure. Binding IDs, paths, filenames, source identities, profile
/// names, and content inspection never participate in routing.
///
/// # Errors
///
/// Returns [`RenderContractDiscoveryError`] for invalid bounds, zero or
/// ambiguous matches, a non-bare binding, content-address drift, or an invalid
/// authored document.
pub fn discover_render_contract(
    normal: &VerifiedNormalPublication,
    limits: RenderContractDiscoveryLimits,
) -> Result<DiscoveredRenderContract, RenderContractDiscoveryError> {
    if limits.max_contract_bytes == 0 {
        return Err(RenderContractDiscoveryError::InvalidLimit);
    }
    let matches = normal
        .publication()
        .world()
        .bindings()
        .iter()
        .filter(|binding| binding.capability_uri == RENDER_CONTRACT_CAPABILITY_URI)
        .collect::<Vec<_>>();
    let [binding] = matches.as_slice() else {
        return Err(RenderContractDiscoveryError::MatchCount {
            observed: matches.len(),
        });
    };
    if binding.element_uri.is_some() {
        return Err(RenderContractDiscoveryError::ElementNotBare {
            binding_id: binding.binding_id.clone(),
        });
    }
    let bytes = normal
        .publication()
        .read_binding_bounded(binding, limits.max_contract_bytes)
        .map_err(|source| RenderContractDiscoveryError::Read { source })?;
    let document = ContractDocument::from_json_bytes(&bytes)
        .map_err(|source| RenderContractDiscoveryError::Document { source })?;
    Ok(DiscoveredRenderContract {
        document,
        binding_id: binding.binding_id.clone(),
        address: ContentAddress::from(*binding),
    })
}
