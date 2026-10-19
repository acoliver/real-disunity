//! Canonical STRPAK package artifact capability handler.

use std::fmt;
use std::sync::Arc;

use worldrunner_package::{Package, PackageLimits};
use worldrunner_world_runtime::{CapabilityHandler, CapabilitySessionError};

use crate::capability::route::ExactCapabilityRoute;
use crate::capability::session::{ExactCapabilityHandler, ExactDecoder, artifact_rejection};

/// Owned immutable canonical STRPAK package artifact.
///
/// The payload wraps the parsed [`Package`], which wholly owns its bytes and
/// index, so it outlives the publication and provider that streamed it.
pub struct PackagePayload {
    package: Package,
}

impl fmt::Debug for PackagePayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PackagePayload")
            .field("manifest", self.package.manifest())
            .field("entries", &self.package.entries())
            .finish_non_exhaustive()
    }
}

impl PackagePayload {
    /// Returns the wholly owned parsed package artifact.
    #[must_use]
    pub fn package(&self) -> &Package {
        &self.package
    }
}

struct PackageDecoder {
    limits: PackageLimits,
}

impl ExactDecoder for PackageDecoder {
    type Payload = PackagePayload;

    fn max_stream_bytes(&self) -> usize {
        self.limits.max_package_bytes
    }

    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError> {
        let package = Package::parse(bytes, self.limits)
            .map_err(|source| artifact_rejection("package", &source))?;
        Ok(PackagePayload { package })
    }
}

/// Builds the canonical STRPAK package artifact handler for one caller-authored route.
///
/// The caller is the sole authority for the exact capability URI, the exact
/// payload-contract URI, the handler descriptor, the element policy in
/// `route`, and every operational bound in `limits`. The session's independent
/// accumulation bound is `limits.max_package_bytes`; the canonical STRPAK
/// format is validated only after streaming completes, and nothing about the
/// content ever selects the handler.
#[must_use]
pub fn package_handler(
    route: ExactCapabilityRoute,
    limits: PackageLimits,
) -> Arc<dyn CapabilityHandler> {
    Arc::new(ExactCapabilityHandler::new(
        route,
        PackageDecoder { limits },
    ))
}
