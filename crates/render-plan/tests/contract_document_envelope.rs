//! Envelope, round-trip, and strictness tests for authored render contracts.

mod common;

use common::{contract_bytes, valid_contract};
use serde_json::json;
use worldrunner_render_plan::{
    ContractDocument, ContractDocumentError, RENDER_CONTRACT_SCHEMA, RENDER_CONTRACT_SCHEMA_VERSION,
};

#[test]
fn previous_contract_is_rejected_at_the_identity_boundary() {
    let bytes = include_bytes!("fixtures/render-contract-v3.json");
    assert!(matches!(
        ContractDocument::from_json_bytes(bytes),
        Err(ContractDocumentError::SchemaTagMismatch { expected, observed })
            if expected == RENDER_CONTRACT_SCHEMA && observed == "real-disunity.render-contract.v3"
    ));
    let mut value: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    value["schema"] = json!(RENDER_CONTRACT_SCHEMA);
    assert!(matches!(
        ContractDocument::from_json_bytes(&contract_bytes(&value)),
        Err(ContractDocumentError::UnsupportedSchemaVersion {
            expected: 4,
            observed: 3
        })
    ));
}

#[test]
fn current_contract_matches_the_golden_bytes() {
    let document = ContractDocument::from_json_bytes(&contract_bytes(&valid_contract())).unwrap();
    let golden = include_bytes!("fixtures/render-contract-v4.json");
    assert_eq!(document.to_json_bytes().unwrap(), golden);
    assert_eq!(ContractDocument::from_json_bytes(golden).unwrap(), document);
}

#[test]
fn current_contract_requires_each_new_ssao_field() {
    for field in ["sample_count", "bias_millimeters"] {
        let mut value = valid_contract();
        value["effects"]["ssao"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(matches!(
            ContractDocument::from_json_bytes(&contract_bytes(&value)),
            Err(ContractDocumentError::Json(error)) if error.to_string().contains(field)
        ));
    }
}
#[test]
fn contract_identity_has_an_independent_version_boundary() {
    assert_eq!(RENDER_CONTRACT_SCHEMA, "real-disunity.render-contract.v4");
    assert_eq!(RENDER_CONTRACT_SCHEMA_VERSION, 4);
    assert_eq!(
        worldrunner_render_plan::RENDER_CONTRACT_CAPABILITY_URI,
        "urn:real-disunity:capability:render-contract:v4"
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

#[test]
fn current_contract_requires_each_water_field() {
    for field in [
        "height_millimeters",
        "color_micro",
        "absorption_micro",
        "roughness_micro",
    ] {
        let mut value = valid_contract();
        value["effects"]["water"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            matches!(ContractDocument::from_json_bytes(&contract_bytes(&value)), Err(ContractDocumentError::Json(error)) if error.to_string().contains(field))
        );
    }
}

#[test]
fn optional_ssr_keeps_v4_identity_and_requires_its_complete_policy() {
    let policy = json!({"enabled":true,"max_steps":64,"thickness_millimeters":100,"roughness_cutoff_micro":600_000,"resolution_scale_micro":500_000});
    let mut contract = valid_contract();
    assert!(
        ContractDocument::from_json_bytes(&contract_bytes(&contract))
            .unwrap()
            .effects()
            .ssr()
            .is_none()
    );
    contract["effects"]["ssr"] = policy.clone();
    let document = ContractDocument::from_json_bytes(&contract_bytes(&contract)).unwrap();
    assert_eq!(document.header().schema_version, 4);
    assert_eq!(document.effects().ssr().unwrap().max_steps, 64);
    assert_eq!(
        ContractDocument::from_json_bytes(&document.to_json_bytes().unwrap()).unwrap(),
        document
    );
    for field in [
        "enabled",
        "max_steps",
        "thickness_millimeters",
        "roughness_cutoff_micro",
        "resolution_scale_micro",
    ] {
        let mut missing = contract.clone();
        missing["effects"]["ssr"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(ContractDocument::from_json_bytes(&contract_bytes(&missing)).is_err());
    }
    contract["effects"]["ssr"]["resolution_scale_micro"] = json!(750_000);
    assert!(ContractDocument::from_json_bytes(&contract_bytes(&contract)).is_err());
}

#[test]
fn optional_hdr_post_keeps_v4_and_requires_complete_valid_settings() {
    let mut contract = valid_contract();
    assert!(
        ContractDocument::from_json_bytes(&contract_bytes(&contract))
            .unwrap()
            .effects()
            .hdr_post()
            .is_none()
    );
    let policy = json!({
        "adaptive_exposure": {
            "log_min_milli": -12_000, "log_max_milli": 12_000,
            "low_percentile_micro": 10_000, "high_percentile_micro": 990_000,
            "key_micro": 180_000, "min_exposure_micro": 10_000, "max_exposure_micro": 64_000_000,
            "speed_up_milli": 3000, "speed_down_milli": 1000,
            "fixed_delta_microseconds": 16_667, "fixed_exposure_micro": null
        },
        "bloom": {"threshold_micro": 1_000_000, "soft_knee_micro": 500_000, "intensity_micro": 200_000, "levels": 5},
        "fxaa_subpixel_micro": 750_000
    });
    contract["effects"]["hdr_post"] = policy.clone();
    let document = ContractDocument::from_json_bytes(&contract_bytes(&contract)).unwrap();
    assert_eq!(document.header().schema_version, 4);
    assert_eq!(
        ContractDocument::from_json_bytes(&document.to_json_bytes().unwrap()).unwrap(),
        document
    );
    for field in ["adaptive_exposure", "bloom", "fxaa_subpixel_micro"] {
        let mut missing = contract.clone();
        missing["effects"]["hdr_post"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(ContractDocument::from_json_bytes(&contract_bytes(&missing)).is_err());
    }
    for (section, field, invalid) in [
        ("adaptive_exposure", "log_max_milli", -12_000),
        ("adaptive_exposure", "high_percentile_micro", 10_000),
        ("adaptive_exposure", "speed_down_milli", 0),
        ("bloom", "levels", 9),
        ("bloom", "soft_knee_micro", 1_000_001),
    ] {
        let mut invalid_contract = contract.clone();
        invalid_contract["effects"]["hdr_post"][section][field] = json!(invalid);
        assert!(ContractDocument::from_json_bytes(&contract_bytes(&invalid_contract)).is_err());
    }
}
