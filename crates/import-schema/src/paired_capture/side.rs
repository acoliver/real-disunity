//! One side of a paired capture: role, provenance, state, and output.
//!
//! Identity is recorded by URI and digest only; no installation path,
//! machine name, or account data belongs in a capture side. The payload
//! itself stays private; only its content identity travels in the manifest.

use serde::{Deserialize, Serialize};

use super::geometry::{
    PoseRecord, ProjectionRecord, ViewportRecord, validate_pose, validate_projection,
    validate_viewport,
};
use super::observation::{
    CaptureState, Observed, validate_observed_text, validate_reason, validate_state,
};
use super::{PairedCaptureError, invalid_field, validate_hash};
use crate::validation::{is_identifier, is_uri};

/// Which side of the pair one capture represents.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationRole {
    /// The capture observed at the original source build.
    Source,
    /// The capture replayed through this engine.
    Replay,
}

/// The build identity of one side of the pair.
///
/// Identity is recorded by URI and digest only; no installation path, machine name, or
/// account data belongs here. A digest that could not be pinned is an explicit unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildIdentity {
    /// The source-neutral identity of the observed product.
    pub product_uri: String,
    /// The product version string, when known.
    pub version: Observed<String>,
    /// The digest of the pinned build artifacts, when known.
    pub build_digest: Observed<String>,
}

/// The retained output frame of one capture, content-addressed.
///
/// The payload itself stays private; only its identity travels in the manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputArtifact {
    /// The format identity, e.g. a PNG URI.
    pub format_uri: String,
    /// The SHA-256 of the output bytes.
    pub sha256: String,
    /// The output length in bytes.
    pub bytes: u64,
}

/// One side of a paired capture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureSide {
    /// The role this capture plays in the pair.
    pub role: ObservationRole,
    /// The stable identity of this observation.
    pub observation_id: String,
    /// The digest of the private save-derived input this capture observed.
    pub input_sha256: String,
    /// The build identity.
    pub build: BuildIdentity,
    /// The requested and actual pose.
    pub pose: PoseRecord,
    /// The viewport.
    pub viewport: ViewportRecord,
    /// The projection, when observable.
    pub projection: Observed<ProjectionRecord>,
    /// The named state, covering every core key.
    pub state: CaptureState,
    /// The retained output frame.
    pub output: OutputArtifact,
}

impl CaptureSide {
    /// Validates this side for the expected role.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed observation violation.
    pub fn validate_for(&self, expected: ObservationRole) -> Result<(), PairedCaptureError> {
        if self.role != expected {
            return Err(invalid_field(
                "role",
                "the expected side role",
                "another role",
            ));
        }
        if !is_identifier(&self.observation_id) {
            return Err(invalid_field(
                "observation_id",
                "nonempty identifier",
                &self.observation_id,
            ));
        }
        validate_hash("input_sha256", &self.input_sha256)?;
        validate_build(&self.build)?;
        validate_pose(&self.pose)?;
        validate_viewport(self.viewport)?;
        validate_projection(&self.projection)?;
        validate_state(&self.state)?;
        validate_output(&self.output)
    }
}

/// Validates one build identity.
fn validate_build(build: &BuildIdentity) -> Result<(), PairedCaptureError> {
    if !is_uri(&build.product_uri) {
        return Err(invalid_field(
            "build.product_uri",
            "product URI",
            &build.product_uri,
        ));
    }
    validate_observed_text("build.version", &build.version)?;
    match &build.build_digest {
        Observed::Known { value } => validate_hash("build.build_digest", value),
        Observed::Unknown { reason } => validate_reason("build.build_digest", reason),
    }
}

/// Validates one output artifact.
fn validate_output(output: &OutputArtifact) -> Result<(), PairedCaptureError> {
    if !is_uri(&output.format_uri) {
        return Err(invalid_field(
            "output.format_uri",
            "format URI",
            &output.format_uri,
        ));
    }
    validate_hash("output.sha256", &output.sha256)?;
    if output.bytes == 0 {
        return Err(invalid_field(
            "output.bytes",
            "a nonempty retained frame",
            "0",
        ));
    }
    Ok(())
}
