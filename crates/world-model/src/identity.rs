//! Corpus-free decoding results and the source identity derived from them.

use serde::{Deserialize, Serialize};

use crate::canonical::{self, CanonicalWriter, DOMAIN_SOURCE_IDENTITY};
use crate::digest::ContentDigest;
use crate::document::ResourceBinding;
use crate::validation::{
    ValidationLimits, WorldModelError, validate_decoded_save, validate_decoded_save_structure,
};
use crate::value::SemanticMap;

/// One arbitrary decoded semantic unit.
///
/// Components impose no entity hierarchy: any stable identifier, schema URI,
/// and map payload combination is representable, so decode layers can emit
/// whatever a source actually contained rather than a prescribed shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticComponent {
    /// Stable caller-authored identifier, unique within its collection.
    pub component_id: String,
    /// Structural URI of the schema the payload claims to follow.
    pub schema_uri: String,
    /// Arbitrary semantic payload.
    pub payload: SemanticMap,
}

/// Corpus-free result of decoding one save.
///
/// Components and pathless, content-addressed source resources are each held
/// in strict identifier order. No physical path, manifest/lock identity,
/// corpus, policy, timestamp, or review metadata can be represented here.
/// Identity is a pure function of these decoded semantics and exact source
/// resource addresses, so equivalent saves hash identically in any context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DecodedSave {
    components: Vec<SemanticComponent>,
    resources: Vec<ResourceBinding>,
}

impl DecodedSave {
    /// Validates components with no decoded source resources.
    pub fn new(
        components: Vec<SemanticComponent>,
        limits: &ValidationLimits,
    ) -> Result<Self, WorldModelError> {
        Self::with_resources(components, Vec::new(), limits)
    }

    /// Validates components and pathless source resources under explicit limits.
    pub fn with_resources(
        components: Vec<SemanticComponent>,
        resources: Vec<ResourceBinding>,
        limits: &ValidationLimits,
    ) -> Result<Self, WorldModelError> {
        let save = Self::from_structural(components, resources)?;
        save.validate(limits)?;
        Ok(save)
    }

    /// Returns the strictly ordered components.
    pub fn components(&self) -> &[SemanticComponent] {
        &self.components
    }

    /// Returns the strictly ordered, pathless source resources.
    pub fn resources(&self) -> &[ResourceBinding] {
        &self.resources
    }

    /// Re-runs full validation, for instances obtained via deserialization.
    pub fn validate(&self, limits: &ValidationLimits) -> Result<(), WorldModelError> {
        validate_decoded_save(self.components(), self.resources(), limits)
    }

    fn from_structural(
        components: Vec<SemanticComponent>,
        resources: Vec<ResourceBinding>,
    ) -> Result<Self, WorldModelError> {
        validate_decoded_save_structure(&components, &resources)?;
        Ok(Self {
            components,
            resources,
        })
    }
}

/// Canonical identity of a decoded save.
///
/// Derived solely from the canonical encoding of decoded components and
/// pathless source resources under [`crate::DOMAIN_SOURCE_IDENTITY`]. Because
/// no provenance can be attached to a [`DecodedSave`], identity is independent
/// of corpus, policy, and physical storage by construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceIdentity {
    digest: ContentDigest,
}

impl SourceIdentity {
    /// Derives identity from canonical components and source resources.
    pub fn of(save: &DecodedSave) -> Result<Self, WorldModelError> {
        let mut writer = CanonicalWriter::new(DOMAIN_SOURCE_IDENTITY)?;
        canonical::write_source_state(&mut writer, save.components(), save.resources())?;
        Ok(Self {
            digest: writer.finish(),
        })
    }

    /// Returns the underlying content digest.
    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }
}

impl Serialize for SourceIdentity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.digest.to_hex())
    }
}

impl<'de> Deserialize<'de> for SourceIdentity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let hex = String::deserialize(deserializer)?;
        let digest = ContentDigest::from_hex(&hex).map_err(serde::de::Error::custom)?;
        Ok(Self { digest })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDecodedSave {
    components: Vec<SemanticComponent>,
    resources: Vec<ResourceBinding>,
}

impl<'de> Deserialize<'de> for DecodedSave {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawDecodedSave::deserialize(deserializer)?;
        // Ordering and uniqueness are enforced eagerly; payload validation is
        // deferred to `validate` because budgets are caller-supplied.
        DecodedSave::from_structural(raw.components, raw.resources)
            .map_err(serde::de::Error::custom)
    }
}
