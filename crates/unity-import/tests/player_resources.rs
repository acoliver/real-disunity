use std::path::Path;

use serde_json::json;
use worldrunner_unity_import::{
    PlayerError,
    player_identity::{QualityLevels, player_identity, player_quality, player_scene_paths},
    player_resources::{ResourceAsset, ResourcesPlan, plan_player_resources},
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    player_settings::{FIELDS, Field},
    script_data::{FieldType, Primitive, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 8] = [
    ClassId::RESOURCE_MANAGER,
    ClassId::TEXT_ASSET,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::LIGHT,
    ClassId::BUILD_SETTINGS,
    ClassId::PLAYER_SETTINGS,
    ClassId::QUALITY_SETTINGS,
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
        script_id: (class_id == ClassId::MONO_BEHAVIOUR).then_some([7; 16]),
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

fn resource_manager() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    let entries = [
        ("text/hello", ptr(1, 2)),
        ("config/main", ptr(1, 3)),
        ("art/light", ptr(1, 4)),
        ("stripped", ptr(0, 0)),
    ];
    w.i32(4);
    for (path, target) in entries {
        w.string(path).unwrap().pptr(target);
    }
    w.i32(0);
    w.finish()
}

fn text_asset() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("hello").unwrap().byte_array(b"hi there").unwrap();
    w.finish()
}

fn script() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Config").unwrap().i32(0).bytes(&[0; 16]);
    w.string("Config")
        .unwrap()
        .string("Example")
        .unwrap()
        .string("Example.Game.dll")
        .unwrap();
    w.finish()
}

/// A `Config` asset named `main` holding a level, the text asset and an unplanned light.
fn config() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 0))
        .u8(1)
        .align()
        .pptr(ptr(0, 5))
        .string("main")
        .unwrap()
        .i32(3)
        .pptr(ptr(0, 2))
        .pptr(ptr(0, 4));
    w.finish()
}

fn layouts() -> ScriptLayouts {
    let field = |name: &str, ty| ScriptField {
        name: name.to_owned(),
        ty,
    };
    let object = || FieldType::Object("UnityEngine.Object".to_owned());
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Config".to_owned(),
        value_type: false,
        fields: vec![
            field("level", FieldType::Primitive(Primitive::I32)),
            field("notes", object()),
            field("light", object()),
        ],
    }])
}

/// Plans the resources of a player with a text asset, a script asset and a light.
fn planned() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::RESOURCE_MANAGER, resource_manager())],
        &["resources.assets"],
    );
    write_file(
        dir.path(),
        "resources.assets",
        vec![
            object(2, ClassId::TEXT_ASSET, text_asset()),
            object(3, ClassId::MONO_BEHAVIOUR, config()),
            object(4, ClassId::LIGHT, Vec::new()),
            object(5, ClassId::MONO_SCRIPT, script()),
        ],
        &[],
    );
    let plan = plan_player_resources(dir.path(), &layouts(), &dir.path().join("out")).unwrap();
    (dir, plan)
}

#[test]
fn lists_every_named_resource_with_its_class() {
    let (_dir, plan) = planned();
    let entries: Vec<_> = plan
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.class.as_str(), e.asset))
        .collect();
    assert_eq!(
        entries,
        [
            ("text/hello", "TextAsset", Some(0)),
            ("config/main", "MonoBehaviour", Some(1)),
            ("art/light", "Light", None),
        ]
    );
    assert_eq!(
        plan.differences,
        [
            "resource stripped names no object",
            "resource main.light: references object 4 of file 0, which the plan does not hold",
        ]
    );
}

#[test]
fn plans_text_by_its_bytes_and_scripts_by_their_fields() {
    let (dir, plan) = planned();
    let ResourceAsset::TextAsset {
        name,
        file,
        offset,
        length,
    } = &plan.assets[0]
    else {
        panic!("the first asset is the text: {:?}", plan.assets[0])
    };
    let bytes = std::fs::read(file).unwrap();
    let start = usize::try_from(*offset).unwrap();
    let end = start + usize::try_from(*length).unwrap();
    assert_eq!(
        (name.as_str(), &bytes[start..end]),
        ("hello", &b"hi there"[..])
    );
    let ResourceAsset::Script {
        class,
        assembly,
        name,
        fields,
    } = &plan.assets[1]
    else {
        panic!("the second asset is the script: {:?}", plan.assets[1])
    };
    assert_eq!(
        (class.as_str(), assembly.as_str(), name.as_str()),
        ("Example.Config", "Example.Game", "main")
    );
    let path = dir.path().join("out").join(fields);
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(
        written,
        json!({"level": "3", "notes": {"$asset": 0}, "light": null})
    );
}

/// `PlayerSettings` bytes naming the company, product and version, with every other field
/// zero and every collection empty.
fn player_settings() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    for &(name, field, aligned) in FIELDS {
        match field {
            Field::Bool => {
                w.u8(0);
            }
            Field::I32
            | Field::U32
            | Field::F32
            | Field::SplashLogos
            | Field::StringIntMap
            | Field::I32s => {
                w.i32(0);
            }
            Field::Str => {
                let text = match name {
                    "companyName" => "Studio",
                    "productName" => "Demo",
                    "bundleVersion" => "2.0.1",
                    _ => "",
                };
                w.string(text).unwrap();
            }
            Field::PPtr => {
                w.pptr(ptr(0, 0));
            }
            Field::Floats(n) => {
                w.floats(&vec![0.0; n]);
            }
            Field::Bytes(n) => {
                w.bytes(&vec![0; n]);
            }
        }
        if aligned {
            w.align();
        }
    }
    w.i32(77);
    w.finish()
}

fn build_settings(version: &str) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    for _ in 0..4 {
        w.i32(0);
    }
    w.bytes(&[0; 14]).align();
    w.string(version).unwrap().string("").unwrap().i32(0);
    w.finish()
}

#[test]
fn reads_the_identity_a_player_reports() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("Demo_Data");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(dir.path().join("UnityPlayer.dll"), b"").unwrap();
    write_file(
        &data,
        "globalgamemanagers",
        vec![
            object(1, ClassId::BUILD_SETTINGS, build_settings("6000.3.15f1")),
            object(2, ClassId::PLAYER_SETTINGS, player_settings()),
        ],
        &[],
    );
    let identity = player_identity(&data).unwrap();
    assert_eq!(
        (
            identity.company_name.as_str(),
            identity.product_name.as_str(),
            identity.version.as_str(),
            identity.unity_version.as_str(),
            identity.platform,
        ),
        ("Studio", "Demo", "2.0.1", "6000.3.15f1", "WindowsPlayer")
    );
    // Another release's settings layout is not read; the names come from app.info.
    std::fs::remove_file(dir.path().join("UnityPlayer.dll")).unwrap();
    std::fs::write(data.join("app.info"), "Other Studio\nOther Demo").unwrap();
    write_file(
        &data,
        "globalgamemanagers",
        vec![
            object(1, ClassId::BUILD_SETTINGS, build_settings("2022.3.1f1")),
            object(2, ClassId::PLAYER_SETTINGS, vec![0; 4]),
        ],
        &[],
    );
    let identity = player_identity(&data).unwrap();
    assert_eq!(
        (
            identity.company_name.as_str(),
            identity.product_name.as_str(),
            identity.version.as_str(),
            identity.platform,
        ),
        ("Other Studio", "Other Demo", "", "LinuxPlayer")
    );
}

/// Writes a 6000.3 player's managers naming Studio's Demo into `data`.
fn write_managers(data: &Path) {
    write_file(
        data,
        "globalgamemanagers",
        vec![
            object(1, ClassId::BUILD_SETTINGS, build_settings("6000.3.15f1")),
            object(2, ClassId::PLAYER_SETTINGS, player_settings()),
        ],
        &[],
    );
}

#[test]
fn rejects_decoded_names_that_app_info_contradicts() {
    let dir = tempfile::tempdir().unwrap();
    write_managers(dir.path());
    std::fs::write(dir.path().join("app.info"), "Studio\nDemo").unwrap();
    assert_eq!(player_identity(dir.path()).unwrap().product_name, "Demo");
    std::fs::write(dir.path().join("app.info"), "Studio\nOther Demo").unwrap();
    match player_identity(dir.path()) {
        Err(PlayerError::IdentityMismatch {
            decoded, app_info, ..
        }) => {
            assert_eq!(decoded.1, "Demo");
            assert_eq!(app_info.1, "Other Demo");
        }
        other => panic!("a contradiction is reported: {other:?}"),
    }
}

#[test]
fn recognizes_a_macos_app_bundle() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("Demo.app/Contents/Resources/Data");
    std::fs::create_dir_all(&data).unwrap();
    write_managers(&data);
    assert_eq!(player_identity(&data).unwrap().platform, "OSXPlayer");
}

fn quality_settings(current: i32, names: &[&str]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(current).i32(i32::try_from(names.len()).unwrap());
    for name in names {
        w.string(name).unwrap().bytes(&[0x5A; 192]);
    }
    w.bytes(&[0; 8]);
    w.finish()
}

#[test]
fn reads_the_quality_levels_a_player_defines() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![
            object(1, ClassId::BUILD_SETTINGS, build_settings("6000.3.15f1")),
            object(
                2,
                ClassId::QUALITY_SETTINGS,
                quality_settings(1, &["Low", "Medium", "High"]),
            ),
        ],
        &[],
    );
    let quality = player_quality(dir.path()).unwrap().unwrap();
    assert_eq!(
        quality,
        QualityLevels {
            names: vec!["Low".into(), "Medium".into(), "High".into()],
            current: 1,
        }
    );
    // The play request carries the levels in this shape.
    assert_eq!(
        serde_json::to_value(&quality).unwrap(),
        json!({"names": ["Low", "Medium", "High"], "current": 1})
    );
}

#[test]
fn a_player_without_quality_settings_has_no_quality_levels() {
    let dir = tempfile::tempdir().unwrap();
    write_managers(dir.path());
    assert_eq!(player_quality(dir.path()).unwrap(), None);
}

fn build_settings_with_scenes(scenes: &[&str]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(i32::try_from(scenes.len()).unwrap());
    for scene in scenes {
        w.string(scene).unwrap();
    }
    for _ in 0..3 {
        w.i32(0);
    }
    w.bytes(&[0; 14]).align();
    w.string("6000.3.15f1").unwrap().string("").unwrap().i32(0);
    w.finish()
}

#[test]
fn reads_the_scene_paths_of_a_players_build_in_build_order() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(
            1,
            ClassId::BUILD_SETTINGS,
            build_settings_with_scenes(&[
                "Assets/GameAssets/Scenes/StartScreen.unity",
                "Assets/GameAssets/Scenes/MainGame.unity",
            ]),
        )],
        &[],
    );
    assert_eq!(
        player_scene_paths(dir.path()).unwrap(),
        vec![
            "Assets/GameAssets/Scenes/StartScreen.unity".to_owned(),
            "Assets/GameAssets/Scenes/MainGame.unity".to_owned(),
        ]
    );
}

#[test]
fn a_player_without_build_settings_lists_no_scenes() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "globalgamemanagers", vec![], &[]);
    assert!(player_scene_paths(dir.path()).unwrap().is_empty());
}
