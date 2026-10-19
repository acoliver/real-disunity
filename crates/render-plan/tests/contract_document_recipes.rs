//! Terrain, environment, effects, limits, and asset-closure recipe tests.

mod common;

use common::{contract_bytes, valid_contract};
use serde_json::json;
use worldrunner_render_plan::{ContractDocument, ContractDocumentError};

fn decode(contract: &serde_json::Value) -> Result<ContractDocument, ContractDocumentError> {
    ContractDocument::from_json_bytes(&contract_bytes(contract))
}

fn reject(contract: &serde_json::Value) -> ContractDocumentError {
    decode(contract).expect_err("mutated fixture must fail")
}

#[test]
fn terrain_rejects_degenerate_dimensions() {
    let mut contract = valid_contract();
    contract["terrain"]["columns"] = json!(1);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "terrain columns",
            ..
        }
    ));
    let mut contract = valid_contract();
    contract["terrain"]["rows"] = json!(1);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "terrain rows",
            ..
        }
    ));
}

#[test]
fn terrain_rejects_zero_spacing() {
    let mut contract = valid_contract();
    contract["terrain"]["sample_spacing_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "terrain sample_spacing_millimeters",
            ..
        }
    ));
}

#[test]
fn terrain_requires_a_sorted_nonempty_palette() {
    let mut contract = valid_contract();
    contract["terrain"]["material_palette"] = json!([]);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::EmptyCollection {
            field: "terrain material_palette"
        }
    ));

    let mut contract = valid_contract();
    let mut slot = contract["terrain"]["material_palette"][0].clone();
    slot["material_uri"] = json!("synthetic:element/terrain-material-beta");
    contract["terrain"]["material_palette"]
        .as_array_mut()
        .expect("palette")
        .push(slot);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::DuplicateKey {
            field: "terrain material_palette",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["terrain"]["material_palette"][0]["slot_index"] = json!(5);
    contract["terrain"]["material_palette"][0]["uv_scale_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "terrain uv_scale_millimeters",
            ..
        }
    ));
}

#[test]
fn terrain_sample_count_is_exact() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    assert_eq!(document.terrain().sample_count().expect("small grid"), 15);
}

#[test]
fn mesh_positions_are_not_confined_to_the_terrain_rectangle() {
    let mut inside = valid_contract();
    inside["scene"]["meshes"][0]["transform"]["position"]["x"] = json!(600);
    inside["scene"]["meshes"][0]["transform"]["position"]["z"] = json!(-1200);
    assert!(decode(&inside).is_ok());

    let mut outside = valid_contract();
    outside["scene"]["meshes"][0]["transform"]["position"]["x"] = json!(601);
    outside["scene"]["meshes"][0]["transform"]["position"]["z"] = json!(-1201);
    assert!(decode(&outside).is_ok());
}

#[test]
fn lod_policies_require_contiguous_increasing_levels() {
    let mut contract = valid_contract();
    contract["environment"]["lod_policies"][0]["levels"] = json!([]);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::EmptyCollection {
            field: "lod policy levels"
        }
    ));

    let mut contract = valid_contract();
    contract["environment"]["lod_policies"][0]["levels"][1]["level_index"] = json!(2);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ViolatedInvariant {
            field: "lod policy levels",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["environment"]["lod_policies"][0]["levels"][1]["max_distance_millimeters"] =
        json!(50_000);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ViolatedInvariant {
            field: "lod level max_distance_millimeters",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["environment"]["lod_policies"][0]["levels"][0]["max_distance_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "lod level max_distance_millimeters",
            ..
        }
    ));
}

#[test]
fn lod_mesh_uri_must_be_structural() {
    let mut contract = valid_contract();
    contract["environment"]["lod_policies"][0]["levels"][0]["mesh_uri"] = json!("lod-mesh");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::InvalidUri {
            field: "lod level mesh URI",
            ..
        }
    ));
}

#[test]
fn wind_parameters_are_range_bounded() {
    let mut contract = valid_contract();
    contract["environment"]["wind"]["amplitude_micro"] = json!(1_000_001);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "wind amplitude_micro",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["environment"]["wind"]["period_milliseconds"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "wind period_milliseconds",
            ..
        }
    ));
}

#[test]
fn water_geometry_requires_positive_extents_and_checked_corners() {
    let mut contract = valid_contract();
    contract["effects"]["water"]["east_extent_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "water plane east_extent_millimeters",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["water"]["north_extent_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "water plane north_extent_millimeters",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["water"]["east_min_millimeters"] = json!(i64::MAX);
    contract["effects"]["water"]["east_extent_millimeters"] = json!(u64::MAX);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ArithmeticOverflow { .. }
    ));
}

#[test]
fn optional_water_may_be_absent() {
    let mut contract = valid_contract();
    contract["effects"]
        .as_object_mut()
        .expect("effects")
        .remove("water");
    assert!(decode(&contract).is_ok());
}

#[test]
fn light_intensities_are_bounded() {
    let mut contract = valid_contract();
    contract["effects"]["sun"]["irradiance_micro"] = json!(3_550_000);
    assert!(decode(&contract).is_ok());

    let mut contract = valid_contract();
    contract["effects"]["sun"]["irradiance_micro"] = json!(10_000_001);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "sun irradiance_micro",
            ..
        }
    ));

    // Ambient stays a ratio, and stays unit-bounded.
    let mut contract = valid_contract();
    contract["effects"]["ambient"]["irradiance_micro"] = json!(1_000_001);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "ambient irradiance_micro",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["sun"]["direction"]["w"] = json!(999_999);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ViolatedInvariant {
            field: "rotation quaternion",
            ..
        }
    ));
}

#[test]
fn shadow_cascades_require_unique_increasing_extents() {
    let mut contract = valid_contract();
    contract["effects"]["shadows"]["cascades"] = json!([]);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::EmptyCollection {
            field: "shadow cascades"
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["shadows"]["blend_band_micro"] = json!(1_000_001);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "shadow blend_band_micro",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["shadows"]["cascades"][1]["extent_millimeters"] = json!(8_000);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ViolatedInvariant {
            field: "shadow cascade extent_millimeters",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["shadows"]["cascades"][0]["extent_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "shadow cascade extent_millimeters",
            ..
        }
    ));

    let mut contract = valid_contract();
    let cascade = contract["effects"]["shadows"]["cascades"][1].clone();
    contract["effects"]["shadows"]["cascades"]
        .as_array_mut()
        .expect("cascades")
        .push(cascade);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::DuplicateKey {
            field: "shadow cascades",
            ..
        }
    ));
}

#[test]
fn ssao_parameters_are_range_bounded() {
    let mut contract = valid_contract();
    contract["effects"]["ssao"]["radius_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "ssao radius_millimeters",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["ssao"]["intensity_micro"] = json!(2_000_001);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "ssao intensity_micro",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["ssao"]["horizon_angle_millidegrees"] = json!(180_001);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "ssao horizon_angle_millidegrees",
            ..
        }
    ));
}

#[test]
fn camera_intrinsics_enforce_open_fov_and_plane_order() {
    let mut contract = valid_contract();
    contract["effects"]["camera"]["vertical_fov_millidegrees"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "camera vertical_fov_millidegrees",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["camera"]["vertical_fov_millidegrees"] = json!(180_000);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "camera vertical_fov_millidegrees",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["camera"]["near_plane_millimeters"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "camera near_plane_millimeters",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["camera"]["far_plane_millimeters"] = json!(100);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::ViolatedInvariant {
            field: "camera plane gap",
            ..
        }
    ));
}

#[test]
fn mandatory_limits_must_be_positive_and_dominant() {
    let mut contract = valid_contract();
    contract["limits"]["max_mesh_components"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "limits max_mesh_components",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["limits"]["max_lod_levels_per_policy"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "limits max_lod_levels_per_policy",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["limits"]["stream"]["max_stream_bytes"] = json!(0);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "limits max_stream_bytes",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["limits"]["stream"]["max_total_bytes"] = json!(1024);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::OutOfRange {
            field: "limits max_stream_bytes",
            ..
        }
    ));
}

#[test]
fn authored_limits_govern_authored_content() {
    let mut contract = valid_contract();
    contract["limits"]["max_mesh_components"] = json!(1);
    let clone = contract["scene"]["meshes"][0].clone();
    contract["scene"]["meshes"]
        .as_array_mut()
        .expect("meshes")
        .push(clone);
    contract["scene"]["meshes"][1]["component_id"] = json!("mesh-beta");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::LimitExceeded {
            field: "scene meshes",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["limits"]["max_lod_levels_per_policy"] = json!(1);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::LimitExceeded {
            field: "lod policy levels",
            ..
        }
    ));
}

#[test]
fn former_binding_byte_length_is_unknown() {
    let mut contract = valid_contract();
    contract["assets"]["bindings"][0]["byte_length"] = json!(1024);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));

    let mut contract = valid_contract();
    contract["assets"]["bindings"][0]["byte_length"] = json!(0);
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));
}

#[test]
fn former_binding_capability_uri_is_unknown() {
    let mut contract = valid_contract();
    contract["assets"]["bindings"][0]["capability_uri"] = json!("synthetic:capability/bytes");
    assert!(matches!(reject(&contract), ContractDocumentError::Json(_)));
}

#[test]
fn zero_byte_authority_is_absent_from_the_asset_layer() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    let encoded = String::from_utf8(document.to_json_bytes().expect("document encodes"))
        .expect("document encodes as UTF-8");
    assert!(!encoded.contains("byte_length"));
    let round: serde_json::Value =
        serde_json::from_str(&encoded).expect("encoded document is JSON");
    let assets = round["assets"].to_string();
    assert!(!assets.contains("byte_length"));
    assert!(!assets.contains("capability_uri"));
    let bindings = round["assets"]["bindings"]
        .as_array()
        .expect("bindings survive the round-trip");
    for binding in bindings {
        let mut keys = binding
            .as_object()
            .expect("one binding is an object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(keys, ["asset_uri", "binding_id", "role_uri"]);
    }
}

#[test]
fn asset_resolution_maps_asset_uri_to_exact_binding_id() {
    let bytes = contract_bytes(&valid_contract());
    let document = ContractDocument::from_json_bytes(&bytes).expect("valid fixture must decode");
    let height = document
        .assets()
        .binding_for("synthetic:element/terrain-height")
        .expect("fixture declares the height binding");
    assert_eq!(height.binding_id(), "binding-0007");
    assert_eq!(height.asset_uri(), "synthetic:element/terrain-height");
    assert!(
        document
            .assets()
            .binding_for("synthetic:element/absent")
            .is_none()
    );
}

#[test]
fn empty_asset_policy_is_rejected() {
    let mut contract = valid_contract();
    contract["assets"]["bindings"] = json!([]);
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::EmptyCollection {
            field: "asset bindings"
        }
    ));
}

#[test]
fn undeclared_asset_uris_fail_closed() {
    let mut contract = valid_contract();
    contract["scene"]["meshes"][0]["asset_uri"] = json!("synthetic:element/mesh-undeclared");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnresolvedAssetUri {
            collection: "scene meshes",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["terrain"]["height_asset_uri"] = json!("synthetic:element/terrain-height-undeclared");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnresolvedAssetUri {
            collection: "terrain height",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["terrain"]["material_palette"][0]["material_uri"] =
        json!("synthetic:element/terrain-material-undeclared");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnresolvedAssetUri {
            collection: "terrain material_palette",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["sky"]["asset_uri"] = json!("synthetic:element/sky-undeclared");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnresolvedAssetUri {
            collection: "effects sky",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["effects"]["water"]["asset_uri"] = json!("synthetic:element/water-undeclared");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnresolvedAssetUri {
            collection: "effects water",
            ..
        }
    ));

    let mut contract = valid_contract();
    contract["environment"]["lod_policies"][0]["levels"][0]["mesh_uri"] =
        json!("synthetic:element/lod-undeclared");
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnresolvedAssetUri {
            collection: "environment lod_policies",
            ..
        }
    ));
}

#[test]
fn terrain_height_binding_is_part_of_asset_closure() {
    let mut contract = valid_contract();
    contract["assets"]["bindings"]
        .as_array_mut()
        .expect("bindings")
        .pop();
    assert!(matches!(
        reject(&contract),
        ContractDocumentError::UnresolvedAssetUri {
            collection: "terrain height",
            ..
        }
    ));
}
