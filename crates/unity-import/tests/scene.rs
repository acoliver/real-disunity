use worldrunner_unity_import::{
    Guid, ObjectId, Origin, PackageAsset, PackageMaterial, Reference, Scene, SceneError, Value,
};

const ITEM: &str = "11111111111111111111111111111111";
const CONFIG: &str = "22222222222222222222222222222222";
const TEXTURE: &str = "33333333333333333333333333333333";
const SCRIPT: &str = "44444444444444444444444444444444";
const OUTER: &str = "55555555555555555555555555555555";
const LEGACY: &str = "66666666666666666666666666666666";

#[path = "support/fixture.rs"]
mod fixture;
#[path = "support/objects.rs"]
mod objects;

use fixture::Fixture;
use objects::{game_object, import, transform};

fn fixture() -> Fixture {
    let fixture = Fixture::new("6000.3.7f1");
    fixture.asset(
        "Assets/Scripts/Item.cs",
        SCRIPT,
        "MonoImporter",
        "class Item {}",
    );
    fixture.asset("Assets/Art/item.png", TEXTURE, "TextureImporter", "png");
    fixture.asset(
        "Assets/Config.asset",
        CONFIG,
        "NativeFormatImporter",
        config(),
    );
    fixture.asset("Assets/Item.prefab", ITEM, "PrefabImporter", item_prefab());
    fixture
}

fn behaviour(id: u64, object: u64, fields: &str) -> String {
    format!(
        "--- !u!114 &{id}\nMonoBehaviour:\n  m_GameObject: {{fileID: {object}}}\n  m_Enabled: 1\n  m_Script: {{fileID: 11500000, guid: {SCRIPT}, type: 3}}\n{fields}"
    )
}

fn config() -> String {
    format!(
        "--- !u!114 &11400000\nMonoBehaviour:\n  m_GameObject: {{fileID: 0}}\n  m_Script: {{fileID: 11500000, guid: {SCRIPT}, type: 3}}\n  gridSize: {{x: 40, y: 20}}\n"
    )
}

/// Item.prefab: root `Item` with a transform, a behaviour and a collider, and one child.
fn item_prefab() -> String {
    [
        game_object(10, "Item", &[20, 30, 40]),
        transform(20, 10, 1.0, 0),
        behaviour(30, 10, "  speed: 1\n  targets: []\n  self: {fileID: 20}\n"),
        "--- !u!61 &40\nBoxCollider2D:\n  m_GameObject: {fileID: 10}\n".into(),
        game_object(50, "Child", &[60]),
        transform(60, 50, 2.0, 20),
    ]
    .concat()
}

fn instance(id: u64, prefab: &str, parent: u64, modifications: &str, removed: &str) -> String {
    format!(
        "--- !u!1001 &{id}\nPrefabInstance:\n  m_Modification:\n    serializedVersion: 3\n    m_TransformParent: {{fileID: {parent}}}\n    m_Modifications:\n{modifications}    m_RemovedComponents:{removed}\n    m_RemovedGameObjects: []\n    m_AddedGameObjects: []\n    m_AddedComponents: []\n  m_SourcePrefab: {{fileID: 100100000, guid: {prefab}, type: 3}}\n"
    )
}

fn modification(guid: &str, target: u64, path: &str, value: &str, reference: &str) -> String {
    format!(
        "    - target: {{fileID: {target}, guid: {guid}, type: 3}}\n      propertyPath: {path}\n      value: {value}\n      objectReference: {reference}\n"
    )
}

fn by_name(scene: &Scene, name: &str) -> ObjectId {
    scene
        .objects()
        .find(|(id, _)| scene.game_object(*id).is_some_and(|o| o.name == name))
        .map_or_else(|| panic!("no GameObject {name}"), |(id, _)| id)
}

fn component(scene: &Scene, object: ObjectId, class_id: i32) -> ObjectId {
    scene
        .game_object(object)
        .unwrap()
        .components
        .into_iter()
        .find(|id| scene.object(*id).unwrap().class_id == class_id)
        .unwrap()
}

fn reference(scene: &Scene, id: ObjectId, path: &[&str]) -> Reference {
    let mut value = &scene.object(id).unwrap().fields;
    for key in path {
        value = value.get(key).unwrap();
    }
    value.as_reference().unwrap()
}

fn modern_scene() -> String {
    let overrides = [
        modification(ITEM, 10, "m_Name", "Renamed", "{fileID: 0}"),
        modification(ITEM, 20, "m_LocalPosition.x", "7", "{fileID: 0}"),
        modification(ITEM, 30, "speed", "3", "{fileID: 0}"),
        modification(ITEM, 30, "targets.Array.size", "2", "{fileID: 0}"),
        modification(ITEM, 30, "targets.Array.data[1]", "", "{fileID: 900}"),
    ]
    .concat();
    let removed = format!("\n    - {{fileID: 40, guid: {ITEM}, type: 3}}");
    [
        game_object(900, "Holder", &[910, 920]),
        transform(910, 900, 0.0, 0),
        behaviour(
            920,
            900,
            &format!(
                "  item: {{fileID: 950}}\n  child: {{fileID: 960}}\n  template: {{fileID: 10, guid: {ITEM}, type: 3}}\n  config: {{fileID: 11400000, guid: {CONFIG}, type: 2}}\n  sprite: {{fileID: 21300000, guid: {TEXTURE}, type: 3}}\n  builtin: {{fileID: 10754, guid: 0000000000000000f000000000000000, type: 0}}\n  gone: {{fileID: 5, guid: 77777777777777777777777777777777, type: 3}}\n  dangling: {{fileID: 12345}}\n"
            ),
        ),
        instance(940, ITEM, 910, &overrides, &removed),
        format!("--- !u!114 &950 stripped\nMonoBehaviour:\n  m_CorrespondingSourceObject: {{fileID: 30, guid: {ITEM}, type: 3}}\n  m_PrefabInstance: {{fileID: 940}}\n  m_PrefabAsset: {{fileID: 0}}\n"),
        format!("--- !u!4 &960 stripped\nTransform:\n  m_CorrespondingSourceObject: {{fileID: 60, guid: {ITEM}, type: 3}}\n  m_PrefabInstance: {{fileID: 940}}\n  m_PrefabAsset: {{fileID: 0}}\n"),
        "--- !u!1660057539 &9223372036854775807\nSceneRoots:\n  m_Roots:\n  - {fileID: 910}\n".into(),
    ]
    .concat()
}

#[test]
fn prefab_instances_expand_with_overrides_parenting_and_removed_components() {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Main.unity", &modern_scene()).unwrap();
    let holder = scene.transform_of(by_name(&scene, "Holder")).unwrap();
    assert_eq!(scene.roots(), [holder]);
    let item = by_name(&scene, "Renamed");
    let item_transform = scene.transform_of(item).unwrap();
    let view = scene.transform(item_transform).unwrap();
    assert_eq!(view.parent, Some(holder));
    assert!(near(view.local_position, [7.0, 0.0, 0.0]));
    assert_eq!(scene.transform(holder).unwrap().children, [item_transform]);
    let classes: Vec<_> = scene
        .game_object(item)
        .unwrap()
        .components
        .iter()
        .map(|id| scene.object(*id).unwrap().class_id)
        .collect();
    assert_eq!(classes, [4, 114], "the removed collider is gone");
    let behaviour = component(&scene, item, 114);
    let fields = &scene.object(behaviour).unwrap().fields;
    assert_eq!(
        fields.get("speed").unwrap().as_scalar().unwrap().text(),
        "3"
    );
    let targets = fields.get("targets").unwrap().as_sequence().unwrap();
    assert_eq!(targets.len(), 2);
    let holder_object = by_name(&scene, "Holder");
    assert_eq!(
        targets[1].as_reference(),
        Some(Reference::Object(holder_object))
    );
    assert_eq!(
        reference(&scene, behaviour, &["self"]),
        Reference::Object(item_transform)
    );
    assert_eq!(
        scene.script(behaviour).unwrap(),
        external(SCRIPT, 11_500_000)
    );
}

fn near(actual: [f64; 3], expected: [f64; 3]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| (a - e).abs() < 1e-12)
}

fn external(guid: &str, file_id: i64) -> Reference {
    Reference::External {
        guid: guid.parse().unwrap(),
        file_id,
    }
}

#[test]
fn references_resolve_to_stripped_objects_and_templates() {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Main.unity", &modern_scene()).unwrap();
    let item = by_name(&scene, "Renamed");
    let holder = component(&scene, by_name(&scene, "Holder"), 114);
    let at = |key: &str| reference(&scene, holder, &[key]);
    assert_eq!(at("item"), Reference::Object(component(&scene, item, 114)));
    let child = scene.transform_of(by_name(&scene, "Child")).unwrap();
    assert_eq!(at("child"), Reference::Object(child));
    let template_guid: Guid = ITEM.parse().unwrap();
    let Reference::Object(template) = at("template") else {
        panic!("template reference")
    };
    let template_object = scene.object(template).unwrap();
    assert_eq!(template_object.origin, Origin::Template(template_guid));
    assert_eq!(scene.game_object(template).unwrap().name, "Item");
    let template_root = scene.template(template_guid).unwrap();
    assert!(near(
        scene.transform(template_root).unwrap().local_position,
        [1.0, 0.0, 0.0]
    ));
    assert!(!scene.roots().contains(&template_root));
}

#[test]
fn references_to_assets_load_them_and_unknown_targets_are_reported() {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Main.unity", &modern_scene()).unwrap();
    let holder = component(&scene, by_name(&scene, "Holder"), 114);
    let at = |key: &str| reference(&scene, holder, &[key]);
    let Reference::Object(config) = at("config") else {
        panic!("config reference")
    };
    let config = scene.object(config).unwrap();
    assert_eq!(config.origin, Origin::Asset(CONFIG.parse().unwrap()));
    assert_eq!(
        config
            .fields
            .get("gridSize")
            .unwrap()
            .get("x")
            .unwrap()
            .as_scalar()
            .unwrap()
            .as_i64(),
        Some(40)
    );
    assert_eq!(at("sprite"), external(TEXTURE, 21_300_000));
    assert_eq!(
        at("builtin"),
        Reference::Package(PackageAsset::Material(PackageMaterial::SpritesDefault))
    );
    let missing: Vec<_> = scene
        .missing_references()
        .into_iter()
        .map(|(_, r)| r)
        .collect();
    assert_eq!(
        missing,
        [
            Reference::Missing {
                guid: Some("77777777777777777777777777777777".parse().unwrap()),
                file_id: 5
            },
            Reference::Missing {
                guid: None,
                file_id: 12345
            },
        ]
    );
}

/// Unity 2017 prefab file: a `Prefab` root document and objects linked through `m_PrefabInternal`.
fn legacy_prefab() -> String {
    format!(
        "--- !u!1001 &100100000\nPrefab:\n  m_Modification:\n    m_TransformParent: {{fileID: 0}}\n    m_Modifications: []\n    m_RemovedComponents: []\n  m_ParentPrefab: {{fileID: 0}}\n  m_RootGameObject: {{fileID: 1}}\n  m_IsPrefabParent: 1\n{}{}",
        game_object(1, "Paddle", &[4]).replace("GameObject:\n", "GameObject:\n  m_PrefabParentObject: {fileID: 0}\n  m_PrefabInternal: {fileID: 100100000}\n"),
        transform(4, 1, 0.0, 0) + "  m_RootOrder: 0\n",
    )
}

fn legacy_instance(id: u64, name: &str, order: u64) -> String {
    let overrides = [
        modification(LEGACY, 1, "m_Name", name, "{fileID: 0}"),
        modification(LEGACY, 4, "m_RootOrder", &order.to_string(), "{fileID: 0}"),
    ]
    .concat()
    .replace("type: 3", "type: 2");
    format!(
        "--- !u!1001 &{id}\nPrefab:\n  m_Modification:\n    m_TransformParent: {{fileID: 0}}\n    m_Modifications:\n{overrides}    m_RemovedComponents: []\n  m_ParentPrefab: {{fileID: 100100000, guid: {LEGACY}, type: 2}}\n  m_IsPrefabParent: 0\n"
    )
}

#[test]
fn legacy_prefabs_expand_through_parent_prefab_links_in_root_order() {
    let fixture = fixture();
    fixture.asset(
        "Assets/Paddle.prefab",
        LEGACY,
        "NativeFormatImporter",
        legacy_prefab(),
    );
    let scene = [
        legacy_instance(300, "Right", 2),
        legacy_instance(200, "Left", 1),
        game_object(100, "Camera", &[110]),
        transform(110, 100, 0.0, 0) + "  m_RootOrder: 0\n",
        format!("--- !u!4 &400 stripped\nTransform:\n  m_PrefabParentObject: {{fileID: 4, guid: {LEGACY}, type: 2}}\n  m_PrefabInternal: {{fileID: 300}}\n"),
        behaviour(500, 100, "  right: {fileID: 400}\n  prefab: {fileID: 100100000, guid: 66666666666666666666666666666666, type: 2}\n"),
    ]
    .concat();
    let scene = import(&fixture, "Assets/Legacy.unity", &scene).unwrap();
    let names: Vec<_> = scene
        .roots()
        .iter()
        .map(|id| {
            let object = scene.transform(*id).unwrap().game_object.unwrap();
            scene.game_object(object).unwrap().name
        })
        .collect();
    assert_eq!(names, ["Camera", "Left", "Right"]);
    let right = scene.transform_of(by_name(&scene, "Right")).unwrap();
    let behaviour = component(&scene, by_name(&scene, "Camera"), 114);
    assert_eq!(
        reference(&scene, behaviour, &["right"]),
        Reference::Object(right)
    );
    let template = scene.template(LEGACY.parse().unwrap()).unwrap();
    assert_eq!(
        reference(&scene, behaviour, &["prefab"]),
        Reference::Object(template)
    );
}

#[test]
fn nested_prefab_objects_use_unity_derived_file_ids() {
    let fixture = fixture();
    let outer = [
        game_object(1, "Outer", &[2]),
        transform(2, 1, 0.0, 0),
        instance(500, ITEM, 2, "", " []"),
    ]
    .concat();
    fixture.asset("Assets/Outer.prefab", OUTER, "PrefabImporter", &outer);
    // Instance 500 (0x1f4) of the item's behaviour 30 (0x1e).
    let derived = (0x1f4_i64 ^ 0x1e) & i64::MAX;
    let scene = [
        instance(700, OUTER, 0, "", " []"),
        format!("--- !u!114 &800 stripped\nMonoBehaviour:\n  m_CorrespondingSourceObject: {{fileID: {derived}, guid: {OUTER}, type: 3}}\n  m_PrefabInstance: {{fileID: 700}}\n  m_PrefabAsset: {{fileID: 0}}\n"),
        game_object(900, "Reader", &[910]),
        behaviour(910, 900, "  nested: {fileID: 800}\n"),
    ]
    .concat();
    let scene = import(&fixture, "Assets/Nested.unity", &scene).unwrap();
    let item = by_name(&scene, "Item");
    let item_transform = scene.transform_of(item).unwrap();
    let outer_transform = scene.transform_of(by_name(&scene, "Outer")).unwrap();
    assert_eq!(
        scene.transform(item_transform).unwrap().parent,
        Some(outer_transform)
    );
    let reader = component(&scene, by_name(&scene, "Reader"), 114);
    assert_eq!(
        reference(&scene, reader, &["nested"]),
        Reference::Object(component(&scene, item, 114))
    );
}

fn error(result: Result<Scene, SceneError>) -> String {
    result.unwrap_err().to_string()
}

#[test]
fn invalid_prefab_structure_fails_with_its_file_and_object() {
    let fixture = fixture();
    let a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    fixture.asset(
        "Assets/A.prefab",
        a,
        "PrefabImporter",
        instance(1, b, 0, "", " []"),
    );
    fixture.asset(
        "Assets/B.prefab",
        b,
        "PrefabImporter",
        instance(1, a, 0, "", " []"),
    );
    let message = error(import(
        &fixture,
        "Assets/Cycle.unity",
        &instance(5, a, 0, "", " []"),
    ));
    assert!(message.contains("nests itself"), "{message}");
    let outside = modification(OUTER, 10, "m_Name", "x", "{fileID: 0}");
    let message = error(import(
        &fixture,
        "Assets/Outside.unity",
        &instance(5, ITEM, 0, &outside, " []"),
    ));
    assert!(
        message.contains("Outside.unity object 5") && message.contains("outside prefab"),
        "{message}"
    );
    let unknown = modification(ITEM, 999, "m_Name", "x", "{fileID: 0}");
    let message = error(import(
        &fixture,
        "Assets/Unknown.unity",
        &instance(5, ITEM, 0, &unknown, " []"),
    ));
    assert!(message.contains("missing from prefab"), "{message}");
    let message = error(import(
        &fixture,
        "Assets/Gone.unity",
        &instance(5, "99999999999999999999999999999999", 0, "", " []"),
    ));
    assert!(message.contains("not in the project"), "{message}");
}

#[test]
fn unsupported_or_inconsistent_instances_fail_with_their_cause() {
    let fixture = fixture();
    let a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let removed = instance(5, ITEM, 0, "", " []").replace(
        "m_RemovedGameObjects: []",
        "m_RemovedGameObjects:\n    - {fileID: 50}",
    );
    let message = error(import(&fixture, "Assets/Removed.unity", &removed));
    assert!(message.contains("not supported"), "{message}");
    let stripped = "--- !u!4 &9 stripped\nTransform:\n  m_PrefabInstance: {fileID: 123}\n  m_CorrespondingSourceObject: {fileID: 20}\n";
    let message = error(import(&fixture, "Assets/Stripped.unity", stripped));
    assert!(message.contains("unknown prefab instance"), "{message}");
    let two_roots = [
        game_object(1, "A", &[2]),
        transform(2, 1, 0.0, 0),
        game_object(3, "B", &[4]),
        transform(4, 3, 0.0, 0),
    ]
    .concat();
    fixture.asset("Assets/Two.prefab", a, "PrefabImporter", &two_roots);
    let message = error(import(
        &fixture,
        "Assets/Two.unity",
        &instance(5, a, 0, "", " []"),
    ));
    assert!(
        message.contains("more than one root transform"),
        "{message}"
    );
    let bad_path = modification(ITEM, 20, "m_LocalPosition.x.Array.size", "1", "{fileID: 0}");
    let message = error(import(
        &fixture,
        "Assets/Path.unity",
        &instance(5, ITEM, 0, &bad_path, " []"),
    ));
    assert!(
        message.contains("override `m_LocalPosition.x.Array.size`"),
        "{message}"
    );
}

#[test]
fn every_value_kind_converts() {
    let fixture = fixture();
    let scene = import(&fixture, "Assets/Main.unity", &modern_scene()).unwrap();
    let holder = component(&scene, by_name(&scene, "Holder"), 114);
    let fields = &scene.object(holder).unwrap().fields;
    assert!(matches!(fields, Value::Record(_)));
    assert!(
        fields.get("m_PrefabInstance").is_none(),
        "bookkeeping is consumed"
    );
    assert_eq!(
        fields
            .get("m_Enabled")
            .unwrap()
            .as_scalar()
            .unwrap()
            .as_bool(),
        Some(true)
    );
}
