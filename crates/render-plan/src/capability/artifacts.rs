//! Canonical STRMSH, STRMAT, and STRTEX artifact capability handlers.
//!
//! The three handlers share one shape: stream wholly bounded into memory,
//! then validate the canonical artifact format exactly once through its
//! universal decoder. Format validation happens only after complete streaming
/// and never selects a handler or route.
use std::sync::Arc;

use worldrunner_material::MaterialArtifact;
use worldrunner_mesh::StaticMeshArtifact;
use worldrunner_texture::TextureArtifact;
use worldrunner_world_runtime::{CapabilityHandler, CapabilitySessionError};

use crate::capability::route::ExactCapabilityRoute;
use crate::capability::session::{ExactCapabilityHandler, ExactDecoder, artifact_rejection};

/// Owned immutable canonical STRMSH static mesh artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct StaticMeshPayload {
    artifact: StaticMeshArtifact,
}

impl StaticMeshPayload {
    /// Returns the wholly owned decoded static mesh artifact.
    #[must_use]
    pub fn artifact(&self) -> &StaticMeshArtifact {
        &self.artifact
    }
}

/// Owned immutable canonical STRMAT material artifact.
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialPayload {
    artifact: MaterialArtifact,
}

impl MaterialPayload {
    /// Returns the wholly owned decoded material artifact.
    #[must_use]
    pub fn artifact(&self) -> &MaterialArtifact {
        &self.artifact
    }
}

/// Owned immutable canonical STRTEX texture artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TexturePayload {
    artifact: TextureArtifact,
}

impl TexturePayload {
    /// Returns the wholly owned decoded texture artifact.
    #[must_use]
    pub fn artifact(&self) -> &TextureArtifact {
        &self.artifact
    }
}

struct StaticMeshDecoder {
    max_stream_bytes: usize,
}

impl ExactDecoder for StaticMeshDecoder {
    type Payload = StaticMeshPayload;

    fn max_stream_bytes(&self) -> usize {
        self.max_stream_bytes
    }

    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError> {
        let artifact = StaticMeshArtifact::decode(&bytes)
            .map_err(|source| artifact_rejection("static mesh", &source))?;
        Ok(StaticMeshPayload { artifact })
    }
}

struct MaterialDecoder {
    max_stream_bytes: usize,
}

impl ExactDecoder for MaterialDecoder {
    type Payload = MaterialPayload;

    fn max_stream_bytes(&self) -> usize {
        self.max_stream_bytes
    }

    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError> {
        let artifact = MaterialArtifact::decode(&bytes)
            .map_err(|source| artifact_rejection("material", &source))?;
        Ok(MaterialPayload { artifact })
    }
}

struct TextureDecoder {
    max_stream_bytes: usize,
}

impl ExactDecoder for TextureDecoder {
    type Payload = TexturePayload;

    fn max_stream_bytes(&self) -> usize {
        self.max_stream_bytes
    }

    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError> {
        let artifact = TextureArtifact::decode(&bytes)
            .map_err(|source| artifact_rejection("texture", &source))?;
        Ok(TexturePayload { artifact })
    }
}

/// Builds the canonical STRMSH static mesh handler for one caller-authored route.
///
/// The caller is the sole authority for the exact capability URI, the exact
/// payload-contract URI, the handler descriptor, the element policy in
/// `route`, and the independent maximum byte count one session may
/// accumulate. The canonical STRMSH format is validated only after streaming
/// completes; format bytes never select the handler.
#[must_use]
pub fn static_mesh_handler(
    route: ExactCapabilityRoute,
    max_stream_bytes: usize,
) -> Arc<dyn CapabilityHandler> {
    Arc::new(ExactCapabilityHandler::new(
        route,
        StaticMeshDecoder { max_stream_bytes },
    ))
}

/// Builds the canonical STRMAT material handler for one caller-authored route.
///
/// The authority boundary matches [`static_mesh_handler`]: the caller supplies
/// every exact route spelling and the independent session byte bound, and the
/// canonical STRMAT format is validated only after complete streaming.
#[must_use]
pub fn material_handler(
    route: ExactCapabilityRoute,
    max_stream_bytes: usize,
) -> Arc<dyn CapabilityHandler> {
    Arc::new(ExactCapabilityHandler::new(
        route,
        MaterialDecoder { max_stream_bytes },
    ))
}

/// Builds the canonical STRTEX texture handler for one caller-authored route.
///
/// The authority boundary matches [`static_mesh_handler`]: the caller supplies
/// every exact route spelling and the independent session byte bound, and the
/// canonical STRTEX format is validated only after complete streaming.
#[must_use]
pub fn texture_handler(
    route: ExactCapabilityRoute,
    max_stream_bytes: usize,
) -> Arc<dyn CapabilityHandler> {
    Arc::new(ExactCapabilityHandler::new(
        route,
        TextureDecoder { max_stream_bytes },
    ))
}
