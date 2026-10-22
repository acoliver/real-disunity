//! Envelope, round-trip, and strictness tests for authored render contracts.

mod common;

use common::{contract_bytes, valid_contract};
use serde_json::json;
use worldrunner_render_plan::{
    ContractDocument, ContractDocumentError, RENDER_CONTRACT_SCHEMA, RENDER_CONTRACT_SCHEMA_VERSION,
};
#[test]
fn contract_identity_has_an_independent_version_boundary() {
    assert_eq!(RENDER_CONTRACT_SCHEMA, "real-disunity.render-contract.v2");
    assert_eq!(RENDER_CONTRACT_SCHEMA_VERSION, 2);
    assert_eq!(
        worldrunner_render_plan::RENDER_CONTRACT_CAPABILITY_URI,
        "urn:real-disunity:capability:render-contract:v2"
    );
    let mut contract = valid_contract();
    contract["schema"] = json!("legacy.render-contract.v1");
    assert!(matches!(
        ContractDocument::from_json_bytes(&contract_bytes(&contract)),
        Err(ContractDocumentError::SchemaTagMismatch { .. })
    ));
}

#[test]
fn valid_document_round_trips_through_json() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    let encoded = document
        .to_json_bytes()
        .expect("validated document encodes");
    let redecoded = ContractDocument::from_json_bytes(&encoded)
        .expect("re-encoded document must decode identically");
    assert_eq!(document, redecoded);
    assert_eq!(
        redecoded.to_json_bytes().expect("second pass encodes"),
        encoded
    );
}

#[test]
fn decoded_document_reports_exact_authored_parts() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    assert_eq!(document.header().schema, RENDER_CONTRACT_SCHEMA);
    assert_eq!(
        document.header().schema_version,
        RENDER_CONTRACT_SCHEMA_VERSION
    );
    assert_eq!(document.contract_id(), "synthetic-contract-alpha");
    assert_eq!(document.scene().meshes().len(), 1);
    assert_eq!(document.terrain().material_palette().len(), 1);
    assert_eq!(document.environment().lod_policies().len(), 1);
    assert!(document.effects().water().is_some());
    assert_eq!(document.routes().components().len(), 2);
    assert_eq!(document.routes().capabilities().len(), 2);
    assert_eq!(document.assets().bindings().len(), 7);
    assert_eq!(document.limits().max_mesh_components(), 16);
}

#[test]
fn wrong_schema_tag_is_rejected() {
    let mut contract = valid_contract();
    contract["schema"] = json!("example.other-document.v1");
    let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect_err("wrong schema family must fail");
    assert!(matches!(
        error,
        ContractDocumentError::SchemaTagMismatch { .. }
    ));
}

#[test]
fn future_schema_version_is_rejected() {
    let mut contract = valid_contract();
    contract["schema_version"] = json!(RENDER_CONTRACT_SCHEMA_VERSION + 1);
    let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect_err("unknown schema version must fail");
    assert!(matches!(
        error,
        ContractDocumentError::UnsupportedSchemaVersion { expected, observed }
            if expected == RENDER_CONTRACT_SCHEMA_VERSION && observed == expected + 1
    ));
}

#[test]
fn older_schema_version_is_rejected() {
    let mut contract = valid_contract();
    contract["schema_version"] = json!(0);
    let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect_err("older schema version must fail");
    assert!(matches!(
        error,
        ContractDocumentError::UnsupportedSchemaVersion { .. }
    ));
}

#[test]
fn unknown_root_field_is_rejected() {
    let mut contract = valid_contract();
    contract["surprise"] = json!(1);
    let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect_err("unknown root field must fail");
    assert!(matches!(error, ContractDocumentError::Json(_)));
}

#[test]
fn unknown_nested_field_is_rejected() {
    let mut contract = valid_contract();
    contract["terrain"]["surprise"] = json!(true);
    let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect_err("unknown nested field must fail");
    assert!(matches!(error, ContractDocumentError::Json(_)));
}

#[test]
fn missing_mandatory_part_is_rejected() {
    for part in [
        "limits",
        "assets",
        "routes",
        "effects",
        "environment",
        "terrain",
        "scene",
        "basis",
        "contract_id",
    ] {
        let mut contract = valid_contract();
        contract
            .as_object_mut()
            .expect("fixture is an object")
            .remove(part);
        let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
            .expect_err("missing mandatory part must fail");
        assert!(
            matches!(error, ContractDocumentError::Json(_)),
            "removing {part} must fail as strict JSON"
        );
    }
}

#[test]
fn missing_nested_mandatory_field_is_rejected() {
    let mut contract = valid_contract();
    contract["terrain"]
        .as_object_mut()
        .expect("terrain is an object")
        .remove("columns");
    let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect_err("missing nested mandatory field must fail");
    assert!(matches!(error, ContractDocumentError::Json(_)));
}

#[test]
fn malformed_json_is_rejected() {
    let error = ContractDocument::from_json_bytes(b"{").expect_err("truncated JSON must fail");
    assert!(matches!(error, ContractDocumentError::Json(_)));
}

#[test]
fn fractional_values_are_rejected_by_exact_scalar_types() {
    let mut contract = valid_contract();
    contract["scene"]["meshes"][0]["transform"]["scale"]["millionths"] = json!(1.5);
    let error = ContractDocument::from_json_bytes(&contract_bytes(&contract))
        .expect_err("fractional scale must fail");
    assert!(matches!(error, ContractDocumentError::Json(_)));
}

#[test]
fn revalidation_of_decoded_document_succeeds() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    assert!(document.validate().is_ok());
}
