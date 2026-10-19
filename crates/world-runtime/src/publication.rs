//! Exact read-only verification of immutable semantic-world publications.
//!
//! Verification is fail-closed and path-explicit. It validates the descriptor,
//! receipt, semantic world, generated-state identity, and complete filesystem
//! closure before returning opaque evidence. Every distinct world binding
//! address additionally earns one exact route into a retained
//! content-address-to-member index, and [`VerifiedPublication`] reopens those
//! members as a [`ResourceProvider`]. Verification does not claim atomicity
//! against concurrent filesystem mutation; the per-open byte re-verification is
//! the consumer's single pass over each returned stream, which
//! [`crate::RuntimePlanner`] hashes and counts in full.

mod filesystem;

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use worldrunner_import_schema::{
    ImportReceipt, ImportReceiptError, PUBLICATION_SET_SCHEMA, PUBLICATION_SET_VERSION,
    PublicationSetDescriptor, PublishedArtifact, ReceiptOutputArtifact,
};
use worldrunner_world_model::{
    ContentDigest, DigestError, GeneratedStateDigest, ResourceBinding, SemanticWorldDocument,
    ValidationLimits, WorldModelError,
};

use crate::resource::{ContentAddress, ProviderError, ResourceProvider, STREAM_CHUNK_BYTES};

/// Exact logical paths needed to verify one publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationPaths<'a> {
    pub descriptor: &'a str,
    pub receipt: &'a str,
    pub world: &'a str,
}

/// Explicit resource bounds for read-only publication verification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationLimits {
    pub max_artifacts: usize,
    pub max_artifact_bytes: u64,
    pub max_total_bytes: u64,
    pub max_path_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
}

/// Opaque evidence that a complete candidate or normal publication passed exact verification.
///
/// This evidence establishes publication closure, not promotion authority. Only
/// [`crate::verify_normal_authority`] can mint a [`crate::VerifiedNormalPublication`]
/// after independently checking the candidate receipt and attestation chain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPublication {
    root: PathBuf,
    receipt: ImportReceipt,
    descriptor: PublicationSetDescriptor,
    world: SemanticWorldDocument,
    generated_state_digest: GeneratedStateDigest,
    /// One exact logical member per distinct bound content address.
    resources: HashMap<ContentAddress, String>,
}

impl VerifiedPublication {
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn receipt(&self) -> &ImportReceipt {
        &self.receipt
    }

    #[must_use]
    pub fn descriptor(&self) -> &PublicationSetDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub fn world(&self) -> &SemanticWorldDocument {
        &self.world
    }

    #[must_use]
    pub fn generated_state_digest(&self) -> GeneratedStateDigest {
        self.generated_state_digest
    }

    /// The exact logical member retained for one bound content address, if any.
    #[must_use]
    pub fn resource_member(&self, address: ContentAddress) -> Option<&str> {
        self.resources.get(&address).map(String::as_str)
    }

    /// Reads one retained world binding under an explicit byte bound.
    ///
    /// The supplied binding must equal the complete binding retained in the
    /// verified world. Content is then opened only through its content address
    /// and independently rechecked through a fixed-size buffer.
    ///
    /// # Errors
    ///
    /// Returns [`BoundResourceReadError`] for a foreign or drifted binding,
    /// bound/allocation failure, provider or read failure, or content-address
    /// mismatch.
    pub fn read_binding_bounded(
        &self,
        binding: &ResourceBinding,
        max_bytes: u64,
    ) -> Result<Vec<u8>, BoundResourceReadError> {
        let retained = self
            .world
            .bindings()
            .binary_search_by(|candidate| candidate.binding_id.cmp(&binding.binding_id))
            .ok()
            .and_then(|index| self.world.bindings().get(index))
            .ok_or_else(|| BoundResourceReadError::UnknownBinding {
                binding_id: binding.binding_id.clone(),
            })?;
        if retained != binding {
            return Err(BoundResourceReadError::BindingMismatch {
                binding_id: binding.binding_id.clone(),
            });
        }
        if binding.byte_length > max_bytes {
            return Err(BoundResourceReadError::LimitExceeded {
                observed: binding.byte_length,
                maximum: max_bytes,
            });
        }
        let capacity = usize::try_from(binding.byte_length)
            .map_err(|_| BoundResourceReadError::AllocationSizeOverflow)?;
        let reader = self
            .open(ContentAddress::from(binding))
            .map_err(|source| BoundResourceReadError::Open { source })?;
        read_verified_binding(reader, binding, capacity)
    }
}

fn read_verified_binding(
    mut reader: impl Read,
    binding: &ResourceBinding,
    capacity: usize,
) -> Result<Vec<u8>, BoundResourceReadError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| BoundResourceReadError::AllocationFailed { bytes: capacity })?;
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
    let mut hasher = Sha256::new();
    let mut observed = 0_u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| BoundResourceReadError::Read { source })?;
        if read == 0 {
            break;
        }
        let increment =
            u64::try_from(read).map_err(|_| BoundResourceReadError::ByteCountOverflow)?;
        observed = observed
            .checked_add(increment)
            .ok_or(BoundResourceReadError::ByteCountOverflow)?;
        if observed > binding.byte_length {
            return Err(BoundResourceReadError::ByteLengthMismatch {
                expected: binding.byte_length,
                observed,
            });
        }
        let chunk = &buffer[..read];
        hasher.update(chunk);
        bytes.extend_from_slice(chunk);
    }
    if observed != binding.byte_length {
        return Err(BoundResourceReadError::ByteLengthMismatch {
            expected: binding.byte_length,
            observed,
        });
    }
    let digest = ContentDigest::from_bytes(hasher.finalize().into())
        .map_err(|source| BoundResourceReadError::MalformedDigest { source })?;
    if digest != binding.content {
        return Err(BoundResourceReadError::DigestMismatch {
            expected: binding.content,
            observed: digest,
        });
    }
    Ok(bytes)
}

impl ResourceProvider for VerifiedPublication {
    /// Opens the member retained for `address` as a streamed read.
    ///
    /// Resolution goes only through the index earned during verification, then
    /// member shape is rechecked before the stream is returned. The caller's
    /// single pass over the stream is the per-open byte re-verification; bytes
    /// are never pre-buffered or read twice here.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError`] for unknown addresses and any member-shape or
    /// open failure.
    fn open<'a>(&'a self, address: ContentAddress) -> Result<Box<dyn Read + 'a>, ProviderError> {
        let member = self.resources.get(&address).ok_or_else(|| {
            ProviderError::new(format!(
                "no verified member for {}",
                address_label(&address)
            ))
        })?;
        let file = filesystem::open_member(&self.root, member)
            .map_err(|source| ProviderError::new(source.to_string()))?;
        Ok(Box::new(file))
    }
}

/// Failure while reopening one exact retained binding into bounded owned bytes.
#[derive(Debug, Error)]
pub enum BoundResourceReadError {
    #[error("world has no retained binding {binding_id}")]
    UnknownBinding { binding_id: String },
    #[error("supplied binding {binding_id} differs from the retained world binding")]
    BindingMismatch { binding_id: String },
    #[error("bound resource has {observed} bytes, exceeding maximum {maximum}")]
    LimitExceeded { observed: u64, maximum: u64 },
    #[error("bound resource byte length cannot be represented for allocation")]
    AllocationSizeOverflow,
    #[error("could not reserve {bytes} bytes for bound resource")]
    AllocationFailed { bytes: usize },
    #[error("bound resource provider failed: {source}")]
    Open { source: ProviderError },
    #[error("bound resource read failed: {source}")]
    Read { source: std::io::Error },
    #[error("bound resource byte accounting overflowed")]
    ByteCountOverflow,
    #[error("bound resource has {observed} bytes instead of {expected}")]
    ByteLengthMismatch { expected: u64, observed: u64 },
    #[error("bound resource digest could not be represented: {source}")]
    MalformedDigest { source: DigestError },
    #[error("bound resource digest {observed:?} differs from expected {expected:?}")]
    DigestMismatch {
        expected: ContentDigest,
        observed: ContentDigest,
    },
}

#[derive(Debug, Error)]
pub enum PublicationVerificationError {
    #[error("publication logical path in {field} is invalid: {value}")]
    InvalidPath { field: &'static str, value: String },
    #[error("publication descriptor rejected: {reason}")]
    InvalidDescriptor { reason: String },
    #[error("publication receipt rejected: {source}")]
    InvalidReceipt { source: ImportReceiptError },
    #[error("publication world JSON rejected: {source}")]
    InvalidWorldJson { source: serde_json::Error },
    #[error("publication world rejected: {source}")]
    InvalidWorld { source: WorldModelError },
    #[error("publication receipt is not marked as validation-passed")]
    ValidationNotPassed,
    #[error("publication receipt has no generated-state digest")]
    MissingStateDigest,
    #[error("publication receipt digest {field} is malformed: {source}")]
    MalformedDigest {
        field: &'static str,
        source: DigestError,
    },
    #[error("publication identity {field} mismatch: expected {expected}, observed {observed}")]
    IdentityMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
    #[error("publication receipt binding closure has {receipt} entries but world has {world}")]
    BindingClosureMismatch { receipt: u64, world: u64 },
    #[error("publication descriptor and receipt closure differ at {logical_path}")]
    ClosureMismatch { logical_path: String },
    #[error("publication exceeds {limit}: observed {observed}, maximum {maximum}")]
    LimitExceeded {
        limit: &'static str,
        observed: u64,
        maximum: u64,
    },
    #[error("publication byte accounting overflowed")]
    ByteCountOverflow,
    #[error("publication contains unexpected node {path}")]
    UnexpectedNode { path: String },
    #[error("publication is missing node {path}")]
    MissingNode { path: String },
    #[error("publication artifact {path} has {observed} bytes instead of {expected}")]
    ByteLengthMismatch {
        path: String,
        expected: u64,
        observed: u64,
    },
    #[error("publication artifact {path} has digest {observed} instead of {expected}")]
    DigestMismatch {
        path: String,
        expected: String,
        observed: String,
    },
    #[error("publication has no member for bound resource {address}")]
    MissingResourceMember { address: String },
    #[error(
        "publication has {} members for bound resource {address}: {members:?}",
        members.len()
    )]
    AmbiguousResourceMember {
        address: String,
        members: Vec<String>,
    },
    #[error("publication root is not a directory: {path}")]
    NotDirectory { path: PathBuf },
    #[error("filesystem operation {operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExpectedArtifact {
    sha256: String,
    bytes: u64,
}

/// Reopens and verifies an exact immutable semantic-world publication.
///
/// # Errors
///
/// Returns a typed error for any malformed document, identity drift, bound
/// violation, missing or added filesystem node, link/special node, byte drift,
/// or any world binding whose content address matches no exactly-one
/// descriptor member.
pub fn verify_publication(
    root: &Path,
    paths: PublicationPaths<'_>,
    limits: PublicationLimits,
    world_limits: &ValidationLimits,
) -> Result<VerifiedPublication, PublicationVerificationError> {
    validate_paths(paths, limits.max_path_bytes)?;
    let descriptor_bytes = filesystem::read_bounded(
        root,
        paths.descriptor,
        limits.max_artifact_bytes.min(limits.max_total_bytes),
    )?;
    let descriptor = PublicationSetDescriptor::from_json(&descriptor_bytes).map_err(|source| {
        PublicationVerificationError::InvalidDescriptor {
            reason: source.to_string(),
        }
    })?;
    validate_descriptor(
        &descriptor,
        paths.descriptor,
        descriptor_bytes.len(),
        limits,
    )?;
    let expected = expected_artifacts(&descriptor, paths.descriptor, &descriptor_bytes)?;
    let receipt_bytes = filesystem::read_bounded(
        root,
        paths.receipt,
        expected_member_bytes(&expected, paths.receipt)?,
    )?;
    let receipt = ImportReceipt::from_json(&receipt_bytes)
        .map_err(|source| PublicationVerificationError::InvalidReceipt { source })?;
    require_receipt_closure(&descriptor, paths.receipt, &receipt)?;
    let world_bytes = filesystem::read_bounded(
        root,
        paths.world,
        expected_member_bytes(&expected, paths.world)?,
    )?;
    let world: SemanticWorldDocument = serde_json::from_slice(&world_bytes)
        .map_err(|source| PublicationVerificationError::InvalidWorldJson { source })?;
    world
        .validate(world_limits)
        .map_err(|source| PublicationVerificationError::InvalidWorld { source })?;
    require_binding_closure(&receipt, &world)?;
    let generated_state_digest = require_world_identity(&receipt, &world, paths.world)?;
    let canonical_root = filesystem::verify_exact(root, &expected, limits)?;
    let resources = build_resource_index(&world, &descriptor)?;
    Ok(VerifiedPublication {
        root: canonical_root,
        receipt,
        descriptor,
        world,
        generated_state_digest,
        resources,
    })
}

/// Derives the exact address-to-member index for every distinct world binding.
///
/// A binding's complete content address must match exactly one descriptor
/// record: zero matches and collisions with arbitrary other outputs both fail
/// closed, because the descriptor is the only authoritative association.
fn build_resource_index(
    world: &SemanticWorldDocument,
    descriptor: &PublicationSetDescriptor,
) -> Result<HashMap<ContentAddress, String>, PublicationVerificationError> {
    let mut index = HashMap::with_capacity(world.bindings().len());
    for binding in world.bindings() {
        let address = ContentAddress::from(binding);
        if index.contains_key(&address) {
            continue;
        }
        let members: Vec<&str> = descriptor
            .artifacts
            .iter()
            .filter(|artifact| {
                artifact.bytes == address.byte_length()
                    && artifact.sha256 == address.digest().to_hex()
            })
            .map(|artifact| artifact.logical_path.as_str())
            .collect();
        match members.as_slice() {
            [member] => {
                index.insert(address, (*member).to_owned());
            }
            [] => {
                return Err(PublicationVerificationError::MissingResourceMember {
                    address: address_label(&address),
                });
            }
            _ => {
                return Err(PublicationVerificationError::AmbiguousResourceMember {
                    address: address_label(&address),
                    members: members.iter().map(|member| (*member).to_owned()).collect(),
                });
            }
        }
    }
    Ok(index)
}

fn require_binding_closure(
    receipt: &ImportReceipt,
    world: &SemanticWorldDocument,
) -> Result<(), PublicationVerificationError> {
    let receipt_total = receipt
        .census
        .binding_closure
        .total()
        .ok_or(PublicationVerificationError::ByteCountOverflow)?;
    let world_total = u64::try_from(world.bindings().len())
        .map_err(|_| PublicationVerificationError::ByteCountOverflow)?;
    if receipt_total != world_total {
        return Err(PublicationVerificationError::BindingClosureMismatch {
            receipt: receipt_total,
            world: world_total,
        });
    }
    Ok(())
}

/// Names one content address with its exact digest and byte length.
fn address_label(address: &ContentAddress) -> String {
    format!(
        "sha256 {} length {}",
        address.digest().to_hex(),
        address.byte_length()
    )
}

fn validate_paths(
    paths: PublicationPaths<'_>,
    max_path_bytes: usize,
) -> Result<(), PublicationVerificationError> {
    for (field, path) in [
        ("descriptor", paths.descriptor),
        ("receipt", paths.receipt),
        ("world", paths.world),
    ] {
        validate_path(field, path, max_path_bytes)?;
    }
    if paths.descriptor == paths.receipt
        || paths.descriptor == paths.world
        || paths.receipt == paths.world
    {
        return Err(PublicationVerificationError::InvalidPath {
            field: "publication paths",
            value: "paths must be distinct".to_owned(),
        });
    }
    Ok(())
}

fn validate_descriptor(
    descriptor: &PublicationSetDescriptor,
    descriptor_path: &str,
    descriptor_bytes: usize,
    limits: PublicationLimits,
) -> Result<(), PublicationVerificationError> {
    if descriptor.schema != PUBLICATION_SET_SCHEMA
        || descriptor.schema_version != PUBLICATION_SET_VERSION
        || !is_identifier(&descriptor.publication_id)
    {
        return Err(PublicationVerificationError::InvalidDescriptor {
            reason: "schema, version, or publication identifier is invalid".to_owned(),
        });
    }
    let count = descriptor
        .artifacts
        .len()
        .checked_add(1)
        .ok_or(PublicationVerificationError::ByteCountOverflow)?;
    require_count_limit(count, limits.max_artifacts)?;
    let mut total_bytes = u64::try_from(descriptor_bytes)
        .map_err(|_| PublicationVerificationError::ByteCountOverflow)?;
    let mut previous: Option<&str> = None;
    for artifact in &descriptor.artifacts {
        validate_descriptor_artifact(artifact, descriptor_path, limits, previous)?;
        total_bytes = total_bytes
            .checked_add(artifact.bytes)
            .ok_or(PublicationVerificationError::ByteCountOverflow)?;
        previous = Some(&artifact.logical_path);
    }
    if total_bytes > limits.max_total_bytes {
        return Err(PublicationVerificationError::LimitExceeded {
            limit: "total byte length",
            observed: total_bytes,
            maximum: limits.max_total_bytes,
        });
    }
    Ok(())
}

fn validate_descriptor_artifact(
    artifact: &PublishedArtifact,
    descriptor_path: &str,
    limits: PublicationLimits,
    previous: Option<&str>,
) -> Result<(), PublicationVerificationError> {
    validate_path(
        "descriptor.artifacts.logical_path",
        &artifact.logical_path,
        limits.max_path_bytes,
    )?;
    if artifact.logical_path == descriptor_path
        || previous.is_some_and(|path| path >= artifact.logical_path.as_str())
        || !is_nonzero_lower_hex(&artifact.sha256)
    {
        return Err(PublicationVerificationError::InvalidDescriptor {
            reason: format!("invalid or unordered artifact {}", artifact.logical_path),
        });
    }
    if artifact.bytes > limits.max_artifact_bytes {
        return Err(PublicationVerificationError::LimitExceeded {
            limit: "artifact byte length",
            observed: artifact.bytes,
            maximum: limits.max_artifact_bytes,
        });
    }
    Ok(())
}

fn require_receipt_closure(
    descriptor: &PublicationSetDescriptor,
    receipt_path: &str,
    receipt: &ImportReceipt,
) -> Result<(), PublicationVerificationError> {
    let receipt_record = descriptor
        .artifacts
        .iter()
        .find(|artifact| artifact.logical_path == receipt_path)
        .ok_or_else(|| PublicationVerificationError::ClosureMismatch {
            logical_path: receipt_path.to_owned(),
        })?;
    let outputs: Vec<&PublishedArtifact> = descriptor
        .artifacts
        .iter()
        .filter(|artifact| artifact.logical_path != receipt_path)
        .collect();
    if outputs.len() != receipt.outputs.len() || receipt_record.bytes == 0 {
        return Err(PublicationVerificationError::ClosureMismatch {
            logical_path: receipt_path.to_owned(),
        });
    }
    for (descriptor_output, receipt_output) in outputs.into_iter().zip(&receipt.outputs) {
        require_same_record(descriptor_output, receipt_output)?;
    }
    Ok(())
}

fn require_world_identity(
    receipt: &ImportReceipt,
    world: &SemanticWorldDocument,
    world_path: &str,
) -> Result<GeneratedStateDigest, PublicationVerificationError> {
    if !receipt
        .outputs
        .iter()
        .any(|output| output.logical_path == world_path)
    {
        return Err(PublicationVerificationError::ClosureMismatch {
            logical_path: world_path.to_owned(),
        });
    }
    if !receipt.validation_passed {
        return Err(PublicationVerificationError::ValidationNotPassed);
    }
    require_identity(
        "source",
        &receipt.source_identity_digest,
        &world.source().digest().to_hex(),
    )?;
    let corpus = parse_content_digest("corpus_manifest_digest", &receipt.corpus_manifest_digest)?;
    let policy = parse_content_digest("scope_policy_digest", &receipt.scope_policy_digest)?;
    let computed = world
        .generated_state_digest(corpus, policy)
        .map_err(|source| PublicationVerificationError::InvalidWorld { source })?;
    let expected = receipt
        .state_digest
        .as_deref()
        .ok_or(PublicationVerificationError::MissingStateDigest)?;
    require_identity("generated_state", expected, &computed.to_hex())?;
    Ok(computed)
}

fn require_same_record(
    descriptor: &PublishedArtifact,
    receipt: &ReceiptOutputArtifact,
) -> Result<(), PublicationVerificationError> {
    if descriptor.logical_path != receipt.logical_path
        || descriptor.sha256 != receipt.sha256
        || descriptor.bytes != receipt.bytes
    {
        return Err(PublicationVerificationError::ClosureMismatch {
            logical_path: descriptor.logical_path.clone(),
        });
    }
    Ok(())
}

fn expected_member_bytes(
    expected: &BTreeMap<String, ExpectedArtifact>,
    logical_path: &str,
) -> Result<u64, PublicationVerificationError> {
    expected
        .get(logical_path)
        .map(|artifact| artifact.bytes)
        .ok_or_else(|| PublicationVerificationError::ClosureMismatch {
            logical_path: logical_path.to_owned(),
        })
}

fn expected_artifacts(
    descriptor: &PublicationSetDescriptor,
    descriptor_path: &str,
    descriptor_bytes: &[u8],
) -> Result<BTreeMap<String, ExpectedArtifact>, PublicationVerificationError> {
    let mut expected = descriptor
        .artifacts
        .iter()
        .map(|artifact| {
            (
                artifact.logical_path.clone(),
                ExpectedArtifact {
                    sha256: artifact.sha256.clone(),
                    bytes: artifact.bytes,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    expected.insert(
        descriptor_path.to_owned(),
        ExpectedArtifact {
            sha256: sha256_hex(descriptor_bytes),
            bytes: u64::try_from(descriptor_bytes.len())
                .map_err(|_| PublicationVerificationError::ByteCountOverflow)?,
        },
    );
    Ok(expected)
}

fn validate_path(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), PublicationVerificationError> {
    if value.len() > maximum || !is_logical_path(value) {
        return Err(PublicationVerificationError::InvalidPath {
            field,
            value: value.to_owned(),
        });
    }
    Ok(())
}

fn require_count_limit(
    observed: usize,
    maximum: usize,
) -> Result<(), PublicationVerificationError> {
    if observed > maximum {
        return Err(PublicationVerificationError::LimitExceeded {
            limit: "artifact count",
            observed: u64::try_from(observed)
                .map_err(|_| PublicationVerificationError::ByteCountOverflow)?,
            maximum: u64::try_from(maximum)
                .map_err(|_| PublicationVerificationError::ByteCountOverflow)?,
        });
    }
    Ok(())
}

fn parse_content_digest(
    field: &'static str,
    value: &str,
) -> Result<ContentDigest, PublicationVerificationError> {
    ContentDigest::from_hex(value)
        .map_err(|source| PublicationVerificationError::MalformedDigest { field, source })
}

fn require_identity(
    field: &'static str,
    expected: &str,
    observed: &str,
) -> Result<(), PublicationVerificationError> {
    if expected != observed {
        return Err(PublicationVerificationError::IdentityMismatch {
            field,
            expected: expected.to_owned(),
            observed: observed.to_owned(),
        });
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn is_nonzero_lower_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        && value.bytes().any(|byte| byte != b'0')
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && !value.chars().any(char::is_control)
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
}

fn is_logical_path(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && !value.chars().any(char::is_control)
        && !value.contains(['\\', ':'])
        && !value.starts_with('/')
        && !value.ends_with('/')
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}
