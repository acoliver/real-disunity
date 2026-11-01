//! Animators of a player's prefabs and scenes, and the controller assets they reference: an
//! animator plans with its settings and the asset index of its controller, controllers and
//! override controllers plan with the clips they list, and what the plan cannot hold binds
//! null and is listed.

use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::{
    player_plan::plan_player_scene,
    player_resources::{ResourceAsset, ResourcesPlan, plan_player_resources},
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components).unwrap().u32(0).string(name).unwrap();
    w.u16(0).u8(1);
    w.finish()
}

fn transform(game_object: i64) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[0.0; 3]).floats(&[1.0; 3]);
    w.pptrs(&[]).unwrap().pptr(ptr(0, 0));
    w.finish()
}

const TYPES: [ClassId; 7] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::RESOURCE_MANAGER,
    ClassId::ANIMATOR,
    ClassId::ANIMATOR_CONTROLLER,
    ClassId::ANIMATOR_OVERRIDE_CONTROLLER,
    ClassId::ANIMATION_CLIP,
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

/// What an animator of a test sets: the references and the two modes and flag bytes.
struct AnimatorSpec {
    object: i64,
    enabled: bool,
    avatar: PPtr,
    controller: PPtr,
    modes: [i32; 2],
    flags: [u8; 8],
}

fn animator(spec: &AnimatorSpec) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, spec.object))
        .u8(u8::from(spec.enabled))
        .align();
    w.pptr(spec.avatar).pptr(spec.controller);
    w.i32(spec.modes[0]).i32(spec.modes[1]);
    w.bytes(&spec.flags);
    w.finish()
}

fn plain(object: i64, controller: PPtr) -> AnimatorSpec {
    AnimatorSpec {
        object,
        enabled: true,
        avatar: ptr(0, 0),
        controller,
        modes: [0, 0],
        flags: [0; 8],
    }
}

/// A controller with no layers, parameters or names that lists `clips`.
fn controller(name: &str, clips: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().align();
    w.u32(0).i32(0).i32(0);
    for _ in 0..8 {
        w.i32(0);
    }
    w.i32(0);
    w.pptrs(clips).unwrap();
    w.i32(0).i32(0).i32(0);
    w.u8(0).align();
    w.finish()
}

fn override_controller(name: &str, base: PPtr, pairs: &[(PPtr, PPtr)]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().align().pptr(base);
    w.i32(i32::try_from(pairs.len()).unwrap());
    for &(original, replacement) in pairs {
        w.pptr(original).pptr(replacement);
    }
    w.finish()
}

fn clip(name: &str) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string(name).unwrap().align().bytes(&[7; 12]);
    w.finish()
}

fn resource_manager(prefabs: &[(&str, i64)]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(i32::try_from(prefabs.len()).unwrap());
    for &(path, target) in prefabs {
        w.string(path).unwrap().pptr(ptr(1, target));
    }
    w.i32(0);
    w.finish()
}

/// A prefab object of `name` with a transform and one animator at the next path IDs.
fn prefab(object_id: i64, name: &str, spec: &AnimatorSpec) -> Vec<ObjectData> {
    vec![
        object(
            object_id,
            ClassId::GAME_OBJECT,
            game_object(name, &[ptr(0, object_id + 1), ptr(0, object_id + 2)]),
        ),
        object(object_id + 1, ClassId::TRANSFORM, transform(object_id)),
        object(object_id + 2, ClassId::ANIMATOR, animator(spec)),
    ]
}

/// A "Unit" prefab with an animator on an override controller (path 20) of a base controller
/// (21) that lists clips 30 and 32; the override replaces clip 30 with 31 and removes clip 32.
/// A "Static" prefab has an animator with no controller. A "Lost" prefab's animator names an
/// avatar and a controller the file does not hold.
fn plan() -> (tempfile::TempDir, ResourcesPlan) {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(
            1,
            ClassId::RESOURCE_MANAGER,
            resource_manager(&[("unit", 2), ("static", 5), ("lost", 8)]),
        )],
        &["resources.assets"],
    );
    let unit = AnimatorSpec {
        enabled: false,
        modes: [2, 1],
        flags: [1, 0, 1, 0, 1, 1, 1, 1],
        ..plain(2, ptr(0, 20))
    };
    let lost = AnimatorSpec {
        avatar: ptr(0, 700),
        ..plain(8, ptr(0, 701))
    };
    let mut objects = prefab(2, "Unit", &unit);
    objects.extend(prefab(5, "Static", &plain(5, ptr(0, 0))));
    objects.extend(prefab(8, "Lost", &lost));
    objects.push(object(
        20,
        ClassId::ANIMATOR_OVERRIDE_CONTROLLER,
        override_controller(
            "Nets Animator",
            ptr(0, 21),
            &[(ptr(0, 30), ptr(0, 31)), (ptr(0, 32), ptr(0, 0))],
        ),
    ));
    objects.push(object(
        21,
        ClassId::ANIMATOR_CONTROLLER,
        controller("Base", &[ptr(0, 30), ptr(0, 32)]),
    ));
    for (id, name) in [(30, "Walk"), (31, "WalkNets"), (32, "Idle")] {
        objects.push(object(id, ClassId::ANIMATION_CLIP, clip(name)));
    }
    write_file(dir.path(), "resources.assets", objects, &[]);
    let plan = plan_player_resources(
        dir.path(),
        &ScriptLayouts::new(Vec::new()),
        &dir.path().join("out"),
    )
    .unwrap();
    (dir, plan)
}

fn as_json(plan: &ResourcesPlan, index: usize) -> Json {
    serde_json::to_value(&plan.assets[index]).unwrap()
}

/// The index and plan entry of the asset of `kind` named `name`; there is exactly one.
fn named(plan: &ResourcesPlan, kind: &str, name: &str) -> (usize, Json) {
    let found: Vec<_> = (0..plan.assets.len())
        .map(|i| (i, as_json(plan, i)))
        .filter(|(_, asset)| asset["type"] == kind && asset["name"] == name)
        .collect();
    assert_eq!(found.len(), 1, "{kind} {name}: {:?}", plan.assets);
    found.into_iter().next().unwrap()
}

fn prefab_components(dir: &tempfile::TempDir, plan: &ResourcesPlan, name: &str) -> Vec<Json> {
    let file = plan
        .assets
        .iter()
        .find_map(|asset| match asset {
            ResourceAsset::Prefab { name: n, plan } if n == name => Some(plan),
            _ => None,
        })
        .unwrap_or_else(|| panic!("prefab {name}: {:?}", plan.assets));
    let bytes = std::fs::read(dir.path().join("out").join(file)).unwrap();
    let scene: Json = serde_json::from_slice(&bytes).unwrap();
    scene["objects"][0]["components"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn an_animator_plans_its_settings_and_the_asset_index_of_its_controller() {
    let (dir, plan) = plan();
    let (controller, _) = named(&plan, "AnimatorOverrideController", "Nets Animator");
    assert_eq!(
        prefab_components(&dir, &plan, "Unit"),
        vec![json!({
            "type": "Animator", "enabled": false, "controller": controller,
            "culling_mode": 2, "update_mode": 1, "apply_root_motion": true,
            "linear_velocity_blending": false, "stabilize_feet": true,
            "keep_state_on_disable": true, "write_defaults_on_disable": true,
        })]
    );
}

#[test]
fn an_override_controller_plans_its_base_and_the_clips_it_replaces() {
    let (_dir, plan) = plan();
    let (base, base_json) = named(&plan, "AnimatorController", "Base");
    let (walk, _) = named(&plan, "AnimationClip", "Walk");
    let (walk_nets, _) = named(&plan, "AnimationClip", "WalkNets");
    let (idle, _) = named(&plan, "AnimationClip", "Idle");
    let (_, nets) = named(&plan, "AnimatorOverrideController", "Nets Animator");
    assert_eq!(
        nets,
        json!({
            "type": "AnimatorOverrideController", "name": "Nets Animator", "base_controller": base,
            "overrides": [
                {"original": walk, "replacement": walk_nets},
                {"original": idle, "replacement": null},
            ],
        })
    );
    assert_eq!(
        base_json,
        json!({"type": "AnimatorController", "name": "Base", "clips": [walk, idle]})
    );
}

#[test]
fn a_clip_plans_once_however_many_controllers_list_it() {
    let (_dir, plan) = plan();
    let clips = (0..plan.assets.len())
        .filter(|&i| as_json(&plan, i)["type"] == "AnimationClip")
        .count();
    assert_eq!(clips, 3);
}

#[test]
fn an_animator_with_no_controller_plans_null_and_lists_nothing() {
    let (dir, plan) = plan();
    let components = prefab_components(&dir, &plan, "Static");
    assert_eq!(components[0]["controller"], Json::Null);
    assert_eq!(components[0]["enabled"], true);
    assert!(
        plan.differences.iter().all(|d| !d.contains("Static")),
        "{:?}",
        plan.differences
    );
}

#[test]
fn an_avatar_and_a_controller_the_plan_cannot_hold_are_listed_and_bind_null() {
    let (dir, plan) = plan();
    let components = prefab_components(&dir, &plan, "Lost");
    assert_eq!(components[0]["controller"], Json::Null);
    let lost: Vec<_> = plan
        .differences
        .iter()
        .filter(|d| d.starts_with("prefab Lost:"))
        .cloned()
        .collect();
    assert_eq!(
        lost,
        vec![
            "prefab Lost: Lost: Animator avatar references an Avatar asset, which plans cannot build yet",
            "prefab Lost: Lost: Animator controller references object 701 of file 0, which the plan does not hold",
        ]
    );
}

#[test]
fn a_scene_animator_plans_with_a_null_controller_and_says_scenes_hold_none() {
    let dir = tempfile::tempdir().unwrap();
    let mut objects = prefab(2, "Hero", &plain(2, ptr(0, 20)));
    objects.push(object(
        20,
        ClassId::ANIMATOR_CONTROLLER,
        controller("Base", &[]),
    ));
    write_file(dir.path(), "level0", objects, &[]);
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    let component = serde_json::to_value(&plan.objects[0].components[0]).unwrap();
    assert_eq!(component["type"], "Animator");
    assert_eq!(component["controller"], Json::Null);
    assert_eq!(
        plan.differences,
        vec![
            "Hero: Animator controller references AnimatorController object 20 of file 0, which scene plans do not hold"
        ]
    );
}
