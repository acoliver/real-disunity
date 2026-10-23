//! Shared synthetic fixtures for authored render-contract document tests.
//!
//! The fixture is one minimal fully closed contract; failure tests mutate
//! one part at a time so every rejection is attributable to one invariant.

use serde_json::{Value, json};

/// One valid asset binding with an incrementally ordered binding id.
pub fn asset_binding(id: &str, asset_uri: &str, role_uri: &str) -> Value {
    json!({
        "binding_id": id,
        "asset_uri": asset_uri,
        "role_uri": role_uri,
    })
}

/// One valid mesh component placed at east `x` and north `z`.
pub fn mesh_component(id: &str, asset_uri: &str, x: i64, z: i64) -> Value {
    json!({
        "component_id": id,
        "asset_uri": asset_uri,
        "transform": {
            "position": { "x": x, "y": 0, "z": z },
            "rotation": { "x": 0, "y": 0, "z": 0, "w": 1_000_000 },
            "scale": { "millionths": 1_000_000 },
        },
    })
}

/// One valid LOD level.
pub fn lod_level(index: u8, distance: u64, asset_uri: &str) -> Value {
    json!({
        "level_index": index,
        "max_distance_millimeters": distance,
        "mesh_uri": asset_uri,
    })
}

fn basis() -> Value {
    json!({
        "handedness": "right_handed",
        "up_axis": "positive_y",
        "forward_axis": "positive_z",
        "unit_millimeters": 1000,
    })
}

fn terrain() -> Value {
    json!({
        "height_asset_uri": "synthetic:element/terrain-height",
        "columns": 3,
        "rows": 5,
        "sample_spacing_millimeters": 250,
        "origin_east_millimeters": -250,
        "origin_north_millimeters": -500,
        "material_palette": [{
            "slot_index": 0,
            "material_uri": "synthetic:element/terrain-material",
            "uv_scale_millimeters": 1000,
        }],
    })
}

fn environment() -> Value {
    json!({
        "lod_policies": [{
            "lod_id": "vegetation-alpha",
            "levels": [
                lod_level(0, 50_000, "synthetic:element/lod-mesh-near"),
                lod_level(1, 200_000, "synthetic:element/lod-mesh-far"),
            ],
        }],
        "wind": {
            "direction_microradians": 0,
            "amplitude_micro": 75_000,
            "period_milliseconds": 1800,
        },
    })
}

fn effects() -> Value {
    json!({
        "water": { "height_millimeters": 0, "color_micro": [50000,200_000,250_000], "absorption_micro": [300_000,100_000,50000], "roughness_micro": 200_000,
            "asset_uri": "synthetic:element/water-surface",
            "east_min_millimeters": -1000,
            "north_min_millimeters": -1000,
            "east_extent_millimeters": 2000,
            "north_extent_millimeters": 2000,
        },
        "sky": { "asset_uri": "synthetic:element/sky-dome" },
        "sun": {
            "direction": [0, 0, 1_000_000],
            "irradiance_micro": 1_250_000,
        },
        "ambient": { "irradiance_micro": 300_000 },
        "shadows": {
            "cascades": [
                { "cascade_index": 0, "extent_millimeters": 8_000 },
                { "cascade_index": 1, "extent_millimeters": 32_000 },
            ],
            "blend_band_micro": 10_000,
        },
        "ssao": {
            "sample_count": 16,
            "bias_millimeters": 10,
            "radius_millimeters": 750,
            "intensity_micro": 1_000_000,
            "horizon_angle_millidegrees": 90_000,
        },
        "camera": {
            "vertical_fov_millidegrees": 60_000,
            "near_plane_millimeters": 100,
            "far_plane_millimeters": 2_000_000,
        },
    })
}

fn routes() -> Value {
    json!({
        "document": {
            "schema_uri": "synthetic:schema/document-alpha",
            "schema_version": 1,
            "handler_factory_uri": "synthetic:factory/document-handler",
            "handler_id": "synthetic-document-handler",
            "handler_revision": 3,
            "payload_contract_uri": "synthetic:payload/document-payload",
            "role_uri": "synthetic:role/document",
        },
        "components": [
            {
                "schema_uri": "synthetic:schema/component-mesh",
                "handler_factory_uri": "synthetic:factory/mesh-handler",
                "handler_id": "synthetic-mesh-handler",
                "handler_revision": 2,
                "payload_contract_uri": "synthetic:payload/mesh-payload",
                "role_uri": "synthetic:role/mesh",
                "cardinality": { "minimum": 0, "maximum": 16 },
            },
            {
                "schema_uri": "synthetic:schema/component-water",
                "handler_factory_uri": "synthetic:factory/water-handler",
                "handler_id": "synthetic-water-handler",
                "handler_revision": 0,
                "payload_contract_uri": "synthetic:payload/water-payload",
                "role_uri": "synthetic:role/water",
                "cardinality": { "minimum": 0, "maximum": 1 },
            },
        ],
        "capabilities": [
            {
                "capability_uri": "synthetic:capability/mesh-artifact",
                "handler_factory_uri": "synthetic:factory/mesh-handler",
                "handler_id": "synthetic-mesh-handler",
                "handler_revision": 2,
                "payload_contract_uri": "synthetic:payload/mesh-payload",
                "role_uri": "synthetic:role/mesh",
                "cardinality": { "minimum": 1, "maximum": 16 },
                "element_policy": { "kind": "bare" },
            },
            {
                "capability_uri": "synthetic:capability/terrain-height",
                "handler_factory_uri": "synthetic:factory/height-handler",
                "handler_id": "synthetic-height-handler",
                "handler_revision": 1,
                "payload_contract_uri": "synthetic:payload/height-payload",
                "role_uri": "synthetic:role/terrain",
                "cardinality": { "minimum": 1, "maximum": 1 },
                "element_policy": {
                    "kind": "exact",
                    "element_uri": "synthetic:element/terrain-height",
                },
            },
        ],
    })
}

fn assets() -> Value {
    json!({
        "bindings": [
            asset_binding("binding-0001", "synthetic:element/mesh-alpha", "synthetic:role/mesh"),
            asset_binding("binding-0002", "synthetic:element/terrain-material", "synthetic:role/terrain"),
            asset_binding("binding-0003", "synthetic:element/lod-mesh-near", "synthetic:role/vegetation"),
            asset_binding("binding-0004", "synthetic:element/lod-mesh-far", "synthetic:role/vegetation"),
            asset_binding("binding-0005", "synthetic:element/water-surface", "synthetic:role/water"),
            asset_binding("binding-0006", "synthetic:element/sky-dome", "synthetic:role/sky"),
            asset_binding("binding-0007", "synthetic:element/terrain-height", "synthetic:role/terrain"),
        ],
    })
}

/// One minimal fully valid authored render contract.
pub fn valid_contract() -> Value {
    json!({
        "schema": "real-disunity.render-contract.v4",
        "schema_version": 4,
        "contract_id": "synthetic-contract-alpha",
        "basis": basis(),
        "scene": { "meshes": [
            mesh_component("mesh-alpha", "synthetic:element/mesh-alpha", 0, 0),
        ]},
        "terrain": terrain(),
        "environment": environment(),
        "effects": effects(),
        "routes": routes(),
        "assets": assets(),
        "limits": {
            "max_mesh_components": 16,
            "max_lod_levels_per_policy": 4,
            "stream": {
                "max_stream_bytes": 1_048_576,
                "max_total_bytes": 8_388_608,
            },
        },
    })
}

/// Encodes one fixture to bytes for document decoding.
pub fn contract_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("synthetic fixture serializes")
}
