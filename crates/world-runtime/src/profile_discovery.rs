//! Governed discovery of one authored runtime profile from selected world bindings.

use thiserror::Error;
use worldrunner_world_model::ResourceBinding;

use crate::{
    AuthoredRuntimeProfile, BoundResourceReadError, ContentAddress, RUNTIME_PROFILE_CAPABILITY_URI,
    RuntimeProfileError, RuntimeProfileLimits, VerifiedNormalPublication,
};

/// Explicit bounds for governed runtime-profile discovery and decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeProfileDiscoveryLimits {
    /// Maximum encoded profile bytes that may be retained.
    pub max_profile_bytes: u64,
    /// Structural limits applied while validating the authored profile.
    pub profile: RuntimeProfileLimits,
}

/// One validated runtime profile discovered through verified normal authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredRuntimeProfile {
    profile: AuthoredRuntimeProfile,
    binding_id: String,
    address: ContentAddress,
}

impl DiscoveredRuntimeProfile {
    /// Returns the validated authored runtime profile.
    #[must_use]
    pub const fn profile(&self) -> &AuthoredRuntimeProfile {
        &self.profile
    }

    /// Returns the exact selected-world binding that governed discovery.
    #[must_use]
    pub fn binding_id(&self) -> &str {
        &self.binding_id
    }

    /// Returns the complete content address of the governed profile bytes.
    #[must_use]
    pub const fn address(&self) -> ContentAddress {
        self.address
    }
}

/// Failure while discovering or decoding the governed authored runtime profile.
#[derive(Debug, Error)]
pub enum RuntimeProfileDiscoveryError {
    /// The encoded-byte ceiling was zero.
    #[error("runtime-profile maximum byte bound must be positive")]
    InvalidLimit,
    /// The selected binding closure did not contain exactly one profile.
    #[error(
        "verified world must contain exactly one {RUNTIME_PROFILE_CAPABILITY_URI} binding; observed {observed}"
    )]
    MatchCount { observed: usize },
    /// The governed document binding declared an element type.
    #[error("runtime-profile binding {binding_id} must have no element URI")]
    ElementNotBare { binding_id: String },
    /// The selected binding could not be reread under its content address.
    #[error("runtime-profile binding could not be read: {source}")]
    Read { source: BoundResourceReadError },
    /// The selected bytes were not one valid exact runtime profile.
    #[error("runtime-profile document rejected: {source}")]
    Profile { source: RuntimeProfileError },
}

/// Discovers exactly one governed runtime profile from a verified normal publication.
///
/// Selection uses only the exact profile capability in the selected binding
/// closure. Binding IDs, paths, filenames, source identities, profile names,
/// and content inspection never participate in routing.
///
/// # Errors
///
/// Returns [`RuntimeProfileDiscoveryError`] for invalid bounds, zero or
/// ambiguous matches, a non-bare binding, content-address drift, or an invalid
/// authored profile.
pub fn discover_runtime_profile(
    normal: &VerifiedNormalPublication,
    limits: RuntimeProfileDiscoveryLimits,
) -> Result<DiscoveredRuntimeProfile, RuntimeProfileDiscoveryError> {
    discover_from_bindings(
        normal.publication().world().bindings(),
        limits,
        |binding, maximum_bytes| {
            normal
                .publication()
                .read_binding_bounded(binding, maximum_bytes)
        },
    )
}

fn discover_from_bindings(
    bindings: &[ResourceBinding],
    limits: RuntimeProfileDiscoveryLimits,
    read: impl FnOnce(&ResourceBinding, u64) -> Result<Vec<u8>, BoundResourceReadError>,
) -> Result<DiscoveredRuntimeProfile, RuntimeProfileDiscoveryError> {
    if limits.max_profile_bytes == 0 {
        return Err(RuntimeProfileDiscoveryError::InvalidLimit);
    }
    let matches = bindings
        .iter()
        .filter(|binding| binding.capability_uri == RUNTIME_PROFILE_CAPABILITY_URI)
        .collect::<Vec<_>>();
    let [binding] = matches.as_slice() else {
        return Err(RuntimeProfileDiscoveryError::MatchCount {
            observed: matches.len(),
        });
    };
    if binding.element_uri.is_some() {
        return Err(RuntimeProfileDiscoveryError::ElementNotBare {
            binding_id: binding.binding_id.clone(),
        });
    }
    let bytes = read(binding, limits.max_profile_bytes)
        .map_err(|source| RuntimeProfileDiscoveryError::Read { source })?;
    let profile = AuthoredRuntimeProfile::from_json(&bytes, limits.profile)
        .map_err(|source| RuntimeProfileDiscoveryError::Profile { source })?;
    Ok(DiscoveredRuntimeProfile {
        profile,
        binding_id: binding.binding_id.clone(),
        address: ContentAddress::from(*binding),
    })
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};
    use worldrunner_world_model::ContentDigest;

    use super::*;

    fn limits(max_profile_bytes: u64) -> RuntimeProfileDiscoveryLimits {
        RuntimeProfileDiscoveryLimits {
            max_profile_bytes,
            profile: RuntimeProfileLimits::new(2, 2, 2).unwrap(),
        }
    }

    fn profile_bytes() -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "schema": "real-disunity.runtime-profile.v1",
            "schema_version": 1,
            "profile_id": "discovered-profile",
            "world_schema": { "tag": "urn:synthetic:world:v1", "version": 1 },
            "root_handler": {
                "handler_id": "synthetic-root",
                "revision": 1,
                "payload_contract_uri": "urn:synthetic:payload:root:v1"
            },
            "component_handlers": [],
            "capability_handlers": [{
                "capability_uri": RUNTIME_PROFILE_CAPABILITY_URI,
                "element_uri": null,
                "handler_id": "synthetic-profile",
                "revision": 1,
                "payload_contract_uri": "urn:synthetic:payload:profile:v1"
            }]
        }))
        .unwrap()
    }

    fn digest(bytes: &[u8]) -> ContentDigest {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        ContentDigest::from_bytes(digest).unwrap()
    }

    fn binding(id: &str, bytes: &[u8], element_uri: Option<&str>) -> ResourceBinding {
        ResourceBinding {
            binding_id: id.to_owned(),
            capability_uri: RUNTIME_PROFILE_CAPABILITY_URI.to_owned(),
            content: digest(bytes),
            byte_length: u64::try_from(bytes.len()).unwrap(),
            element_uri: element_uri.map(str::to_owned),
        }
    }

    #[test]
    fn discovery_requires_positive_limits_and_exactly_one_match() {
        assert!(matches!(
            discover_from_bindings(&[], limits(0), |_, _| unreachable!()).unwrap_err(),
            RuntimeProfileDiscoveryError::InvalidLimit
        ));
        assert!(matches!(
            discover_from_bindings(&[], limits(1), |_, _| unreachable!()).unwrap_err(),
            RuntimeProfileDiscoveryError::MatchCount { observed: 0 }
        ));

        let bytes = profile_bytes();
        let bindings = [binding("a", &bytes, None), binding("b", &bytes, None)];
        assert!(matches!(
            discover_from_bindings(&bindings, limits(u64::MAX), |_, _| unreachable!()).unwrap_err(),
            RuntimeProfileDiscoveryError::MatchCount { observed: 2 }
        ));
    }

    #[test]
    fn discovery_rejects_non_bare_oversized_and_tampered_profile_bindings() {
        let bytes = profile_bytes();
        let non_bare = [binding("profile", &bytes, Some("urn:synthetic:element:v1"))];
        assert!(matches!(
            discover_from_bindings(&non_bare, limits(u64::MAX), |_, _| unreachable!()).unwrap_err(),
            RuntimeProfileDiscoveryError::ElementNotBare { .. }
        ));

        let exact = [binding("profile", &bytes, None)];
        assert!(matches!(
            discover_from_bindings(&exact, limits(1), |binding, maximum| {
                Err(BoundResourceReadError::LimitExceeded {
                    observed: binding.byte_length,
                    maximum,
                })
            })
            .unwrap_err(),
            RuntimeProfileDiscoveryError::Read {
                source: BoundResourceReadError::LimitExceeded { .. }
            }
        ));
        assert!(matches!(
            discover_from_bindings(&exact, limits(u64::MAX), |binding, _| {
                Err(BoundResourceReadError::DigestMismatch {
                    expected: binding.content,
                    observed: digest(b"tampered"),
                })
            })
            .unwrap_err(),
            RuntimeProfileDiscoveryError::Read {
                source: BoundResourceReadError::DigestMismatch { .. }
            }
        ));
    }

    #[test]
    fn valid_discovery_retains_owned_profile_and_content_address_provenance() {
        let bytes = profile_bytes();
        let expected_address;
        let discovered = {
            let bindings = [binding("governed-profile", &bytes, None)];
            expected_address = ContentAddress::from(&bindings[0]);
            discover_from_bindings(&bindings, limits(u64::MAX), |_, _| Ok(bytes.clone())).unwrap()
        };
        drop(bytes);

        assert_eq!(discovered.profile().profile_id(), "discovered-profile");
        assert_eq!(discovered.binding_id(), "governed-profile");
        assert_eq!(discovered.address(), expected_address);
    }
}
