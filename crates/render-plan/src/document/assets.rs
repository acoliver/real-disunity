//! Authored asset resolution policy.
//!
//! Bindings map one exact structural asset URI onto the logical resource
//! binding that claims it and the role that binding serves. There is
//! deliberately no path, filename, dataset, capability, digest, or byte
//! knowledge here: a binding is exactly the binding identifier, the asset
//! URI, and the role URI. The capability URI, content digest, and byte
//! length remain solely authoritative in the resource-binding layer this
//! policy resolves into.

use serde::{Deserialize, Serialize};

use crate::document::error::ContractDocumentError;
use crate::document::validate::{
    require_identifier, require_sorted_unique, require_unique, require_uri,
};

/// One authored asset binding.
///
/// The binding identifier names the exact logical resource binding that
/// claims the asset; the asset URI is the structural reference this document
/// resolves through that binding; the role URI names the role the binding
/// serves. Multiple bindings may share one role.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetBinding {
    binding_id: String,
    asset_uri: String,
    role_uri: String,
}

impl AssetBinding {
    /// Validates and adopts one asset binding.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when an identifier or URI is
    /// malformed.
    pub fn new(
        binding_id: String,
        asset_uri: String,
        role_uri: String,
    ) -> Result<Self, ContractDocumentError> {
        let binding = Self {
            binding_id,
            asset_uri,
            role_uri,
        };
        binding.validate()?;
        Ok(binding)
    }

    /// Returns the binding identifier.
    ///
    /// The identifier is the exact logical resource-binding claimant.
    #[must_use]
    pub fn binding_id(&self) -> &str {
        &self.binding_id
    }

    /// Returns the structural URI of the asset this binding serves.
    #[must_use]
    pub fn asset_uri(&self) -> &str {
        &self.asset_uri
    }

    /// Returns the structural URI of the role this binding serves.
    ///
    /// The role is authored data only; it is reconciled against the
    /// authoritative publication resource binding in a later phase.
    #[must_use]
    pub fn role_uri(&self) -> &str {
        &self.role_uri
    }

    /// Validates the identifiers and asset and role URIs.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first malformed field.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        require_identifier("asset binding_id", &self.binding_id)?;
        require_uri("asset asset_uri", &self.asset_uri)?;
        require_uri("asset binding role_uri", &self.role_uri)
    }
}

/// Authored asset resolution policy.
///
/// Bindings are held in strictly ascending, duplicate-free `binding_id` order
/// and each asset URI is declared by at most one binding, so resolution by
/// asset URI has exactly one answer and maps onto one exact binding
/// identifier. Multiple bindings may share one role URI. Every asset URI
/// referenced anywhere in the document must be declared here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetResolutionPolicy {
    bindings: Vec<AssetBinding>,
}

impl AssetResolutionPolicy {
    /// Validates and adopts the ordered bindings.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] when bindings are unsorted,
    /// duplicated, empty, carry a duplicated asset URI, or are individually
    /// malformed.
    pub fn new(bindings: Vec<AssetBinding>) -> Result<Self, ContractDocumentError> {
        let policy = Self { bindings };
        policy.validate()?;
        Ok(policy)
    }

    /// Returns the strictly ordered bindings.
    #[must_use]
    pub fn bindings(&self) -> &[AssetBinding] {
        &self.bindings
    }

    /// Returns the binding declaring exactly `asset_uri`, if any.
    #[must_use]
    pub fn binding_for(&self, asset_uri: &str) -> Option<&AssetBinding> {
        self.bindings
            .iter()
            .find(|binding| binding.asset_uri == asset_uri)
    }

    /// Validates ordering, uniqueness, nonemptiness, and every binding.
    ///
    /// # Errors
    ///
    /// Returns [`ContractDocumentError`] on the first violated invariant.
    pub fn validate(&self) -> Result<(), ContractDocumentError> {
        if self.bindings.is_empty() {
            return Err(ContractDocumentError::EmptyCollection {
                field: "asset bindings",
            });
        }
        require_sorted_unique(
            "asset bindings",
            self.bindings
                .iter()
                .map(|binding| binding.binding_id.as_str()),
        )?;
        require_unique(
            "asset bindings",
            self.bindings
                .iter()
                .map(|binding| binding.asset_uri.as_str()),
        )?;
        for binding in &self.bindings {
            binding.validate()?;
        }
        Ok(())
    }
}
