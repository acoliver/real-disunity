//! Skinned mesh renderers of prefabs: the renderer plans its mesh, bone transforms and root bone,
//! and the mesh plans its blend weights, blend indices and bind poses.

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
#[path = "support/renderer_bytes.rs"]
mod renderer_bytes;
use mesh_bytes::{MeshSpec, SkinSpec, mesh_bytes};
use renderer_bytes::{
    Rig, Settings, distinct, filter_bytes, game_object, mesh_renderer_bytes, skinned_bytes,
    transform,
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 7] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MESH_FILTER,
    ClassId::MESH_RENDERER,
    ClassId::SKINNED_MESH_RENDERER,
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

const POSITIONS: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
const WEIGHTS: [[f32; 4]; 3] = [
    [1.0, 0.0, 0.0, 0.0],
    [0.5, 0.5, 0.0, 0.0],
    [0.25, 0.25, 0.25, 0.25],
];
const JOINTS: [[u32; 4]; 3] = [[0, 0, 0, 0], [0, 1, 0, 0], [2, 1, 0, 1]];

/// The skin of the mesh: three bind poses, each 16 row-major floats starting at its own number.
fn skin() -> SkinSpec {
    let pose = |start: f32| std::array::from_fn(|i| start + f32::from(u8::try_from(i).unwrap()));
    SkinSpec {
        bind_poses: vec![pose(0.0), pose(100.0), pose(200.0)],
        bone_hashes: vec![11, 22, 33],
        root_hash: 11,
        variable_weights: Vec::new(),
    }
}

/// Three vertices in one stream of stride 44: a 32-bit float position, four float blend weights
/// and four 32-bit integer blend indices in `joint_format` (10 unsigned, 11 signed).
fn skinned_mesh(joint_format: u8) -> MeshSpec {
    let mut vertex_data = Vec::new();
    for vertex in 0..3 {
        vertex_data.extend(f32s(&POSITIONS[vertex]));
        vertex_data.extend(f32s(&WEIGHTS[vertex]));
        vertex_data.extend(u32s(&JOINTS[vertex]));
    }
    let mut channels = vec![[0, 0, 0, 0]; 14];
    channels[0] = [0, 0, 0, 3];
    channels[12] = [0, 12, 0, 4];
    channels[13] = [0, 28, joint_format, 4];
    MeshSpec {
        name: "Body",
        sub_meshes: vec![[0, 3, 0, 0, 0, 3]],
        compression: 0,
        index_format: 0,
        indices: [0u16, 1, 2].iter().flat_map(|i| i.to_le_bytes()).collect(),
        vertex_count: 3,
        channels,
        vertex_data,
        stream: (0, 0, ""),
        skin: skin(),
    }
}

/// The prefab `rig`: a root object that draws the mesh object 10 with a mesh renderer, and one
/// child, `Body`, whose skinned renderer names the same mesh, the two transforms as bones and the
/// root as root bone.
fn prefab_objects(mesh: Vec<u8>) -> Vec<ObjectData> {
    let enabled = Settings {
        enabled: true,
        ..distinct()
    };
    let bones = [ptr(0, 3), ptr(0, 5)];
    let rig = Rig {
        bones: &bones,
        root_bone: ptr(0, 3),
    };
    vec![
        object(
            2,
            ClassId::GAME_OBJECT,
            game_object("Rig", &[ptr(0, 3), ptr(0, 7), ptr(0, 8)]),
        ),
        object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[ptr(0, 5)])),
        object(7, ClassId::MESH_FILTER, filter_bytes(2, ptr(0, 10))),
        object(
            8,
            ClassId::MESH_RENDERER,
            mesh_renderer_bytes(2, &enabled, &[]),
        ),
        object(
            4,
            ClassId::GAME_OBJECT,
            game_object("Body", &[ptr(0, 5), ptr(0, 6)]),
        ),
        object(5, ClassId::TRANSFORM, transform(4, ptr(0, 3), &[])),
        object(
            6,
            ClassId::SKINNED_MESH_RENDERER,
            skinned_bytes(4, &enabled, ptr(0, 10), &[], &rig),
        ),
        object(10, ClassId::MESH, mesh),
    ]
}

fn plan_prefab(dir: &Path, mesh: Vec<u8>) -> ResourcesPlan {
    let mut managers = ObjectWriter::new(false);
    managers
        .i32(1)
        .string("prefabs/rig")
        .unwrap()
        .pptr(ptr(1, 2));
    managers.i32(0);
    write_file(
        dir,
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, managers.finish())],
        &["resources.assets"],
    );
    write_file(dir, "resources.assets", prefab_objects(mesh), &[]);
    plan_player_resources(dir, &ScriptLayouts::new(Vec::new()), &dir.join("out")).unwrap()
}

fn asset_json(plan: &ResourcesPlan, index: usize) -> Json {
    serde_json::to_value(&plan.assets[index]).unwrap()
}

fn mesh_index(plan: &ResourcesPlan) -> Option<usize> {
    (0..plan.assets.len()).find(|&i| asset_json(plan, i)["type"] == "Mesh")
}

fn prefab_skinned(dir: &Path, plan: &ResourcesPlan) -> Json {
    prefab_object(dir, plan, 1)["components"][0].clone()
}

fn prefab_object(dir: &Path, plan: &ResourcesPlan, object: usize) -> Json {
    let prefab = (0..plan.assets.len())
        .find(|&i| asset_json(plan, i)["type"] == "Prefab")
        .unwrap();
    let file = asset_json(plan, prefab)["plan"]
        .as_str()
        .unwrap()
        .to_owned();
    let scene: Json =
        serde_json::from_slice(&std::fs::read(dir.join("out").join(file)).unwrap()).unwrap();
    scene["objects"][object].clone()
}

#[test]
fn a_skinned_renderer_plans_its_mesh_bones_and_root_bone() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_prefab(dir.path(), mesh_bytes(&skinned_mesh(10)));
    let mesh = mesh_index(&plan).expect("the skinned mesh is planned");
    let skinned = prefab_skinned(dir.path(), &plan);
    assert_eq!(skinned["type"], "SkinnedMeshRenderer");
    assert_eq!(skinned["mesh"], json!(mesh));
    assert_eq!(skinned["bones"], json!([0, 1]));
    assert_eq!(skinned["root_bone"], json!(0));
    let rig = prefab_object(dir.path(), &plan, 0);
    assert_eq!(rig["components"][0]["mesh"], json!(mesh));
    assert_eq!(plan.differences, Vec::<String>::new());
}

#[test]
fn a_skinned_mesh_plans_blend_weights_blend_indices_and_bind_poses() {
    for joint_format in [10, 11] {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_prefab(dir.path(), mesh_bytes(&skinned_mesh(joint_format)));
        let mesh = asset_json(&plan, mesh_index(&plan).unwrap());
        assert_eq!(
            mesh["channels"],
            json!([
                {"name": "position", "dimension": 3},
                {"name": "blend weights", "dimension": 4},
                {"name": "blend indices", "dimension": 4}
            ])
        );
        assert_eq!(mesh["bind_poses"].as_array().unwrap().len(), 3);
        assert_eq!(mesh["bind_poses"][1][0], json!(100.0));
        assert_eq!(mesh["bind_poses"][2][15], json!(215.0));
        let bytes = std::fs::read(
            dir.path()
                .join("out")
                .join(mesh["resource"].as_str().unwrap()),
        )
        .unwrap();
        let mut expected = f32s(&POSITIONS.concat());
        expected.extend(f32s(&WEIGHTS.concat()));
        expected.extend(f32s(
            &JOINTS
                .concat()
                .into_iter()
                .map(|joint| f32::from(u16::try_from(joint).unwrap()))
                .collect::<Vec<_>>(),
        ));
        expected.extend(u32s(&[0, 1, 2]));
        assert_eq!(bytes, expected, "joint format {joint_format}");
        assert_eq!(plan.differences, Vec::<String>::new());
    }
}

#[test]
fn a_mesh_with_varying_bone_counts_is_left_out_and_named() {
    let dir = tempfile::tempdir().unwrap();
    let mut spec = skinned_mesh(10);
    spec.skin.variable_weights = vec![1, 2, 3];
    let plan = plan_prefab(dir.path(), mesh_bytes(&spec));
    assert_eq!(mesh_index(&plan), None);
    assert_eq!(prefab_skinned(dir.path(), &plan)["mesh"], Json::Null);
    let listed = plan.differences.join("\n");
    assert!(
        listed.contains("resource mesh Body: bone counts that vary per vertex are not decoded yet"),
        "{listed}"
    );
}

/// The skinned mesh with its first blend index replaced by the 32-bit pattern `index`.
fn with_first_blend_index(format: u8, index: u32) -> MeshSpec {
    let mut spec = skinned_mesh(format);
    spec.vertex_data[28..32].copy_from_slice(&index.to_le_bytes());
    spec
}

#[test]
fn a_blend_index_that_is_negative_or_beyond_sixteen_bits_leaves_the_mesh_out_and_names_why() {
    for (format, index) in [(11, u32::MAX), (10, 70_000), (11, 65_536)] {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_prefab(
            dir.path(),
            mesh_bytes(&with_first_blend_index(format, index)),
        );
        assert_eq!(mesh_index(&plan), None, "format {format} index {index}");
        let listed = plan.differences.join("\n");
        assert!(
            listed.contains("resource mesh Body: a blend index outside 0 to 65535"),
            "format {format} index {index}: {listed}"
        );
    }
}

#[test]
fn the_largest_blend_index_a_mesh_may_hold_is_kept_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_prefab(dir.path(), mesh_bytes(&with_first_blend_index(10, 65_535)));
    let index = mesh_index(&plan).expect("the mesh is planned");
    let resource = asset_json(&plan, index)["resource"]
        .as_str()
        .unwrap()
        .to_owned();
    let bytes = std::fs::read(dir.path().join("out").join(resource)).unwrap();
    // One float array per channel in plan order: the positions of the three vertices (9 floats),
    // their blend weights (12), then their blend indices, the first of which is the one set.
    let at = (3 * 3 + 4 * 3) * 4;
    let kept = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(kept.to_bits(), 65_535.0_f32.to_bits());
}

/// Three vertices in one stream: a 32-bit float position, then `weights` float blend weights per
/// vertex when it is not zero, then `joints` 32-bit unsigned blend indices per vertex.
fn partial_skin_mesh(weights: usize, joints: usize) -> MeshSpec {
    let mut vertex_data = Vec::new();
    for vertex in 0..3 {
        vertex_data.extend(f32s(&POSITIONS[vertex]));
        vertex_data.extend(f32s(&WEIGHTS[vertex][..weights]));
        vertex_data.extend(u32s(&JOINTS[vertex][..joints]));
    }
    let mut channels = vec![[0, 0, 0, 0]; 14];
    channels[0] = [0, 0, 0, 3];
    let joint_at = 12 + 4 * weights;
    if weights > 0 {
        channels[12] = [0, 12, 0, u8::try_from(weights).unwrap()];
    }
    channels[13] = [
        0,
        u8::try_from(joint_at).unwrap(),
        10,
        u8::try_from(joints).unwrap(),
    ];
    MeshSpec {
        channels,
        vertex_data,
        ..skinned_mesh(10)
    }
}

/// The floats of the planned channel `name` of the mesh, read from its resource.
fn planned_channel(dir: &Path, mesh: &Json, name: &str) -> Vec<f32> {
    let bytes = std::fs::read(dir.join("out").join(mesh["resource"].as_str().unwrap())).unwrap();
    let mut at = 0;
    for channel in mesh["channels"].as_array().unwrap() {
        let count = 3 * usize::try_from(channel["dimension"].as_u64().unwrap()).unwrap();
        if channel["name"] == name {
            return bytes[at..at + count * 4]
                .chunks(4)
                .map(|value| f32::from_le_bytes(value.try_into().unwrap()))
                .collect();
        }
        at += count * 4;
    }
    panic!("no channel {name} in {}", mesh["channels"]);
}

fn joint_floats() -> [[f32; 4]; 3] {
    JOINTS.map(|row| row.map(|joint| f32::from(u16::try_from(joint).unwrap())))
}

fn padded(rows: &[[f32; 4]; 3], kept: usize) -> Vec<f32> {
    rows.iter()
        .flat_map(|row| (0..4).map(move |slot| if slot < kept { row[slot] } else { 0.0 }))
        .collect()
}

#[test]
fn a_mesh_with_blend_indices_and_no_blend_weights_is_rigid_to_each_vertexs_first_bone() {
    for joints in [1, 2] {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_prefab(dir.path(), mesh_bytes(&partial_skin_mesh(0, joints)));
        let mesh = asset_json(&plan, mesh_index(&plan).expect("the mesh is planned"));
        let indices: Vec<f32> = padded(&joint_floats(), joints);
        assert_eq!(planned_channel(dir.path(), &mesh, "blend indices"), indices);
        let rigid = [1.0, 0.0, 0.0, 0.0].repeat(3);
        assert_eq!(planned_channel(dir.path(), &mesh, "blend weights"), rigid);
        assert_eq!(plan.differences, Vec::<String>::new(), "{joints} indices");
    }
}

#[test]
fn blend_weights_and_indices_of_fewer_than_four_bones_are_padded_to_four() {
    let dir = tempfile::tempdir().unwrap();
    let plan = plan_prefab(dir.path(), mesh_bytes(&partial_skin_mesh(2, 2)));
    let mesh = asset_json(&plan, mesh_index(&plan).expect("the mesh is planned"));
    let weights = planned_channel(dir.path(), &mesh, "blend weights");
    assert_eq!(weights, padded(&WEIGHTS, 2));
    let joints = planned_channel(dir.path(), &mesh, "blend indices");
    assert_eq!(joints, padded(&joint_floats(), 2));
    assert_eq!(plan.differences, Vec::<String>::new());
}
