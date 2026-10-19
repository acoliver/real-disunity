//! Identifier, ordering, coordinate, transform, and checked-arithmetic tests.

mod common;

use common::{contract_bytes, valid_contract};
use serde_json::json;
use worldrunner_render_plan::{
    AxisDirection, ContractDocumentError, CoordinateBasis, Handedness, MicroFixed,
    PositionMillimeters, RotationQuaternion, TerrainRecipe, TransformMillimeters,
    UniformScaleMillionths,
};

fn decode(
    contract: &serde_json::Value,
) -> Result<worldrunner_render_plan::ContractDocument, ContractDocumentError> {
    worldrunner_render_plan::ContractDocument::from_json_bytes(&contract_bytes(contract))
}

fn mesh_at(index: usize) -> serde_json::Value {
    valid_contract()["scene"]["meshes"][index].clone()
}

#[test]
fn empty_component_identifier_is_rejected() {
    let mut contract = valid_contract();
    contract["scene"]["meshes"][0]["component_id"] = json!("");
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::InvalidIdentifier { .. })
    ));
}

#[test]
fn control_characters_in_contract_id_are_rejected() {
    let mut contract = valid_contract();
    contract["contract_id"] = json!("synthetic\u{0}contract");
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::InvalidIdentifier { .. })
    ));
}

#[test]
fn schemeless_asset_uri_is_rejected() {
    let mut contract = valid_contract();
    contract["scene"]["meshes"][0]["asset_uri"] = json!("no-scheme-mesh");
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::InvalidUri { .. })
    ));
}

#[test]
fn whitespace_inside_uri_is_rejected() {
    let mut contract = valid_contract();
    contract["terrain"]["height_asset_uri"] = json!("synthetic: terrain-height");
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::InvalidUri { .. })
    ));
}

#[test]
fn unsorted_mesh_components_are_rejected() {
    let mut contract = valid_contract();
    contract["scene"]["meshes"]
        .as_array_mut()
        .expect("meshes")
        .push(mesh_at(0));
    contract["scene"]["meshes"][1]["component_id"] = json!("mesh-aaa");
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::UnsortedEntries {
            field: "scene meshes"
        })
    ));
}

#[test]
fn duplicate_mesh_component_ids_are_rejected() {
    let mut contract = valid_contract();
    contract["scene"]["meshes"]
        .as_array_mut()
        .expect("meshes")
        .push(mesh_at(0));
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::DuplicateKey {
            field: "scene meshes",
            ..
        })
    ));
}

#[test]
fn unsorted_lod_policies_are_rejected() {
    let mut contract = valid_contract();
    let mut policy = contract["environment"]["lod_policies"][0].clone();
    policy["lod_id"] = json!("vegetation-aaa");
    contract["environment"]["lod_policies"]
        .as_array_mut()
        .expect("lod policies")
        .push(policy);
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::UnsortedEntries {
            field: "environment lod_policies"
        })
    ));
}

#[test]
fn duplicate_binding_ids_are_rejected() {
    let mut contract = valid_contract();
    let mut binding = contract["assets"]["bindings"]
        .as_array()
        .and_then(|bindings| bindings.last())
        .expect("last binding")
        .clone();
    binding["asset_uri"] = json!("synthetic:element/another-mesh");
    contract["assets"]["bindings"]
        .as_array_mut()
        .expect("bindings")
        .push(binding);
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::DuplicateKey {
            field: "asset bindings",
            ..
        })
    ));
}

#[test]
fn duplicate_asset_uris_across_bindings_are_rejected() {
    let mut contract = valid_contract();
    let mut binding = contract["assets"]["bindings"][0].clone();
    binding["binding_id"] = json!("binding-0008");
    contract["assets"]["bindings"]
        .as_array_mut()
        .expect("bindings")
        .push(binding);
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::DuplicateKey {
            field: "asset bindings",
            ..
        })
    ));
}

#[test]
fn unknown_handedness_variant_is_rejected() {
    let mut contract = valid_contract();
    contract["basis"]["handedness"] = json!("back_handed");
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::Json(_))
    ));
}

#[test]
fn basis_rejects_equal_axes() {
    let basis = CoordinateBasis {
        handedness: Handedness::RightHanded,
        up_axis: AxisDirection::PositiveY,
        forward_axis: AxisDirection::PositiveY,
        unit_millimeters: 1000,
    };
    assert!(matches!(
        basis.validate(),
        Err(ContractDocumentError::ViolatedInvariant {
            field: "basis up_axis/forward_axis",
            ..
        })
    ));
}

#[test]
fn basis_rejects_opposite_axes() {
    let basis = CoordinateBasis {
        handedness: Handedness::LeftHanded,
        up_axis: AxisDirection::PositiveY,
        forward_axis: AxisDirection::NegativeY,
        unit_millimeters: 1000,
    };
    assert!(basis.validate().is_err());
}

#[test]
fn basis_rejects_zero_unit() {
    let basis = CoordinateBasis {
        handedness: Handedness::RightHanded,
        up_axis: AxisDirection::PositiveY,
        forward_axis: AxisDirection::PositiveZ,
        unit_millimeters: 0,
    };
    assert!(matches!(
        basis.validate(),
        Err(ContractDocumentError::OutOfRange {
            field: "basis unit_millimeters",
            ..
        })
    ));
}

#[test]
fn axis_opposition_is_exact() {
    assert!(AxisDirection::PositiveX.is_opposite(AxisDirection::NegativeX));
    assert!(!AxisDirection::PositiveX.is_opposite(AxisDirection::PositiveX));
    assert!(!AxisDirection::PositiveX.is_opposite(AxisDirection::NegativeZ));
}

#[test]
fn identity_rotation_is_valid_and_drift_is_rejected() {
    assert!(RotationQuaternion::IDENTITY.validate().is_ok());
    let drifted = RotationQuaternion {
        x: 0,
        y: 0,
        z: 0,
        w: 999_999,
    };
    assert!(matches!(
        drifted.validate(),
        Err(ContractDocumentError::ViolatedInvariant {
            field: "rotation quaternion",
            ..
        })
    ));
    let axis = RotationQuaternion {
        x: 1_000_000,
        y: 0,
        z: 0,
        w: 0,
    };
    assert!(axis.validate().is_ok());
}

#[test]
fn zero_scale_is_rejected() {
    let scale = UniformScaleMillionths::from_raw(0);
    assert!(matches!(
        scale.validate(),
        Err(ContractDocumentError::OutOfRange {
            field: "uniform scale millionths",
            ..
        })
    ));
    let mut contract = valid_contract();
    contract["scene"]["meshes"][0]["transform"]["scale"]["millionths"] = json!(0);
    assert!(matches!(
        decode(&contract),
        Err(ContractDocumentError::OutOfRange {
            field: "uniform scale millionths",
            ..
        })
    ));
}

#[test]
fn micro_scalar_arithmetic_is_exact_and_checked() {
    let half = MicroFixed::from_raw(500_000);
    let quarter = MicroFixed::from_raw(250_000);
    assert_eq!(
        half.checked_add(quarter).expect("within range"),
        MicroFixed::from_raw(750_000)
    );
    assert_eq!(half.checked_sub(quarter).expect("within range"), quarter);
    assert_eq!(
        half.checked_mul(half).expect("within range"),
        MicroFixed::from_raw(250_000)
    );
    // Deterministic truncation toward zero: 3 * 0.333333 => 0.999999 => 0.
    assert_eq!(
        MicroFixed::from_raw(3)
            .checked_mul(MicroFixed::from_raw(333_333))
            .expect("within range"),
        MicroFixed::ZERO
    );
    assert!(
        MicroFixed::from_raw(i64::MAX)
            .checked_add(MicroFixed::ONE)
            .is_err()
    );
    assert!(
        MicroFixed::from_raw(i64::MIN)
            .checked_sub(MicroFixed::ONE)
            .is_err()
    );
    assert!(
        MicroFixed::from_raw(i64::MAX)
            .checked_mul(MicroFixed::from_raw(i64::MAX))
            .is_err()
    );
    assert!(MicroFixed::ONE.is_unit_interval());
    assert!(!MicroFixed::from_raw(MicroFixed::ONE.raw() + 1).is_unit_interval());
}

#[test]
fn scale_composition_is_exact_and_checked() {
    let scale = UniformScaleMillionths::from_raw(1_500_000);
    assert_eq!(
        scale
            .checked_mul(UniformScaleMillionths::from_raw(2_000_000))
            .expect("within range"),
        UniformScaleMillionths::from_raw(3_000_000)
    );
    assert!(
        UniformScaleMillionths::from_raw(u64::MAX)
            .checked_mul(UniformScaleMillionths::from_raw(u64::MAX))
            .is_err()
    );
}

#[test]
fn position_addition_is_checked() {
    let base = PositionMillimeters::new(i64::MAX, 0, 0);
    let step = PositionMillimeters::new(1, 0, 0);
    assert!(base.checked_add(step, "test position").is_err());
    let near = PositionMillimeters::new(i64::MAX - 1, 1, -1);
    assert_eq!(
        near.checked_add(step, "test position")
            .expect("within range"),
        PositionMillimeters::new(i64::MAX, 1, -1)
    );
}

#[test]
fn terrain_extent_arithmetic_is_checked() {
    let terrain = TerrainRecipe {
        height_asset_uri: "synthetic:element/terrain-height".to_owned(),
        columns: u32::MAX,
        rows: u32::MAX,
        sample_spacing_millimeters: u32::MAX,
        origin_east_millimeters: i64::MAX,
        origin_north_millimeters: 0,
        material_palette: Vec::new(),
    };
    assert!(terrain.sample_count().is_ok());
    assert!(matches!(
        terrain.east_max_millimeters(),
        Err(ContractDocumentError::ArithmeticOverflow { .. })
    ));
    assert!(matches!(
        terrain.north_max_millimeters(),
        Err(ContractDocumentError::ArithmeticOverflow { .. })
    ));
}

#[test]
fn typed_construction_validates_as_strictly_as_decoding() {
    let terrain = TerrainRecipe {
        height_asset_uri: "synthetic:element/terrain-height".to_owned(),
        columns: 4,
        rows: 4,
        sample_spacing_millimeters: 600,
        origin_east_millimeters: -1200,
        origin_north_millimeters: -1200,
        material_palette: Vec::new(),
    };
    assert!(matches!(
        terrain.validate(),
        Err(ContractDocumentError::EmptyCollection { .. })
    ));
    let identity = RotationQuaternion::IDENTITY;
    assert!(
        TransformMillimeters {
            position: PositionMillimeters::new(0, 0, 0),
            rotation: identity,
            scale: UniformScaleMillionths::ONE,
        }
        .validate()
        .is_ok()
    );
}
