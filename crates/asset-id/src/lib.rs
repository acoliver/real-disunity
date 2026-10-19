use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LogicalAssetId(String);

impl LogicalAssetId {
    pub fn new(value: impl Into<String>) -> Result<Self, AssetIdentityError> {
        let value = value.into();
        validate_logical_id(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LogicalAssetId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for LogicalAssetId {
    type Err = AssetIdentityError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

fn validate_logical_id(value: &str) -> Result<(), AssetIdentityError> {
    if value.is_empty() || value.len() > 192 {
        return Err(AssetIdentityError::InvalidLength);
    }
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        return Err(AssetIdentityError::EmptySegment);
    }
    if !value.contains('.') {
        return Err(AssetIdentityError::MissingNamespace);
    }
    if let Some(character) = value.chars().find(|character| {
        !character.is_ascii_lowercase()
            && !character.is_ascii_digit()
            && !matches!(character, '.' | '_' | '-')
    }) {
        return Err(AssetIdentityError::InvalidCharacter(character));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ContentHash(pub [u8; 32]);

impl ContentHash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn to_hex(self) -> String {
        let mut result = String::with_capacity(64);
        for byte in self.0 {
            use fmt::Write as _;
            write!(&mut result, "{byte:02x}").expect("writing to a String cannot fail");
        }
        result
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum ArtifactKind {
    Texture,
    Mesh,
    Material,
    Terrain,
    Environment,
    Audio,
    Shader,
    Data,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum QualityTier {
    Baseline,
    High,
    Ultra,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct AssetVariant {
    pub art_set: String,
    pub quality: QualityTier,
    pub platform_profile: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum ProvenanceClassification {
    ReferenceOnly,
    ContractCleared,
    Original,
    ThirdParty,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceProvenance {
    pub classification: ProvenanceClassification,
    pub source_package: String,
    pub source_virtual_path: String,
    pub source_sha256: String,
    pub importer_id: String,
    pub importer_version: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum AssetIdentityError {
    #[error("logical asset ID length must be between 1 and 192 bytes")]
    InvalidLength,
    #[error("logical asset ID must contain a namespace separator")]
    MissingNamespace,
    #[error("logical asset ID contains an empty segment")]
    EmptySegment,
    #[error("logical asset ID contains invalid character {0:?}")]
    InvalidCharacter(char),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_canonical_logical_ids() {
        assert!(LogicalAssetId::new("environment.texture.sand_scatter").is_ok());
        assert!(LogicalAssetId::new("Environment.Texture.Sand").is_err());
        assert!(LogicalAssetId::new("no_namespace").is_err());
        assert!(LogicalAssetId::new("terrain..sand").is_err());
    }

    #[test]
    fn content_hash_hex_is_stable() {
        assert_eq!(
            ContentHash::of(b"real-disunity").to_hex(),
            "c54139e8ebd18081f7e267976eb28287f87b61a917c0d7bff59da06eb40a06e5"
        );
    }
}
