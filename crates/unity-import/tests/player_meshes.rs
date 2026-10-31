//! `Mesh` assets that `MeshFilter` components of prefabs reference: each distinct mesh plans once
//! with its channels, sub-meshes and bounds, and its vertex and index data in a resource file.

use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::player_resources::{ResourcesPlan, plan_player_resources};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

#[path = "support/mesh_bytes.rs"]
mod mesh_bytes;
use mesh_bytes::{Channel, MeshSpec, mesh_bytes};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components).unwrap().u32(0).string(name).unwrap();
    w.u16(0).u8(1);
    w.finish()
}

fn transform(game_object: i64, father: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[0.0; 3]).floats(&[1.0; 3]);
    w.pptrs(&[]).unwrap().pptr(father);
    w.finish()
}

fn filter_bytes(owner: i64, mesh: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner)).pptr(mesh);
    w.finish()
}

const TYPES: [ClassId; 5] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MESH_FILTER,
    ClassId::RESOURCE_MANAGER,
    ClassId::MESH,
];

fn object(path_id: i64, class_id: ClassId, data: Vec<u8>) -> ObjectData {
    let type_index = TYPES.iter().position(|&c| c == class_id).unwrap();
    ObjectData {
        path_id,
        type_index: i32::try_from(type_index).unwrap(),
        data,
    }
}

fn write_file(dir: &Path, name: &str, objects: Vec<ObjectData>, externals: &[&str]) {
    let class = |class_id: ClassId| SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: None,
        old_type_hash: [0; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    };
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects,
        scripts: Vec::new(),
        externals: externals
            .iter()
            .map(|path| External {
                guid: [0; 16],
                kind: 0,
                path: (*path).into(),
            })
            .collect(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join(name), contents.write().unwrap()).unwrap();
}

fn f32s(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Four vertices in two streams: stream 0 holds 32-bit float positions and normals (stride 24),
/// stream 1 holds 16-bit float UVs and 8-bit colors (stride 8) and starts at the next 16-byte
/// boundary, byte 96.
fn two_stream_vertices() -> Vec<u8> {
    let positions = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
    ];
    let uv_halves: [[u16; 2]; 4] = [[0, 0], [0x3C00, 0], [0, 0x3C00], [0x3C00, 0x3C00]];
    let colors: [[u8; 4]; 4] = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 0],
    ];
    let mut data = Vec::new();
    for position in positions {
        data.extend(f32s(&position));
        data.extend(f32s(&[0.0, 0.0, 1.0]));
    }
    for vertex in 0..4 {
        data.extend(uv_halves[vertex].iter().flat_map(|h| h.to_le_bytes()));
        data.extend(colors[vertex]);
    }
    data
}

fn two_stream_channels() -> Vec<Channel> {
    let mut channels = vec![[0, 0, 0, 0]; 14];
    channels[0] = [0, 0, 0, 3];
    channels[1] = [0, 12, 0, 3];
    channels[3] = [1, 4, 2, 4];
    channels[4] = [1, 0, 1, 2];
    channels
}

fn two_sub_meshes() -> MeshSpec {
    let indices: Vec<u8> = [0u16, 1, 2, 2, 1, 3]
        .iter()
        .flat_map(|i| i.to_le_bytes())
        .collect();
    MeshSpec {
        name: "Hex",
        sub_meshes: vec![[0, 3, 0, 0, 0, 4], [6, 3, 0, 0, 0, 4]],
        compression: 0,
        index_format: 0,
        indices,
        vertex_count: 4,
        channels: two_stream_channels(),
        vertex_data: two_stream_vertices(),
        stream: (0, 0, ""),
    }
}

fn resource_manager(prefabs: &[(&str, i64)]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(i32::try_from(prefabs.len()).unwrap());
    for (path, id) in prefabs {
        w.string(path).unwrap().pptr(ptr(1, *id));
    }
    w.i32(0);
    w.finish()
}

/// Prefabs `rock` (game object 2) and `twin` (game object 5), each with a filter that names the
/// mesh object 10.
fn prefab_objects(mesh: Vec<u8>) -> Vec<ObjectData> {
    vec![
        object(
            2,
            ClassId::GAME_OBJECT,
            game_object("Rock", &[ptr(0, 3), ptr(0, 4)]),
        ),
        object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0))),
        object(4, ClassId::MESH_FILTER, filter_bytes(2, ptr(0, 10))),
        object(
            5,
            ClassId::GAME_OBJECT,
            game_object("Twin", &[ptr(0, 6), ptr(0, 7)]),
        ),
        object(6, ClassId::TRANSFORM, transform(5, ptr(0, 0))),
        object(7, ClassId::MESH_FILTER, filter_bytes(5, ptr(0, 10))),
        object(10, ClassId::MESH, mesh),
    ]
}

fn plan_prefabs(dir: &Path, mesh: Vec<u8>) -> ResourcesPlan {
    let managers = resource_manager(&[("prefabs/rock", 2), ("prefabs/twin", 5)]);
    write_file(
        dir,
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, managers)],
        &["resources.assets"],
    );
    write_file(dir, "resources.assets", prefab_objects(mesh), &[]);
    plan_player_resources(dir, &ScriptLayouts::new(Vec::new()), &dir.join("out")).unwrap()
}

fn prefab_filter(dir: &Path, plan: &ResourcesPlan, asset: usize) -> Json {
    let file = serde_json::to_value(&plan.assets[asset]).unwrap()["plan"]
        .as_str()
        .unwrap()
        .to_owned();
    let scene: Json =
        serde_json::from_slice(&std::fs::read(dir.join("out").join(file)).unwrap()).unwrap();
    scene["objects"][0]["components"][0].clone()
}

fn mesh_assets(plan: &ResourcesPlan) -> Vec<Json> {
    plan.assets
        .iter()
        .map(|asset| serde_json::to_value(asset).unwrap())
        .filter(|asset| asset["type"] == "Mesh" || asset["type"] == "UnplannedMesh")
        .collect()
}

#[test]
fn a_two_stream_mesh_plans_its_channels_sub_meshes_and_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_prefabs(dir.path(), mesh_bytes(&two_sub_meshes()));
    let [mesh] = &mesh_assets(&plan)[..] else {
        panic!("one mesh: {:?}", plan.assets)
    };
    let triangles = |first: u32| {
        json!({"first_index": first, "index_count": 3, "topology": 0, "base_vertex": 0,
               "first_vertex": 0, "vertex_count": 4})
    };
    assert_eq!(mesh["name"], "Hex");
    assert_eq!(mesh["vertex_count"], 4);
    assert_eq!(mesh["index_format"], 0);
    assert_eq!(mesh["index_count"], 6);
    assert_eq!(
        mesh["channels"],
        json!([
            {"name": "position", "dimension": 3},
            {"name": "normal", "dimension": 3},
            {"name": "color", "dimension": 4},
            {"name": "uv0", "dimension": 2}
        ])
    );
    assert_eq!(mesh["sub_meshes"], json!([triangles(0), triangles(3)]));
    assert_eq!(
        mesh["bounds"],
        json!({"center": [0.5, 0.5, 0.0], "extent": [0.5, 0.5, 0.0]})
    );
}

#[test]
fn the_resource_holds_each_attribute_as_floats_then_32_bit_indices() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_prefabs(dir.path(), mesh_bytes(&two_sub_meshes()));
    let mesh = &mesh_assets(&plan)[0];
    let bytes = std::fs::read(
        dir.path()
            .join("out")
            .join(mesh["resource"].as_str().unwrap()),
    )
    .unwrap();
    let mut expected = f32s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0]);
    expected.extend(f32s(&[
        0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
    ]));
    expected.extend(f32s(&[1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]));
    expected.extend(f32s(&[0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0]));
    expected.extend(f32s(&[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0]));
    expected.extend(u32s(&[0, 1, 2, 2, 1, 3]));
    assert_eq!(bytes, expected);
}

#[test]
fn two_filters_refer_to_one_planned_mesh() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_prefabs(dir.path(), mesh_bytes(&two_sub_meshes()));
    assert_eq!(mesh_assets(&plan).len(), 1);
    let mesh_index = plan
        .assets
        .iter()
        .position(|a| serde_json::to_value(a).unwrap()["type"] == "Mesh")
        .unwrap();
    for prefab in [0, 1] {
        let filter = prefab_filter(dir.path(), &plan, prefab);
        assert_eq!(filter["type"], "MeshFilter");
        assert_eq!(filter["mesh"], json!(mesh_index));
    }
    assert_eq!(plan.differences, Vec::<String>::new());
}

#[test]
fn a_streamed_mesh_reads_its_vertices_from_the_res_s_file() {
    let dir = tempfile::tempdir().unwrap();
    let vertices = two_stream_vertices();
    let mut file = vec![0xEE; 16];
    file.extend(&vertices);
    std::fs::write(dir.path().join("resources.assets.resS"), file).unwrap();
    let mut spec = two_sub_meshes();
    spec.vertex_data = Vec::new();
    spec.stream = (
        16,
        u32::try_from(vertices.len()).unwrap(),
        "archive:/CAB-1/resources.assets.resS",
    );
    let plan = plan_prefabs(dir.path(), mesh_bytes(&spec));
    let inline = tempfile::tempdir().unwrap();
    let reference = plan_prefabs(inline.path(), mesh_bytes(&two_sub_meshes()));
    let read = |root: &Path, plan: &ResourcesPlan| {
        let mesh = &mesh_assets(plan)[0];
        std::fs::read(root.join("out").join(mesh["resource"].as_str().unwrap())).unwrap()
    };
    assert_eq!(read(dir.path(), &plan), read(inline.path(), &reference));
    assert_eq!(mesh_assets(&plan)[0]["vertex_count"], 4);
}

#[test]
fn a_compressed_mesh_is_left_out_and_listed() {
    let dir = tempfile::tempdir().unwrap();
    let mut spec = two_sub_meshes();
    spec.compression = 2;
    let plan = plan_prefabs(dir.path(), mesh_bytes(&spec));
    assert_eq!(mesh_assets(&plan), Vec::<Json>::new());
    assert_eq!(prefab_filter(dir.path(), &plan, 0)["mesh"], Json::Null);
    let listed = plan.differences.join("\n");
    assert!(
        listed.contains("resource mesh Hex: compressed meshes are not decoded yet"),
        "{listed}"
    );
    assert!(
        listed.contains("prefab Rock: Rock: MeshFilter mesh references object 10"),
        "{listed}"
    );
}

#[test]
fn an_unsupported_vertex_format_is_left_out_and_listed() {
    let dir = tempfile::tempdir().unwrap();
    let mut spec = two_sub_meshes();
    spec.channels[0] = [0, 0, 12, 3];
    let plan = plan_prefabs(dir.path(), mesh_bytes(&spec));
    assert_eq!(mesh_assets(&plan), Vec::<Json>::new());
    let listed = plan.differences.join("\n");
    assert!(
        listed.contains(
            "resource mesh Hex: vertex channel position has format 12, which is not decoded"
        ),
        "{listed}"
    );
}

/// Three vertices: stream 0 holds 32-bit float positions (stride 12) and ends at byte 36, so
/// stream 1 starts at the next 16-byte boundary, byte 48, with three-component 8-bit colors.
fn padded_stream_mesh() -> MeshSpec {
    let mut vertex_data = f32s(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0]);
    vertex_data.extend([0xAA; 12]);
    vertex_data.extend([255, 0, 0, 0, 255, 0, 0, 0, 255]);
    let mut channels = vec![[0, 0, 0, 0]; 14];
    channels[0] = [0, 0, 0, 3];
    channels[3] = [1, 0, 2, 3];
    MeshSpec {
        name: "Padded",
        sub_meshes: vec![[0, 3, 0, 0, 0, 3]],
        compression: 0,
        index_format: 0,
        indices: [0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()).collect(),
        vertex_count: 3,
        channels,
        vertex_data,
        stream: (0, 0, ""),
    }
}

#[test]
fn a_later_stream_starts_on_a_16_byte_boundary_and_color_alpha_defaults_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_prefabs(dir.path(), mesh_bytes(&padded_stream_mesh()));
    let mesh = &mesh_assets(&plan)[0];
    let bytes = std::fs::read(
        dir.path()
            .join("out")
            .join(mesh["resource"].as_str().unwrap()),
    )
    .unwrap();
    let mut expected = f32s(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 2.0, 0.0]);
    expected.extend(f32s(&[
        1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0,
    ]));
    expected.extend(u32s(&[0, 1, 2]));
    assert_eq!(bytes, expected);
}
