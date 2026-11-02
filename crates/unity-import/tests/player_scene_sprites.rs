//! Sprites a player scene's script fields reference, planned as scene assets with the textures
//! they draw.

use std::path::Path;

use serde_json::{Value, json};
use worldrunner_unity_import::{
    plan::{PlanComponent, ScenePlan},
    player_plan::plan_player_scene,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

#[path = "support/sprite_bytes.rs"]
mod sprite_bytes;
use sprite_bytes::{Pixels, sprite, texture};

const TYPES: [ClassId; 7] = [
    ClassId::TEXTURE_2D,
    ClassId::SPRITE,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::SHADER,
];

const PIXELS: [u8; 16] = [
    11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
];
const STREAM: [u8; 24] = [
    90, 91, 92, 93, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10,
];

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

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

fn script() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Holder").unwrap().i32(0).bytes(&[0; 16]);
    w.string("Holder")
        .unwrap()
        .string("Example")
        .unwrap()
        .string("Example.Game.dll")
        .unwrap();
    w.finish()
}

fn behaviour(owner: i64, script: PPtr, references: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, owner)).u8(1).align().pptr(script);
    w.string("").unwrap();
    for &reference in references {
        w.pptr(reference);
    }
    w.finish()
}

fn layouts(fields: &[&str]) -> ScriptLayouts {
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Holder".to_owned(),
        value_type: false,
        fields: fields
            .iter()
            .map(|name| ScriptField {
                name: (*name).to_owned(),
                ty: FieldType::Object("UnityEngine.Sprite".to_owned()),
            })
            .collect(),
    }])
}

const FIELDS: [&str; 7] = ["first", "again", "sibling", "cube", "far", "bare", "boned"];

/// A scene whose root holds sprites 10 (twice), 11, 12 and 13 of its shared assets file: 10 and
/// 11 draw the inline texture 20, 12 a cube map and 13 a texture in the `.resS` file.
fn write_player(dir: &Path) {
    let game_object = {
        let mut w = ObjectWriter::new(false);
        w.pptrs(&[ptr(0, 3), ptr(0, 4)])
            .unwrap()
            .u32(0)
            .string("Root")
            .unwrap()
            .u16(0)
            .u8(1);
        w.finish()
    };
    let transform = {
        let mut w = ObjectWriter::new(false);
        w.pptr(ptr(0, 2)).floats(&[0.0, 0.0, 0.0, 1.0]);
        w.floats(&[0.0; 3]).floats(&[1.0; 3]);
        w.pptrs(&[]).unwrap().pptr(ptr(0, 0));
        w.finish()
    };
    let references = [
        ptr(1, 10),
        ptr(1, 10),
        ptr(1, 11),
        ptr(1, 12),
        ptr(1, 13),
        ptr(1, 14),
        ptr(1, 15),
    ];
    write_file(
        dir,
        "level0",
        vec![
            object(2, ClassId::GAME_OBJECT, game_object),
            object(3, ClassId::TRANSFORM, transform),
            object(
                4,
                ClassId::MONO_BEHAVIOUR,
                behaviour(2, ptr(0, 5), &references),
            ),
            object(5, ClassId::MONO_SCRIPT, script()),
        ],
        &["sharedassets0.assets"],
    );
    let streamed = Pixels::Streamed("archive:/CAB-1/sharedassets0.assets.resS", 4, 16);
    write_file(
        dir,
        "sharedassets0.assets",
        vec![
            object(
                20,
                ClassId::TEXTURE_2D,
                texture("sheet", 1, 2, &Pixels::Inline(&PIXELS)),
            ),
            object(
                21,
                ClassId::TEXTURE_2D,
                texture("cube", 6, 4, &Pixels::Inline(&[])),
            ),
            object(22, ClassId::TEXTURE_2D, texture("far", 1, 2, &streamed)),
            object(10, ClassId::SPRITE, sprite("first", 20, false)),
            object(11, ClassId::SPRITE, sprite("sibling", 20, false)),
            object(12, ClassId::SPRITE, sprite("cubed", 21, false)),
            object(13, ClassId::SPRITE, sprite("distant", 22, false)),
            object(14, ClassId::SPRITE, sprite("bare", 0, false)),
            object(15, ClassId::SPRITE, sprite("boned", 20, true)),
        ],
        &[],
    );
    std::fs::write(dir.join("sharedassets0.assets.resS"), STREAM).unwrap();
}

fn planned() -> (tempfile::TempDir, ScenePlan) {
    let dir = tempfile::tempdir().unwrap();
    write_player(dir.path());
    let plan = plan_player_scene(dir.path(), "level0", &layouts(&FIELDS)).unwrap();
    (dir, plan)
}

fn assets(plan: &ScenePlan) -> Vec<Value> {
    serde_json::to_value(&plan.assets)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

fn script_fields(plan: &ScenePlan) -> &Value {
    let [PlanComponent::Script { fields, .. }] = &plan.objects[0].components[..] else {
        panic!("the root has one script: {:?}", plan.objects[0].components)
    };
    fields
}

#[test]
fn a_referenced_sprite_is_a_scene_asset_after_the_texture_it_draws() {
    let (_dir, plan) = planned();
    let assets = assets(&plan);
    assert_eq!(assets[0]["type"], "Texture2D");
    assert_eq!(assets[0]["name"], "sheet");
    assert_eq!(assets[1]["type"], "Sprite");
    assert_eq!(assets[1]["name"], "first");
    assert_eq!(assets[1]["texture"], json!({"$asset": 0}));
    assert_eq!(assets[1]["rect"], json!([2.0, 4.0, 16.0, 8.0]));
    assert_eq!(assets[1]["pixels_per_unit"], 50.0);
    assert_eq!(assets[1]["border"], json!([1.0, 2.0, 3.0, 4.0]));
    assert_eq!(assets[1]["pivot"], json!([0.5, 0.25]));
    assert_eq!(assets[1]["texture_rect"], json!([3.0, 5.0, 14.0, 6.0]));
    assert_eq!(assets[1]["texture_rect_offset"], json!([1.0, 0.5]));
    assert_eq!(
        (
            &assets[1]["packed"],
            &assets[1]["packing_rotation"],
            &assets[1]["mesh_type"]
        ),
        (&json!(true), &json!(3), &json!(1))
    );
}

#[test]
fn a_texture_carries_its_size_format_and_sampler() {
    let (_dir, plan) = planned();
    let texture = &assets(&plan)[0];
    assert_eq!(
        (
            &texture["width"],
            &texture["height"],
            &texture["format"],
            &texture["mip_count"]
        ),
        (&json!(2), &json!(2), &json!(4), &json!(1))
    );
    assert_eq!(
        (
            &texture["readable"],
            &texture["color_space"],
            &texture["filter_mode"]
        ),
        (&json!(true), &json!(1), &json!(1))
    );
    assert_eq!(
        (&texture["wrap_u"], &texture["wrap_v"], &texture["wrap_w"]),
        (&json!(0), &json!(1), &json!(2))
    );
    assert_eq!(
        (&texture["aniso_level"], &texture["mip_bias"]),
        (&json!(4), &json!(-0.5))
    );
}

#[test]
fn a_sprite_with_no_texture_binds_null_and_is_listed() {
    let (_dir, plan) = planned();
    assert!(
        plan.differences
            .iter()
            .any(|line| line.starts_with("Root.Example.Holder.bare:")
                && line.contains("texture is not planned")),
        "{:?}",
        plan.differences
    );
}

#[test]
fn what_the_engine_does_not_build_of_a_sprite_is_listed() {
    let (_dir, plan) = planned();
    assert!(
        plan.differences
            .contains(&"resource sprite boned: bones are not built".to_owned()),
        "{:?}",
        plan.differences
    );
}

#[test]
fn script_fields_refer_to_the_sprites_and_a_shared_sprite_is_one_asset() {
    let (_dir, plan) = planned();
    assert_eq!(
        *script_fields(&plan),
        json!({
            "first": {"$asset": 1},
            "again": {"$asset": 1},
            "sibling": {"$asset": 2},
            "cube": null,
            "far": {"$asset": 4},
            "bare": null,
            "boned": {"$asset": 5},
        })
    );
}

#[test]
fn sprites_of_one_texture_share_its_asset() {
    let (_dir, plan) = planned();
    let assets = assets(&plan);
    assert_eq!(assets[2]["name"], "sibling");
    assert_eq!(assets[2]["texture"], json!({"$asset": 0}));
    let textures = assets.iter().filter(|a| a["type"] == "Texture2D").count();
    assert_eq!(textures, 2, "{assets:?}");
}

#[test]
fn inline_pixels_are_named_in_place_in_the_serialized_file() {
    let (dir, plan) = planned();
    let data = &assets(&plan)[0]["data"];
    let file = data["file"].as_str().unwrap();
    assert_eq!(Path::new(file), dir.path().join("sharedassets0.assets"));
    let offset = usize::try_from(data["offset"].as_u64().unwrap()).unwrap();
    assert_eq!(data["size"], 16);
    let bytes = std::fs::read(file).unwrap();
    assert_eq!(bytes[offset..offset + 16], PIXELS);
}

#[test]
fn streamed_pixels_are_named_in_the_res_s_file() {
    let (dir, plan) = planned();
    let assets = assets(&plan);
    assert_eq!(assets[3]["name"], "far");
    let data = &assets[3]["data"];
    assert_eq!(
        Path::new(data["file"].as_str().unwrap()),
        dir.path().join("sharedassets0.assets.resS")
    );
    assert_eq!(
        (data["offset"].clone(), data["size"].clone()),
        (4.into(), 16.into())
    );
    assert_eq!(assets[4]["texture"], json!({"$asset": 3}));
}

#[test]
fn a_sprite_over_a_texture_the_engine_does_not_build_binds_null_and_is_listed() {
    let (_dir, plan) = planned();
    assert!(
        plan.differences
            .iter()
            .any(|line| line.starts_with("Root.Example.Holder.cube:") && line.contains("cube")),
        "{:?}",
        plan.differences
    );
    assert!(
        assets(&plan).iter().all(|asset| asset["name"] != "cubed"),
        "no sprite of the cube map is planned"
    );
}
