//! Caller-supplied exact routing contract shared by every capability handler.
//!
//! A route is wholly caller authority: the exact capability URI that selects
//! the handler, the exact payload-contract URI its contributions must carry,
//! the deterministic handler descriptor, and the exact element policy the
//! bound content must satisfy. Nothing here is derived from content, paths,
//! or dataset knowledge, and nothing is defaulted.

use thiserror::Error;
use worldrunner_world_model::{
    ResourceBinding, WorldModelError, validate_semantic_identifier, validate_structural_uri,
};
use worldrunner_world_runtime::{CapabilitySessionError, HandlerDescriptor};

/// Failures raised while configuring one exact capability handler route.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityConfigError {
    /// A configured field violated a universal identifier or URI rule.
    #[error("invalid {field} {key:?}: {source}")]
    Invalid {
        /// The configured field that was rejected.
        field: &'static str,
        /// The rejected value.
        key: String,
        /// Universal identifier validation failure.
        source: WorldModelError,
    },
    /// The checked byte-length calculation for a grid overflowed.
    #[error("grid byte length calculation overflowed")]
    GridLengthOverflow,
}

/// The exact element a handler accepts on one binding.
///
/// This is caller authority only: handlers never inspect element content to
/// guess which element a stream carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementPolicy {
    /// The binding must carry no element URI.
    Bare,
    /// The binding must carry exactly this element URI.
    Element(String),
}

impl ElementPolicy {
    /// Policy for bindings that carry no element URI.
    #[must_use]
    pub const fn bare() -> Self {
        Self::Bare
    }

    /// Policy for bindings carrying exactly `uri`.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityConfigError::Invalid`] when `uri` is not a
    /// structural URI, so a route no binding could ever satisfy cannot be
    /// configured.
    pub fn element(uri: &str) -> Result<Self, CapabilityConfigError> {
        require_structural_uri("element URI", uri)?;
        Ok(Self::Element(uri.to_owned()))
    }
}

/// Exact route and payload contract one capability handler serves.
///
/// Every field is caller authority checked once at construction; sessions
/// re-verify the binding against these exact spellings at start so a handler
/// wired to a mismatched registry key still fails closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactCapabilityRoute {
    capability_uri: String,
    payload_contract_uri: String,
    descriptor: HandlerDescriptor,
    element_policy: ElementPolicy,
}

impl ExactCapabilityRoute {
    /// Adopts one exact caller-authored route.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityConfigError::Invalid`] when the capability URI,
    /// payload-contract URI, handler identity, or element URI violates the
    /// universal identifier rules.
    pub fn new(
        capability_uri: &str,
        payload_contract_uri: &str,
        handler_id: &str,
        revision: u64,
        element_policy: ElementPolicy,
    ) -> Result<Self, CapabilityConfigError> {
        require_structural_uri("capability URI", capability_uri)?;
        require_structural_uri("payload contract URI", payload_contract_uri)?;
        validate_semantic_identifier(handler_id).map_err(|source| {
            CapabilityConfigError::Invalid {
                field: "handler identity",
                key: handler_id.to_owned(),
                source,
            }
        })?;
        if let ElementPolicy::Element(element_uri) = &element_policy {
            require_structural_uri("element URI", element_uri)?;
        }
        Ok(Self {
            capability_uri: capability_uri.to_owned(),
            payload_contract_uri: payload_contract_uri.to_owned(),
            descriptor: HandlerDescriptor::new(handler_id, revision),
            element_policy,
        })
    }

    /// Returns the exact capability URI that selects this handler.
    #[must_use]
    pub fn capability_uri(&self) -> &str {
        &self.capability_uri
    }

    /// Returns the exact payload-contract URI contributions must carry.
    #[must_use]
    pub fn payload_contract_uri(&self) -> &str {
        &self.payload_contract_uri
    }

    /// Returns the handler descriptor contributions must report.
    #[must_use]
    pub fn descriptor(&self) -> &HandlerDescriptor {
        &self.descriptor
    }

    /// Returns the exact element policy bindings must satisfy.
    #[must_use]
    pub fn element_policy(&self) -> &ElementPolicy {
        &self.element_policy
    }

    /// Verifies one complete binding against this exact route.
    ///
    /// The checks are ordered and deterministic: exact capability URI, exact
    /// element policy, then the declared byte length against the session's
    /// independent byte bound.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilitySessionError::Rejected`] when any field of the
    /// binding mismatches the route or exceeds the session byte bound.
    pub(crate) fn verify_binding(
        &self,
        binding: &ResourceBinding,
        max_stream_bytes: usize,
    ) -> Result<(), CapabilitySessionError> {
        if binding.capability_uri != self.capability_uri {
            return Err(CapabilitySessionError::Rejected {
                reason: format!(
                    "binding capability {:?} does not match the exact route {:?}",
                    binding.capability_uri, self.capability_uri
                ),
            });
        }
        verify_element_policy(&self.element_policy, binding)?;
        let declared = usize::try_from(binding.byte_length).unwrap_or(usize::MAX);
        if declared > max_stream_bytes {
            return Err(CapabilitySessionError::Rejected {
                reason: format!(
                    "binding declares {declared} content bytes but the session permits \
                     at most {max_stream_bytes}"
                ),
            });
        }
        Ok(())
    }
}

fn require_structural_uri(field: &'static str, key: &str) -> Result<(), CapabilityConfigError> {
    validate_structural_uri(key).map_err(|source| CapabilityConfigError::Invalid {
        field,
        key: key.to_owned(),
        source,
    })
}

fn verify_element_policy(
    policy: &ElementPolicy,
    binding: &ResourceBinding,
) -> Result<(), CapabilitySessionError> {
    let matches = match policy {
        ElementPolicy::Bare => binding.element_uri.is_none(),
        ElementPolicy::Element(expected) => binding.element_uri.as_deref() == Some(expected),
    };
    if matches {
        return Ok(());
    }
    let expected = match policy {
        ElementPolicy::Bare => "none",
        ElementPolicy::Element(uri) => uri,
    };
    let observed = binding.element_uri.as_deref().unwrap_or("none");
    Err(CapabilitySessionError::Rejected {
        reason: format!(
            "binding element {observed:?} does not match the exact element policy {expected:?}"
        ),
    })
}
