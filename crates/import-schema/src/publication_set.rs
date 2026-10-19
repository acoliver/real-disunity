use serde::{Deserialize, Serialize};

use crate::version::{
    PUBLICATION_SET_SCHEMA, PUBLICATION_SET_VERSION, VersionedDocumentError, parse_versioned,
};

/// One exact member of an immutable publication set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishedArtifact {
    pub logical_path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Root descriptor binding a receipt and all receipt-recorded outputs.
///
/// The descriptor is the publication trust root and therefore does not hash
/// itself. Exact-set verification must account for the descriptor separately.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationSetDescriptor {
    pub schema: String,
    pub schema_version: u32,
    pub publication_id: String,
    pub artifacts: Vec<PublishedArtifact>,
}

impl PublicationSetDescriptor {
    /// Parses the exact supported descriptor schema and version.
    pub fn from_json(bytes: &[u8]) -> Result<Self, VersionedDocumentError> {
        parse_versioned(bytes, PUBLICATION_SET_SCHEMA, PUBLICATION_SET_VERSION)
    }

    /// Serializes deterministic compact JSON for publication.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> PublicationSetDescriptor {
        PublicationSetDescriptor {
            schema: PUBLICATION_SET_SCHEMA.to_owned(),
            schema_version: PUBLICATION_SET_VERSION,
            publication_id: "publication".to_owned(),
            artifacts: vec![PublishedArtifact {
                logical_path: "world.json".to_owned(),
                sha256: "1".repeat(64),
                bytes: 7,
            }],
        }
    }

    #[test]
    fn compact_round_trip_preserves_descriptor() {
        let descriptor = descriptor();
        let bytes = descriptor.to_json_bytes().unwrap();
        assert!(!bytes.contains(&b'\n'));
        assert_eq!(
            PublicationSetDescriptor::from_json(&bytes).unwrap(),
            descriptor
        );
    }

    #[test]
    fn wrong_schema_and_version_fail_closed() {
        let mut wrong_schema = descriptor();
        wrong_schema.schema = "test:other".to_owned();
        assert!(matches!(
            PublicationSetDescriptor::from_json(&wrong_schema.to_json_bytes().unwrap()),
            Err(VersionedDocumentError::SchemaTagMismatch { .. })
        ));

        let mut wrong_version = descriptor();
        wrong_version.schema_version += 1;
        assert!(matches!(
            PublicationSetDescriptor::from_json(&wrong_version.to_json_bytes().unwrap()),
            Err(VersionedDocumentError::UnsupportedSchemaVersion { .. })
        ));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let bytes = br#"{"schema":"real-disunity.publication-set.v1","schema_version":1,"publication_id":"publication","artifacts":[],"extra":true}"#;
        assert!(matches!(
            PublicationSetDescriptor::from_json(bytes),
            Err(VersionedDocumentError::Json(_))
        ));
    }
}
