//! Shared bounded streaming machinery for exact capability sessions.
//!
//! Every session owns an independent byte bound that is checked before any
//! chunk is accumulated, regardless of the runtime planner's own limits, and
//! decodes its owned buffer through one fixed exact decoder chosen only by the
//! configured route.

use std::sync::Arc;

use worldrunner_world_model::ResourceBinding;
use worldrunner_world_runtime::{
    CapabilityHandler, CapabilitySession, CapabilitySessionError, HandlerDescriptor,
    ResourceContribution, RuntimePayload,
};

use crate::capability::route::ExactCapabilityRoute;

/// Independent per-session accumulation buffer with a fixed byte ceiling.
///
/// The bound is enforced before extending: a chunk that would push the
/// cumulative byte count past the ceiling is rejected whole.
pub(crate) struct BoundedBuffer {
    bytes: Vec<u8>,
    max_bytes: usize,
}

impl BoundedBuffer {
    /// Creates an empty buffer that will never hold more than `max_bytes`.
    pub(crate) const fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
        }
    }

    /// Accumulates one chunk after checking the cumulative bound.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilitySessionError::Rejected`] when the chunk would
    /// exceed the configured maximum or the cumulative count overflows; the
    /// buffer is left untouched in both cases.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<(), CapabilitySessionError> {
        let Some(cumulative) = self.bytes.len().checked_add(chunk.len()) else {
            return Err(CapabilitySessionError::Rejected {
                reason: "cumulative byte count overflowed".to_owned(),
            });
        };
        if cumulative > self.max_bytes {
            return Err(CapabilitySessionError::Rejected {
                reason: format!(
                    "chunk of {} bytes would raise the session to {cumulative} bytes, \
                     exceeding the configured maximum of {}",
                    chunk.len(),
                    self.max_bytes
                ),
            });
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    /// Returns the wholly owned accumulated bytes.
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Fixed decode step one handler's sessions apply to fully streamed content.
///
/// The decoder is chosen by the caller-configured factory, never by inspecting
/// content: streaming performs no format detection of any kind. `max_stream_bytes`
/// is the session's independent byte bound, which for grids is also the exact
/// required length.
pub(crate) trait ExactDecoder: Send + Sync {
    /// Owned immutable typed payload this decoder mints.
    type Payload: RuntimePayload;

    /// Returns the independent maximum byte count one session may accumulate.
    fn max_stream_bytes(&self) -> usize;

    /// Decodes the complete bounded content into the typed payload.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilitySessionError`] when the complete content violates
    /// the canonical artifact or grid contract.
    fn decode(&self, bytes: Vec<u8>) -> Result<Self::Payload, CapabilitySessionError>;
}

/// Maps a canonical artifact decode failure to a deterministic rejection.
pub(crate) fn artifact_rejection(
    kind: &str,
    source: &dyn std::fmt::Display,
) -> CapabilitySessionError {
    CapabilitySessionError::Rejected {
        reason: format!("{kind} artifact decode rejected: {source}"),
    }
}

/// Caller-configured capability handler serving one exact route.
///
/// The handler performs no content inspection: it verifies the complete
/// binding against the exact route at session start and then streams into the
/// fixed decoder the caller selected by choosing the factory.
pub(crate) struct ExactCapabilityHandler<D: ExactDecoder> {
    route: ExactCapabilityRoute,
    decoder: Arc<D>,
}

impl<D: ExactDecoder> ExactCapabilityHandler<D> {
    /// Adopts one already validated route and its fixed decoder.
    pub(crate) fn new(route: ExactCapabilityRoute, decoder: D) -> Self {
        Self {
            route,
            decoder: Arc::new(decoder),
        }
    }
}

impl<D: ExactDecoder + 'static> CapabilityHandler for ExactCapabilityHandler<D> {
    fn descriptor(&self) -> &HandlerDescriptor {
        self.route.descriptor()
    }

    fn start_session(
        &self,
        binding: &ResourceBinding,
    ) -> Result<Box<dyn CapabilitySession>, CapabilitySessionError> {
        self.route
            .verify_binding(binding, self.decoder.max_stream_bytes())?;
        Ok(Box::new(ExactSession {
            descriptor: self.route.descriptor().clone(),
            payload_contract_uri: self.route.payload_contract_uri().to_owned(),
            expected_bytes: binding.byte_length,
            buffer: BoundedBuffer::new(self.decoder.max_stream_bytes()),
            decoder: Arc::clone(&self.decoder),
        }))
    }
}

/// One live bounded session streaming toward its fixed exact decoder.
struct ExactSession<D: ExactDecoder> {
    descriptor: HandlerDescriptor,
    payload_contract_uri: String,
    expected_bytes: u64,
    buffer: BoundedBuffer,
    decoder: Arc<D>,
}

impl<D: ExactDecoder> CapabilitySession for ExactSession<D> {
    fn write_chunk(&mut self, chunk: &[u8]) -> Result<(), CapabilitySessionError> {
        self.buffer.push(chunk)
    }

    fn finish(self: Box<Self>) -> Result<ResourceContribution, CapabilitySessionError> {
        let session = *self;
        let bytes = session.buffer.into_bytes();
        let observed =
            u64::try_from(bytes.len()).map_err(|_| CapabilitySessionError::Rejected {
                reason: "streamed byte count cannot be represented as u64".to_owned(),
            })?;
        if observed != session.expected_bytes {
            return Err(CapabilitySessionError::Rejected {
                reason: format!(
                    "session received {observed} bytes but the binding declares {}",
                    session.expected_bytes
                ),
            });
        }
        let payload = session.decoder.decode(bytes)?;
        Ok(ResourceContribution::new(
            session.descriptor,
            &session.payload_contract_uri,
            payload,
        ))
    }
}
