use crate::validation::{first_unsorted_unique, is_identifier, is_logical_path, is_uri};
use crate::version::{
    SAVE_BUNDLE_MANIFEST_SCHEMA, SAVE_BUNDLE_MANIFEST_VERSION, VersionedDocumentError,
    parse_versioned, to_canonical_vec, to_pretty_vec, verify_header,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// One named sampled axis of a data-defined grid input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisDescriptor {
    pub axis_name: String,
    pub length: u64,
}

/// Generic sampled field descriptor for terrain-like inputs.
///
/// The field identity, element encoding, axis names, and extents are all data-defined through
/// identifiers, URIs, and declared lengths; nothing is fixed in Rust.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SampledFieldDescriptor {
    pub field_id: String,
    pub element_type_uri: String,
    pub axes: Vec<AxisDescriptor>,
}

impl SampledFieldDescriptor {
    /// Returns the checked product of all axis lengths.
    #[must_use]
    pub fn element_count(&self) -> Option<u64> {
        self.axes
            .iter()
            .map(|axis| axis.length)
            .try_fold(1_u64, u64::checked_mul)
    }

    /// Validates field identity, element type, axes, and extent arithmetic.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed descriptor violation.
    pub fn validate(&self) -> Result<(), SaveBundleError> {
        if !is_identifier(&self.field_id) {
            return Err(invalid_field(
                "fields.field_id",
                "nonempty identifier",
                &self.field_id,
            ));
        }
        if !is_uri(&self.element_type_uri) {
            return Err(invalid_field(
                "fields.element_type_uri",
                "element type URI",
                &self.element_type_uri,
            ));
        }
        if self.axes.is_empty() {
            return Err(invalid_field("fields.axes", "at least one axis", "empty"));
        }
        validate_axes(&self.axes)?;
        if self.element_count().is_none() {
            return Err(invalid_field(
                "fields.axes",
                "non-overflowing element count",
                "overflow",
            ));
        }
        Ok(())
    }
}

fn validate_axes(axes: &[AxisDescriptor]) -> Result<(), SaveBundleError> {
    for axis in axes {
        if !is_identifier(&axis.axis_name) {
            return Err(invalid_field(
                "fields.axes.axis_name",
                "nonempty identifier",
                &axis.axis_name,
            ));
        }
        if axis.length == 0 {
            return Err(invalid_field(
                "fields.axes.length",
                "positive axis length",
                "0",
            ));
        }
    }
    Ok(())
}

/// Generic homogeneous collection expected from structural decoding, before any semantic-registry
/// normalization, described by a type URI and optional count.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentSetDescriptor {
    pub component_type_uri: String,
    pub record_count: Option<u64>,
}

/// Typed metadata carried by a manifest entry through extensible capability and type URIs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDescriptor {
    pub capability_uris: Vec<String>,
    pub parameters: BTreeMap<String, Value>,
    pub fields: Vec<SampledFieldDescriptor>,
    pub component_sets: Vec<ComponentSetDescriptor>,
}

/// One arbitrary artifact observed inside a save bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveBundleEntry {
    pub entry_id: String,
    pub schema_uri: String,
    pub logical_path: String,
    pub required: bool,
    pub descriptor: Option<ArtifactDescriptor>,
}

/// Versioned, generic enumeration of the artifacts inside one save bundle.
///
/// The manifest places no limit on entry count and no domain constraint on names; it only
/// requires stable entry identities, schema URIs, safe logical paths, and an explicit
/// required/optional declaration per entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveBundleManifest {
    pub schema: String,
    pub schema_version: u32,
    pub manifest_id: String,
    pub source_uri: String,
    pub entries: Vec<SaveBundleEntry>,
}

impl SaveBundleManifest {
    /// Parses and validates a save-bundle manifest.
    ///
    /// # Errors
    ///
    /// Returns a version, JSON, or manifest validation error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, SaveBundleError> {
        let manifest: Self = parse_versioned(
            bytes,
            SAVE_BUNDLE_MANIFEST_SCHEMA,
            SAVE_BUNDLE_MANIFEST_VERSION,
        )?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Serializes this manifest as deterministic pretty JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is invalid or serialization fails.
    pub fn to_json_pretty(&self) -> Result<Vec<u8>, SaveBundleError> {
        self.validate()?;
        Ok(to_pretty_vec(self)?)
    }

    /// Serializes this manifest as compact canonical JSON after validation.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is invalid or serialization fails.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, SaveBundleError> {
        self.validate()?;
        Ok(to_canonical_vec(self)?)
    }

    /// Finds an entry by its stable entry identifier.
    #[must_use]
    pub fn entry(&self, entry_id: &str) -> Option<&SaveBundleEntry> {
        self.entries
            .binary_search_by_key(&entry_id, |entry| entry.entry_id.as_str())
            .ok()
            .map(|index| &self.entries[index])
    }

    /// Validates the envelope, identity, URI, and every entry with its descriptor.
    ///
    /// # Errors
    ///
    /// Returns the first fail-closed manifest violation.
    pub fn validate(&self) -> Result<(), SaveBundleError> {
        verify_header(
            &crate::DocumentHeader {
                schema: self.schema.clone(),
                schema_version: self.schema_version,
            },
            SAVE_BUNDLE_MANIFEST_SCHEMA,
            SAVE_BUNDLE_MANIFEST_VERSION,
        )?;
        if !is_identifier(&self.manifest_id) {
            return Err(invalid_field(
                "manifest_id",
                "nonempty identifier",
                &self.manifest_id,
            ));
        }
        if !is_uri(&self.source_uri) {
            return Err(invalid_field("source_uri", "source URI", &self.source_uri));
        }
        validate_entries(&self.entries)
    }
}

fn validate_entries(entries: &[SaveBundleEntry]) -> Result<(), SaveBundleError> {
    if entries.is_empty() {
        return Err(invalid_field("entries", "at least one entry", "empty"));
    }
    let entry_ids: Vec<String> = entries.iter().map(|entry| entry.entry_id.clone()).collect();
    if let Some(index) = first_unsorted_unique(&entry_ids) {
        return Err(invalid_field(
            "entries.entry_id",
            "strictly sorted unique identifiers",
            &entries[index].entry_id,
        ));
    }
    let mut paths = BTreeSet::new();
    for entry in entries {
        validate_entry(entry)?;
        if !paths.insert(entry.logical_path.as_str()) {
            return Err(invalid_field(
                "entries.logical_path",
                "unique logical path",
                &entry.logical_path,
            ));
        }
    }
    Ok(())
}

fn validate_entry(entry: &SaveBundleEntry) -> Result<(), SaveBundleError> {
    if !is_identifier(&entry.entry_id) {
        return Err(invalid_field(
            "entries.entry_id",
            "nonempty identifier",
            &entry.entry_id,
        ));
    }
    if !is_uri(&entry.schema_uri) {
        return Err(invalid_field(
            "entries.schema_uri",
            "schema URI",
            &entry.schema_uri,
        ));
    }
    if !is_logical_path(&entry.logical_path) {
        return Err(invalid_field(
            "entries.logical_path",
            "normalized relative logical path",
            &entry.logical_path,
        ));
    }
    if let Some(descriptor) = &entry.descriptor {
        validate_descriptor(descriptor)?;
    }
    Ok(())
}

fn validate_descriptor(descriptor: &ArtifactDescriptor) -> Result<(), SaveBundleError> {
    if let Some(index) = first_unsorted_unique(&descriptor.capability_uris) {
        return Err(invalid_field(
            "descriptor.capability_uris",
            "strictly sorted unique URIs",
            &descriptor.capability_uris[index],
        ));
    }
    if descriptor.capability_uris.iter().any(|uri| !is_uri(uri)) {
        return Err(invalid_field(
            "descriptor.capability_uris",
            "capability URI",
            "invalid URI",
        ));
    }
    validate_parameters(&descriptor.parameters)?;
    validate_fields(&descriptor.fields)?;
    validate_component_sets(&descriptor.component_sets)?;
    if descriptor_has_no_declarations(descriptor) {
        return Err(invalid_field(
            "descriptor",
            "at least one capability, parameter, field, or component set",
            "empty",
        ));
    }
    Ok(())
}

fn descriptor_has_no_declarations(descriptor: &ArtifactDescriptor) -> bool {
    descriptor.capability_uris.is_empty()
        && descriptor.parameters.is_empty()
        && descriptor.fields.is_empty()
        && descriptor.component_sets.is_empty()
}

fn validate_parameters(parameters: &BTreeMap<String, Value>) -> Result<(), SaveBundleError> {
    for (key, value) in parameters {
        if !is_identifier(key) || value.is_null() {
            return Err(invalid_field(
                "descriptor.parameters",
                "identifier key with non-null value",
                key,
            ));
        }
    }
    Ok(())
}

fn validate_fields(fields: &[SampledFieldDescriptor]) -> Result<(), SaveBundleError> {
    let field_ids: Vec<String> = fields.iter().map(|field| field.field_id.clone()).collect();
    if let Some(index) = first_unsorted_unique(&field_ids) {
        return Err(invalid_field(
            "descriptor.fields",
            "strictly sorted unique field identifiers",
            &fields[index].field_id,
        ));
    }
    for field in fields {
        field.validate()?;
    }
    Ok(())
}

fn validate_component_sets(sets: &[ComponentSetDescriptor]) -> Result<(), SaveBundleError> {
    let type_uris: Vec<String> = sets
        .iter()
        .map(|set| set.component_type_uri.clone())
        .collect();
    if let Some(index) = first_unsorted_unique(&type_uris) {
        return Err(invalid_field(
            "descriptor.component_sets",
            "strictly sorted unique component type URIs",
            &sets[index].component_type_uri,
        ));
    }
    for set in sets {
        validate_component_set(set)?;
    }
    Ok(())
}

fn validate_component_set(set: &ComponentSetDescriptor) -> Result<(), SaveBundleError> {
    if !is_uri(&set.component_type_uri) {
        return Err(invalid_field(
            "descriptor.component_sets.component_type_uri",
            "component type URI",
            &set.component_type_uri,
        ));
    }
    if set.record_count == Some(0) {
        return Err(invalid_field(
            "descriptor.component_sets.record_count",
            "positive record count",
            "0",
        ));
    }
    Ok(())
}

fn invalid_field(field: &'static str, expected: &str, observed: &str) -> SaveBundleError {
    SaveBundleError::FieldMismatch {
        field,
        expected: expected.to_owned(),
        observed: observed.to_owned(),
    }
}

/// Save-bundle manifest decoding and field-validation errors.
#[derive(Debug, Error)]
pub enum SaveBundleError {
    #[error(transparent)]
    Document(#[from] VersionedDocumentError),
    #[error("save-bundle field {field} mismatch: expected {expected}, observed {observed}")]
    FieldMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(entry_id: &str, logical_path: &str) -> SaveBundleEntry {
        SaveBundleEntry {
            entry_id: entry_id.to_owned(),
            schema_uri: "urn:real-disunity:artifact:opaque".to_owned(),
            logical_path: logical_path.to_owned(),
            required: true,
            descriptor: None,
        }
    }

    fn terrain_entry() -> SaveBundleEntry {
        let axes = vec![
            AxisDescriptor {
                axis_name: "column".to_owned(),
                length: 8,
            },
            AxisDescriptor {
                axis_name: "row".to_owned(),
                length: 6,
            },
        ];
        SaveBundleEntry {
            entry_id: "field-alpha".to_owned(),
            schema_uri: "urn:real-disunity:artifact:grid".to_owned(),
            logical_path: "streams/field-alpha.grid".to_owned(),
            required: true,
            descriptor: Some(ArtifactDescriptor {
                capability_uris: vec![
                    "urn:real-disunity:capability:sampled-field".to_owned(),
                    "urn:real-disunity:capability:terrain-input".to_owned(),
                ],
                parameters: BTreeMap::from([("spacing".to_owned(), Value::from(4))]),
                fields: vec![SampledFieldDescriptor {
                    field_id: "height".to_owned(),
                    element_type_uri: "urn:real-disunity:element:f32le".to_owned(),
                    axes,
                }],
                component_sets: Vec::new(),
            }),
        }
    }

    fn manifest() -> SaveBundleManifest {
        let components = SaveBundleEntry {
            entry_id: "relays".to_owned(),
            schema_uri: "urn:real-disunity:artifact:components".to_owned(),
            logical_path: "streams/relays.bin".to_owned(),
            required: false,
            descriptor: Some(ArtifactDescriptor {
                capability_uris: Vec::new(),
                parameters: BTreeMap::new(),
                fields: Vec::new(),
                component_sets: vec![ComponentSetDescriptor {
                    component_type_uri: "urn:real-disunity:component:relay".to_owned(),
                    record_count: Some(12),
                }],
            }),
        };
        SaveBundleManifest {
            schema: SAVE_BUNDLE_MANIFEST_SCHEMA.to_owned(),
            schema_version: SAVE_BUNDLE_MANIFEST_VERSION,
            manifest_id: "arbitrary-bundle-name".to_owned(),
            source_uri: "urn:real-disunity:source:arbitrary-slot".to_owned(),
            entries: vec![
                terrain_entry(),
                components,
                entry("zz-opaque", "misc/zz-opaque.bin"),
            ],
        }
    }

    #[test]
    fn manifest_round_trip_is_deterministic_with_arbitrary_names() {
        let document = manifest();
        let bytes = document.to_json_bytes().unwrap();
        assert_eq!(document.to_json_bytes().unwrap(), bytes);
        assert_eq!(SaveBundleManifest::from_json(&bytes).unwrap(), document);
        assert_eq!(
            document.entry("relays").map(|entry| entry.required),
            Some(false)
        );
    }

    #[test]
    fn duplicate_entry_ids_and_paths_are_rejected() {
        let mut duplicate_id = manifest();
        duplicate_id.entries[2] = entry("relays", "misc/zz-opaque.bin");
        assert!(matches!(
            duplicate_id.validate(),
            Err(SaveBundleError::FieldMismatch {
                field: "entries.entry_id",
                ..
            })
        ));

        let mut duplicate_path = manifest();
        duplicate_path.entries[2] = entry("zz-opaque", "streams/relays.bin");
        assert!(matches!(
            duplicate_path.validate(),
            Err(SaveBundleError::FieldMismatch {
                field: "entries.logical_path",
                ..
            })
        ));
    }

    #[test]
    fn unsafe_paths_and_empty_manifests_are_rejected() {
        let mut unsafe_path = manifest();
        unsafe_path.entries[2] = entry("zz-opaque", "../escape.bin");
        assert!(matches!(
            unsafe_path.validate(),
            Err(SaveBundleError::FieldMismatch {
                field: "entries.logical_path",
                ..
            })
        ));

        let mut empty = manifest();
        empty.entries.clear();
        assert!(matches!(
            empty.validate(),
            Err(SaveBundleError::FieldMismatch {
                field: "entries",
                ..
            })
        ));
    }

    #[test]
    fn zero_axis_length_and_empty_descriptors_are_rejected() {
        let mut zero_axis = manifest();
        zero_axis.entries[0].descriptor.as_mut().unwrap().fields[0].axes[0].length = 0;
        assert!(matches!(
            zero_axis.validate(),
            Err(SaveBundleError::FieldMismatch {
                field: "fields.axes.length",
                ..
            })
        ));

        let mut empty_descriptor = manifest();
        empty_descriptor.entries[0].descriptor = Some(ArtifactDescriptor {
            capability_uris: Vec::new(),
            parameters: BTreeMap::new(),
            fields: Vec::new(),
            component_sets: Vec::new(),
        });
        assert!(matches!(
            empty_descriptor.validate(),
            Err(SaveBundleError::FieldMismatch {
                field: "descriptor",
                ..
            })
        ));
    }

    #[test]
    fn unknown_fields_and_future_versions_are_rejected() {
        let bytes = manifest().to_json_bytes().unwrap();
        let with_unknown = String::from_utf8(bytes).unwrap().replace(
            "\"manifest_id\"",
            "\"unexpected_field\": 1, \"manifest_id\"",
        );
        assert!(matches!(
            SaveBundleManifest::from_json(with_unknown.as_bytes()),
            Err(SaveBundleError::Document(VersionedDocumentError::Json(_)))
        ));

        let future = String::from_utf8(manifest().to_json_bytes().unwrap())
            .unwrap()
            .replace("\"schema_version\":1", "\"schema_version\":9");
        assert!(matches!(
            SaveBundleManifest::from_json(future.as_bytes()),
            Err(SaveBundleError::Document(
                VersionedDocumentError::UnsupportedSchemaVersion { .. }
            ))
        ));
    }
}
