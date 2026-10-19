//! Wholly owned exact-bytes capability handler.

use std::sync::Arc;

use worldrunner_world_runtime::{CapabilityHandler, CapabilitySessionError};

use crate::capability::route::ExactCapabilityRoute;
use crate::capability::session::{ExactCapabilityHandler, ExactDecoder};

/// Owned immutable copy of exactly the bytes one binding streamed.
///
/// The payload carries no interpretation of any kind: it is the complete
/// bounded content, wholly owned, for callers whose contracts are the bytes
/// themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactBytes {
    bytes: Vec<u8>,
}

impl ExactBytes {
    /// Returns the wholly owned exact streamed bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

struct ExactBytesDecoder {
    max_stream_bytes: usize,
}

impl ExactDecoder for ExactBytesDecoder {
    type Payload = ExactBytes;

    fn max_stream_bytes(&self) -> usize {
        self.max_stream_bytes
    }

    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError> {
        Ok(ExactBytes { bytes })
    }
}

/// Builds the exact-bytes capability handler for one caller-authored route.
///
/// The caller is the sole authority for the exact capability URI, the exact
/// payload-contract URI, the handler descriptor, and the element policy in
/// `route`, and for the independent maximum byte count one session may
/// accumulate. The handler performs no content inspection and no decoding:
/// the contribution payload is exactly the streamed bytes.
#[must_use]
pub fn exact_bytes_handler(
    route: ExactCapabilityRoute,
    max_stream_bytes: usize,
) -> Arc<dyn CapabilityHandler> {
    Arc::new(ExactCapabilityHandler::new(
        route,
        ExactBytesDecoder { max_stream_bytes },
    ))
}
