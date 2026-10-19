//! Runtime world documents and content-addressed resource bindings.

use serde::{Deserialize, Serialize};

use crate::canonical::{self, CanonicalWriter, DOMAIN_GENERATED_STATE};
use crate::digest::{ContentDigest, GeneratedStateDigest};
use crate::identity::{SemanticComponent, SourceIdentity};
use crate::validation::{
    ValidationLimits, WorldModelError, validate_binding, validate_components, validate_structure,
};

/// Tagged version of the world document schema family.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaDescriptor {
    /// Structural URI naming the schema family.
    pub tag: String,
    /// Version of the schema family.
    pub version: u32,
}

/// Content-addressed resource binding.
///
/// Bindings reference content strictly by digest and byte length, so the same
/// content is representable regardless of where it is physically stored. There
/// is deliberately no field for a physical or logical path, import status,
/// source format, profile, or a closed role enum.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBinding {
    /// Stable binding identifier referenced by `BindingRef` payload values.
    pub binding_id: String,
    /// Structural URI of the capability the bound content serves.
    pub capability_uri: String,
    /// Nonzero `SHA-256` digest of the bound content.
    pub content: ContentDigest,
    /// Byte length of the bound content; zero is valid.
    pub byte_length: u64,
    /// Optional structural URI of an element within the bound content.
    pub element_uri: Option<String>,
}

impl ResourceBinding {
    /// Validates and adopts binding fields into one content-addressed binding.
    ///
    /// Universal constructor for external decoders and resolution seams: it
    /// applies the same identifier and URI checks as whole-document validation.
    /// Whole-document construction remains the authority that validates every
    /// binding regardless of how it was created. Zero byte length is valid; the
    /// digest of empty content is the ordinary nonzero hash of zero bytes.
    ///
    /// # Errors
    ///
    /// Returns [`WorldModelError`] when the binding identifier, capability
    /// URI, or element URI is malformed.
    pub fn new(
        binding_id: &str,
        capability_uri: &str,
        content: ContentDigest,
        byte_length: u64,
        element_uri: Option<&str>,
    ) -> Result<Self, WorldModelError> {
        let binding = Self {
            binding_id: binding_id.to_owned(),
            capability_uri: capability_uri.to_owned(),
            content,
            byte_length,
            element_uri: element_uri.map(str::to_owned),
        };
        validate_binding(&binding)?;
        Ok(binding)
    }
}

/// Validated runtime world document.
///
/// The document carries a schema descriptor, the exact [`SourceIdentity`] it
/// was derived from, strictly ordered runtime components, and strictly ordered
/// content-addressed bindings whose identifiers resolve every `BindingRef`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SemanticWorldDocument {
    schema: SchemaDescriptor,
    source: SourceIdentity,
    components: Vec<SemanticComponent>,
    bindings: Vec<ResourceBinding>,
}

impl SemanticWorldDocument {
    /// Validates and adopts the document parts under explicit limits.
    pub fn new(
        schema: SchemaDescriptor,
        source: SourceIdentity,
        components: Vec<SemanticComponent>,
        bindings: Vec<ResourceBinding>,
        limits: &ValidationLimits,
    ) -> Result<Self, WorldModelError> {
        validate_contents(&schema, &source, &components, &bindings, limits)?;
        Ok(Self {
            schema,
            source,
            components,
            bindings,
        })
    }

    /// Returns the schema descriptor.
    pub fn schema(&self) -> &SchemaDescriptor {
        &self.schema
    }

    /// Returns the exact identity of the source the document was derived from.
    pub fn source(&self) -> &SourceIdentity {
        &self.source
    }

    /// Returns the strictly ordered runtime components.
    pub fn components(&self) -> &[SemanticComponent] {
        &self.components
    }

    /// Returns the strictly ordered content-addressed bindings.
    pub fn bindings(&self) -> &[ResourceBinding] {
        &self.bindings
    }

    /// Re-runs full validation, for instances obtained via deserialization.
    pub fn validate(&self, limits: &ValidationLimits) -> Result<(), WorldModelError> {
        validate_contents(
            &self.schema,
            &self.source,
            &self.components,
            &self.bindings,
            limits,
        )
    }

    /// Computes the generated-state digest of this document.
    ///
    /// The digest covers the complete stored document plus the caller-supplied
    /// exact corpus-manifest and policy digests. Mutating the world, the
    /// corpus, or the policy changes the digest independently.
    pub fn generated_state_digest(
        &self,
        corpus_manifest: ContentDigest,
        policy: ContentDigest,
    ) -> Result<GeneratedStateDigest, WorldModelError> {
        let mut writer = CanonicalWriter::new(DOMAIN_GENERATED_STATE)?;
        canonical::write_generated_state(&mut writer, self, &corpus_manifest, &policy)?;
        Ok(GeneratedStateDigest::from_computed(
            *writer.finish().as_bytes(),
        ))
    }

    fn from_structural(
        schema: SchemaDescriptor,
        source: SourceIdentity,
        components: Vec<SemanticComponent>,
        bindings: Vec<ResourceBinding>,
    ) -> Result<Self, WorldModelError> {
        validate_structure(&schema.tag, &components, &bindings)?;
        Ok(Self {
            schema,
            source,
            components,
            bindings,
        })
    }
}

fn validate_contents(
    schema: &SchemaDescriptor,
    _source: &SourceIdentity,
    components: &[SemanticComponent],
    bindings: &[ResourceBinding],
    limits: &ValidationLimits,
) -> Result<(), WorldModelError> {
    validate_structure(&schema.tag, components, bindings)?;
    validate_components(components, limits)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSemanticWorldDocument {
    schema: SchemaDescriptor,
    source: SourceIdentity,
    components: Vec<SemanticComponent>,
    bindings: Vec<ResourceBinding>,
}

impl<'de> Deserialize<'de> for SemanticWorldDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawSemanticWorldDocument::deserialize(deserializer)?;
        // Ordering, identifiers, URIs, digests, and binding resolution are
        // enforced eagerly; payload budgets are deferred to `validate` because
        // limits are caller-supplied.
        SemanticWorldDocument::from_structural(raw.schema, raw.source, raw.components, raw.bindings)
            .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SHA256_LEN;

    fn content() -> ContentDigest {
        ContentDigest::from_bytes([0x11; SHA256_LEN]).expect("nonzero digest")
    }

    #[test]
    fn constructor_adopts_valid_fields_and_preserves_optionality() {
        let plain = ResourceBinding::new(
            "arbitrary-alpha-binding",
            "synthetic:capability/arbitrary-bytes",
            content(),
            0,
            None,
        )
        .expect("valid binding");
        assert_eq!(plain.binding_id, "arbitrary-alpha-binding");
        assert_eq!(plain.capability_uri, "synthetic:capability/arbitrary-bytes");
        assert_eq!(plain.content, content());
        assert_eq!(plain.byte_length, 0);
        assert_eq!(plain.element_uri, None);

        let with_element = ResourceBinding::new(
            "arbitrary-beta-binding",
            "synthetic:capability/arbitrary-element",
            content(),
            7,
            Some("synthetic:element/arbitrary-leaf"),
        )
        .expect("valid binding");
        assert_eq!(
            with_element.element_uri.as_deref(),
            Some("synthetic:element/arbitrary-leaf")
        );
    }

    #[test]
    fn constructor_rejects_malformed_identifiers_and_uris() {
        let digest = content();
        assert!(matches!(
            ResourceBinding::new("", "synthetic:capability/x", digest, 1, None),
            Err(WorldModelError::InvalidIdentifier { .. })
        ));
        assert!(matches!(
            ResourceBinding::new(
                "arbitrary\u{0}binding",
                "synthetic:capability/x",
                digest,
                1,
                None
            ),
            Err(WorldModelError::InvalidIdentifier { .. })
        ));
        assert!(matches!(
            ResourceBinding::new(
                "arbitrary-alpha-binding",
                "missing-scheme-separator",
                digest,
                1,
                None
            ),
            Err(WorldModelError::InvalidUri { .. })
        ));
        assert!(matches!(
            ResourceBinding::new(
                "arbitrary-alpha-binding",
                "synthetic:capability/x",
                digest,
                1,
                Some("bad uri")
            ),
            Err(WorldModelError::InvalidUri { .. })
        ));
    }
}
