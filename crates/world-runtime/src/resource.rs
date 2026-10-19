//! Content-addressed resource opening and single-pass stream verification.

use std::io::Read;

use sha2::{Digest as Sha256Digest, Sha256};
use thiserror::Error;
use worldrunner_world_model::{ContentDigest, ResourceBinding, WorldModelError};

use crate::error::RuntimePlanError;
use crate::handler::{CapabilitySession, HandlerDescriptor, ResourceContribution};

/// Byte size of the single fixed buffer every binding streams through.
pub const STREAM_CHUNK_BYTES: usize = 64 * 1024;

/// Typed failure raised by a resource provider when opening content.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("resource provider open failure: {message:?}")]
pub struct ProviderError {
    /// Provider-supplied description of the failure.
    pub message: String,
}

impl ProviderError {
    /// Creates a provider failure from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Complete pathless address of one resource byte stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContentAddress {
    digest: ContentDigest,
    byte_length: u64,
}

impl ContentAddress {
    /// Creates an exact address from a nonzero content digest and declared length.
    #[must_use]
    pub const fn new(digest: ContentDigest, byte_length: u64) -> Self {
        Self {
            digest,
            byte_length,
        }
    }

    /// Returns the exact content digest.
    #[must_use]
    pub const fn digest(self) -> ContentDigest {
        self.digest
    }

    /// Returns the exact byte length. Zero is valid.
    #[must_use]
    pub const fn byte_length(self) -> u64 {
        self.byte_length
    }
}

impl From<&ResourceBinding> for ContentAddress {
    fn from(binding: &ResourceBinding) -> Self {
        Self::new(binding.content, binding.byte_length)
    }
}

/// Opens bound content strictly through its complete content address.
///
/// The provider receives only digest and length. It cannot route by binding ID,
/// component identity, capability, element, filename, or path.
pub trait ResourceProvider {
    /// Opens the content at `address`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] when the addressed content cannot be opened.
    fn open<'a>(&'a self, address: ContentAddress) -> Result<Box<dyn Read + 'a>, ProviderError>;
}

/// Streams one binding exactly once through `buffer`, verifying its content
/// address and feeding the session in a single pass.
///
/// The verified digest and length are compared against the binding after the
/// stream reaches EOF and before the session's contribution is accepted.
///
/// # Errors
///
/// Returns [`RuntimePlanError`] for provider open or read failures, session
/// rejections or failures, counter overflow, and digest or length drift.
pub(crate) fn stream_binding(
    binding: &ResourceBinding,
    provider: &dyn ResourceProvider,
    mut session: Box<dyn CapabilitySession>,
    handler: &HandlerDescriptor,
    buffer: &mut [u8],
) -> Result<ResourceContribution, RuntimePlanError> {
    let mut reader = provider
        .open(ContentAddress::from(binding))
        .map_err(|source| RuntimePlanError::ResourceOpen {
            binding_id: binding.binding_id.clone(),
            capability_uri: binding.capability_uri.clone(),
            source,
        })?;
    let mut hasher = Sha256::new();
    let mut byte_length: u64 = 0;
    loop {
        let read = reader
            .read(buffer)
            .map_err(|source| RuntimePlanError::ResourceRead {
                binding_id: binding.binding_id.clone(),
                source,
            })?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        let increment = u64::try_from(read).map_err(|_| RuntimePlanError::CounterOverflow)?;
        let next_length = byte_length
            .checked_add(increment)
            .ok_or(RuntimePlanError::CounterOverflow)?;
        if next_length > binding.byte_length {
            return Err(RuntimePlanError::ResourceLengthMismatch {
                binding_id: binding.binding_id.clone(),
                expected: binding.byte_length,
                observed: next_length,
            });
        }
        Sha256Digest::update(&mut hasher, chunk);
        session
            .write_chunk(chunk)
            .map_err(|error| RuntimePlanError::from_session(binding, handler, error))?;
        byte_length = next_length;
    }
    verify_content_address(binding, byte_length, hasher)?;
    session
        .finish()
        .map_err(|error| RuntimePlanError::from_session(binding, handler, error))
}

fn verify_content_address(
    binding: &ResourceBinding,
    byte_length: u64,
    hasher: Sha256,
) -> Result<(), RuntimePlanError> {
    if byte_length != binding.byte_length {
        return Err(RuntimePlanError::ResourceLengthMismatch {
            binding_id: binding.binding_id.clone(),
            expected: binding.byte_length,
            observed: byte_length,
        });
    }
    let observed = ContentDigest::from_bytes(hasher.finalize().into())
        .map_err(|source| RuntimePlanError::WorldModel(WorldModelError::from(source)))?;
    if observed != binding.content {
        return Err(RuntimePlanError::ResourceDigestMismatch {
            binding_id: binding.binding_id.clone(),
            expected: binding.content,
            observed,
        });
    }
    Ok(())
}
