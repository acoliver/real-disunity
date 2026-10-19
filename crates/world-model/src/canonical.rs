//! Explicit canonical byte encoding for domain-separated `SHA-256` hashing.
//!
//! Hash inputs are built with an explicit length-prefixed writer, never from
//! serialized JSON bytes. The encoding is frozen so digests stay stable:
//!
//! * every stream starts with the length-prefixed (`u64` little-endian length)
//!   UTF-8 domain tag, then domain-specific fields;
//! * unsigned integers are fixed-width little-endian (`u8`, `u32`, or `u64`);
//! * byte strings and text are `u64` length-prefixed raw bytes;
//! * values are encoded as a one-byte discriminant (`null=0`, `bool=1`,
//!   `signed=2`, `unsigned=3`, `text=4`, `sequence=5`, `map=6`,
//!   `binding_ref=7`) followed by the variant payload;
//! * sequences and maps start with a `u64` element count; map entries are
//!   length-prefixed keys followed by encoded values in `BTreeMap` key order;
//! * components are encoded as length-prefixed `component_id`, length-prefixed
//!   `schema_uri`, then the payload map;
//! * bindings are encoded as length-prefixed `binding_id`, length-prefixed
//!   `capability_uri`, 32 raw digest bytes, `u64` byte length, and a `u8`
//!   presence flag followed by a length-prefixed `element_uri` when present.

use sha2::Digest as _;

use crate::digest::{ContentDigest, finalize_digest};
use crate::document::{ResourceBinding, SchemaDescriptor, SemanticWorldDocument};
use crate::identity::SemanticComponent;
use crate::validation::WorldModelError;
use crate::value::{SemanticMap, SemanticValue};

/// Frozen hashing domain for decoded components and pathless source resources.
pub const DOMAIN_SOURCE_IDENTITY: &str = "real-disunity:world-model:source-identity:v2";
/// Frozen hashing domain for validated world state plus corpus and policy.
pub const DOMAIN_GENERATED_STATE: &str = "real-disunity:world-model:generated-state:v1";

const TAG_NULL: u8 = 0x00;
const TAG_BOOL: u8 = 0x01;
const TAG_SIGNED: u8 = 0x02;
const TAG_UNSIGNED: u8 = 0x03;
const TAG_TEXT: u8 = 0x04;
const TAG_SEQUENCE: u8 = 0x05;
const TAG_MAP: u8 = 0x06;
const TAG_BINDING_REF: u8 = 0x07;

/// Incremental writer that feeds canonical bytes into a `SHA-256` hasher.
pub(crate) struct CanonicalWriter {
    hasher: sha2::Sha256,
}

impl CanonicalWriter {
    /// Starts a stream separated by `domain`.
    pub(crate) fn new(domain: &str) -> Result<Self, WorldModelError> {
        let mut writer = Self {
            hasher: sha2::Sha256::new(),
        };
        writer.write_length_prefixed(domain.as_bytes())?;
        Ok(writer)
    }

    /// Writes a single byte.
    pub(crate) fn write_u8(&mut self, byte: u8) {
        self.hasher.update([byte]);
    }

    /// Writes a `u32` little-endian.
    pub(crate) fn write_u32(&mut self, value: u32) {
        self.hasher.update(value.to_le_bytes());
    }

    /// Writes a `u64` little-endian.
    pub(crate) fn write_u64(&mut self, value: u64) {
        self.hasher.update(value.to_le_bytes());
    }

    /// Writes raw bytes without a length prefix.
    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    /// Writes a `u64` length prefix followed by the bytes.
    pub(crate) fn write_length_prefixed(&mut self, bytes: &[u8]) -> Result<(), WorldModelError> {
        self.write_u64(length_prefix(bytes)?);
        self.write_bytes(bytes);
        Ok(())
    }

    /// Writes the 32 raw bytes of `digest`.
    pub(crate) fn write_digest(&mut self, digest: &ContentDigest) {
        self.write_bytes(digest.as_bytes());
    }

    /// Finishes the stream and returns the computed digest.
    pub(crate) fn finish(self) -> ContentDigest {
        finalize_digest(self.hasher)
    }
}

/// Encodes one decoded save under the source-identity domain.
pub(crate) fn write_source_state(
    writer: &mut CanonicalWriter,
    components: &[SemanticComponent],
    resources: &[ResourceBinding],
) -> Result<(), WorldModelError> {
    write_count(writer, components.len())?;
    for component in components {
        write_component(writer, component)?;
    }
    write_count(writer, resources.len())?;
    for resource in resources {
        write_binding(writer, resource)?;
    }
    Ok(())
}

/// Encodes a complete validated document plus corpus and policy digests under
/// the generated-state domain.
pub(crate) fn write_generated_state(
    writer: &mut CanonicalWriter,
    document: &SemanticWorldDocument,
    corpus_manifest: &ContentDigest,
    policy: &ContentDigest,
) -> Result<(), WorldModelError> {
    let schema: &SchemaDescriptor = document.schema();
    writer.write_length_prefixed(schema.tag.as_bytes())?;
    writer.write_u32(schema.version);
    writer.write_digest(document.source().digest());
    let components = document.components();
    write_count(writer, components.len())?;
    for component in components {
        write_component(writer, component)?;
    }
    let bindings = document.bindings();
    write_count(writer, bindings.len())?;
    for binding in bindings {
        write_binding(writer, binding)?;
    }
    writer.write_digest(corpus_manifest);
    writer.write_digest(policy);
    Ok(())
}

fn write_component(
    writer: &mut CanonicalWriter,
    component: &SemanticComponent,
) -> Result<(), WorldModelError> {
    writer.write_length_prefixed(component.component_id.as_bytes())?;
    writer.write_length_prefixed(component.schema_uri.as_bytes())?;
    write_map(writer, &component.payload)
}

fn write_binding(
    writer: &mut CanonicalWriter,
    binding: &ResourceBinding,
) -> Result<(), WorldModelError> {
    writer.write_length_prefixed(binding.binding_id.as_bytes())?;
    writer.write_length_prefixed(binding.capability_uri.as_bytes())?;
    writer.write_digest(&binding.content);
    writer.write_u64(binding.byte_length);
    match &binding.element_uri {
        Some(uri) => {
            writer.write_u8(1);
            writer.write_length_prefixed(uri.as_bytes())?;
        }
        None => writer.write_u8(0),
    }
    Ok(())
}

fn write_value(writer: &mut CanonicalWriter, value: &SemanticValue) -> Result<(), WorldModelError> {
    match value {
        SemanticValue::Null => writer.write_u8(TAG_NULL),
        SemanticValue::Bool(flag) => {
            writer.write_u8(TAG_BOOL);
            writer.write_u8(u8::from(*flag));
        }
        SemanticValue::Signed(value) => {
            writer.write_u8(TAG_SIGNED);
            writer.write_bytes(&value.to_le_bytes());
        }
        SemanticValue::Unsigned(value) => {
            writer.write_u8(TAG_UNSIGNED);
            writer.write_u64(*value);
        }
        SemanticValue::Text(text) => {
            writer.write_u8(TAG_TEXT);
            writer.write_length_prefixed(text.as_bytes())?;
        }
        SemanticValue::Sequence(items) => {
            writer.write_u8(TAG_SEQUENCE);
            write_count(writer, items.len())?;
            for item in items {
                write_value(writer, item)?;
            }
        }
        SemanticValue::Map(map) => {
            writer.write_u8(TAG_MAP);
            write_map(writer, map)?;
        }
        SemanticValue::BindingRef(binding_id) => {
            writer.write_u8(TAG_BINDING_REF);
            writer.write_length_prefixed(binding_id.as_bytes())?;
        }
    }
    Ok(())
}

fn write_map(writer: &mut CanonicalWriter, map: &SemanticMap) -> Result<(), WorldModelError> {
    write_count(writer, map.len())?;
    for (key, value) in map.iter() {
        writer.write_length_prefixed(key.as_bytes())?;
        write_value(writer, value)?;
    }
    Ok(())
}

fn write_count(writer: &mut CanonicalWriter, count: usize) -> Result<(), WorldModelError> {
    writer.write_u64(length_prefix_of(count)?);
    Ok(())
}

fn length_prefix(bytes: &[u8]) -> Result<u64, WorldModelError> {
    length_prefix_of(bytes.len())
}

fn length_prefix_of(count: usize) -> Result<u64, WorldModelError> {
    u64::try_from(count).map_err(|_| WorldModelError::CounterOverflow)
}
