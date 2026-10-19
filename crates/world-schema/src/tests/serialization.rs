use crate::{
    CatalogBinding, EnvironmentInstance, EnvironmentOrientation, WorldCoordinateFrame,
    WorldDocument,
};
use serde::{
    Deserialize,
    de::{
        DeserializeOwned,
        value::{Error, MapDeserializer, StrDeserializer},
    },
};

fn assert_unknown_field<T: DeserializeOwned>(field: &str) {
    let input = MapDeserializer::<_, Error>::new([(field, "discarded")].into_iter());
    let Err(error) = T::deserialize(input) else {
        panic!("unexpectedly accepted removed field {field}");
    };
    assert!(
        error
            .to_string()
            .contains(&format!("unknown field `{field}`")),
        "{error}"
    );
}

#[test]
fn removed_world_inventory_and_probe_fields_are_rejected_not_ignored() {
    for field in [
        "terrain_kind",
        "native_terrain",
        "import_provenance",
        "environment_profile",
        "water",
    ] {
        assert_unknown_field::<WorldDocument>(field);
    }
}

#[test]
fn source_phase_and_readiness_policy_is_not_an_instance_contract() {
    for field in [
        "lifecycle",
        "source_record_id",
        "asset_readiness",
        "color_modifier_percent",
    ] {
        assert_unknown_field::<EnvironmentInstance>(field);
    }
}

#[test]
fn source_rotation_and_frame_encodings_are_rejected() {
    for field in [
        "axis_signed",
        "angle",
        "AxisAngleSixtiethDegrees",
        "YawMicroradians",
    ] {
        assert_unknown_field::<EnvironmentOrientation>(field);
    }
    for frame in ["NativeXyZUp", "EngineYUp"] {
        assert!(WorldCoordinateFrame::deserialize(StrDeserializer::<Error>::new(frame)).is_err());
    }
}

#[test]
fn empty_catalog_binding_cannot_invent_content_preferences() {
    let input = MapDeserializer::<_, Error>::new(std::iter::empty::<(&str, &str)>());
    let error = CatalogBinding::deserialize(input).unwrap_err();
    assert!(error.to_string().contains("missing field `version`"));
}
