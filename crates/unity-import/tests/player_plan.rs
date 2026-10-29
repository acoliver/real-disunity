use std::path::Path;

use serde_json::{Value as Json, json};
use worldrunner_unity_import::{
    Camera, CameraOutput, ClearFlags, Light3D, plan::PlanComponent, player_plan::plan_player_scene,
};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::{FieldType, Primitive, ScriptField, ScriptLayouts, ScriptType},
    write::{FileContents, ObjectData, ObjectWriter},
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

const TYPES: [ClassId; 8] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::MONO_BEHAVIOUR,
    ClassId::MONO_SCRIPT,
    ClassId::LIGHT,
    ClassId::TAG_MANAGER,
    ClassId::CAMERA,
    ClassId::AUDIO_LISTENER,
];

fn class(class_id: ClassId) -> SerializedType {
    SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: (class_id == ClassId::MONO_BEHAVIOUR).then_some([7; 16]),
        old_type_hash: [0; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    }
}

fn object(path_id: i64, class_id: ClassId, data: Vec<u8>) -> ObjectData {
    let type_index = TYPES.iter().position(|&c| c == class_id).unwrap();
    ObjectData {
        path_id,
        type_index: i32::try_from(type_index).unwrap(),
        data,
    }
}

fn game_object(name: &str, tag: u16, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components)
        .unwrap()
        .u32(8)
        .string(name)
        .unwrap()
        .u16(tag)
        .u8(1);
    w.finish()
}

fn transform(game_object: i64, father: PPtr, children: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[1.0, 2.0, 3.0]).floats(&[1.0; 3]);
    w.pptrs(children).unwrap().pptr(father);
    w.finish()
}

fn script() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.string("Spinner").unwrap().i32(0).bytes(&[0; 16]);
    w.string("Spinner")
        .unwrap()
        .string("Example")
        .unwrap()
        .string("Example.Game.dll")
        .unwrap();
    w.finish()
}

/// A `Spinner` on object 2: a speed, then references to the child object, its transform,
/// the root's light, and an object in another file.
fn behaviour() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 2))
        .u8(0)
        .align()
        .pptr(ptr(0, 5))
        .string("")
        .unwrap()
        .f32(2.5);
    for target in [ptr(0, 6), ptr(0, 7), ptr(0, 8), ptr(1, 99), ptr(0, 0)] {
        w.pptr(target);
    }
    w.finish()
}

fn layouts() -> ScriptLayouts {
    let field = |name: &str, ty| ScriptField {
        name: name.to_owned(),
        ty,
    };
    let object = || FieldType::Object("UnityEngine.Object".to_owned());
    ScriptLayouts::new(vec![ScriptType {
        key: "Example.Game:Example.Spinner".to_owned(),
        value_type: false,
        fields: vec![
            field("speed", FieldType::Primitive(Primitive::F32)),
            field("target", object()),
            field("pivot", object()),
            field("light", object()),
            field("material", object()),
            field("nothing", object()),
        ],
    }])
}

fn write_file(dir: &Path, name: &str, objects: Vec<ObjectData>) {
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects,
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join(name), contents.write().unwrap()).unwrap();
}

fn write_scene(dir: &Path) {
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: TYPES.into_iter().map(class).collect(),
        objects: vec![
            object(
                2,
                ClassId::GAME_OBJECT,
                game_object("Root", 5, &[ptr(0, 3), ptr(0, 8), ptr(0, 4)]),
            ),
            object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[ptr(0, 7)])),
            object(4, ClassId::MONO_BEHAVIOUR, behaviour()),
            object(5, ClassId::MONO_SCRIPT, script()),
            object(
                6,
                ClassId::GAME_OBJECT,
                game_object("Child", 0, &[ptr(0, 7)]),
            ),
            object(7, ClassId::TRANSFORM, transform(6, ptr(0, 3), &[])),
            object(8, ClassId::AUDIO_LISTENER, Vec::new()),
        ],
        scripts: Vec::new(),
        externals: vec![External {
            guid: [0; 16],
            kind: 0,
            path: "sharedassets0.assets".into(),
        }],
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(dir.join("level0"), contents.write().unwrap()).unwrap();
}

#[test]
fn plans_objects_scripts_and_references_and_lists_what_it_leaves_out() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let plan = plan_player_scene(dir.path(), "level0", &layouts()).unwrap();
    let names: Vec<_> = plan.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Root", "Child"]);
    let root = &plan.objects[0];
    assert_eq!((root.tag.as_str(), root.layer), ("MainCamera", 8));
    assert_eq!(plan.objects[1].parent, Some(0));
    // Positions widen from f32 exactly.
    assert_eq!(
        root.position.map(f64::to_bits),
        [1.0, 2.0, 3.0].map(f64::to_bits)
    );
    let [PlanComponent::Unsupported { class }, script] = &root.components[..] else {
        panic!(
            "the root has a listener and a script: {:?}",
            root.components
        )
    };
    assert_eq!(class, "AudioListener");
    let PlanComponent::Script {
        class,
        assembly,
        enabled,
        fields,
        ..
    } = script
    else {
        panic!("the second component is the script")
    };
    assert_eq!(
        (class.as_str(), assembly.as_deref(), *enabled),
        ("Example.Spinner", Some("Example.Game"), false)
    );
    assert_eq!(
        *fields,
        json!({
            "speed": "2.5",
            "target": {"$object": 1},
            "pivot": {"$transform": 1},
            "light": Json::Null,
            "material": Json::Null,
            "nothing": Json::Null,
        })
    );
    assert_eq!(
        plan.differences,
        [
            "Root.Example.Spinner.light: references a AudioListener component, which the engine does not build",
            "Root.Example.Spinner.material: references object 99 of file 1, which the plan does not hold",
        ]
    );
}

#[test]
fn names_the_script_whose_fields_do_not_decode() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let mut short = layouts();
    short.types[0].fields.pop();
    let short = ScriptLayouts::new(short.types);
    let error = plan_player_scene(dir.path(), "level0", &short).unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains("Root: script Example.Game:Example.Spinner"),
        "{text}"
    );
}

/// A `Spinner` on object 2 holding each kind of value the planner writes as text, then the
/// reference registry its `[SerializeReference]` field needs.
fn value_behaviour() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 2))
        .u8(1)
        .align()
        .pptr(ptr(0, 5))
        .string("")
        .unwrap();
    w.u64(u64::MAX)
        .u16(0xE9)
        .align()
        .f32(f32::NAN)
        .f32(f32::NEG_INFINITY);
    w.i32(2).i32(1).i32(-2).align();
    w.i64(-2).i32(2).i32(1).i64(-2);
    for _ in 0..3 {
        w.string("").unwrap();
    }
    w.finish()
}

fn value_layouts() -> ScriptLayouts {
    let field = |name: &str, ty| ScriptField {
        name: name.to_owned(),
        ty,
    };
    ScriptLayouts::new(vec![
        ScriptType {
            key: "Example.Game:Example.Spinner".to_owned(),
            value_type: false,
            fields: vec![
                field("big", FieldType::Primitive(Primitive::U64)),
                field("letter", FieldType::Primitive(Primitive::Char)),
                field("nan", FieldType::Primitive(Primitive::F32)),
                field("low", FieldType::Primitive(Primitive::F32)),
                field("pair", FieldType::Composite(1)),
                field("shape", FieldType::Reference("Example.IShape".to_owned())),
            ],
        },
        ScriptType {
            key: "Example.Game:Example.Pair".to_owned(),
            value_type: true,
            fields: vec![field(
                "values",
                FieldType::Array(Box::new(FieldType::Primitive(Primitive::I32))),
            )],
        },
    ])
}

#[test]
fn maps_user_tags_and_writes_every_value_kind_as_the_loader_reads_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut tags = ObjectWriter::new(false);
    tags.i32(1).string("Enemy").unwrap();
    for _ in 0..3 {
        tags.i32(0);
    }
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::TAG_MANAGER, tags.finish())],
    );
    write_file(
        dir.path(),
        "level0",
        vec![
            object(
                2,
                ClassId::GAME_OBJECT,
                game_object("Root", 20_000, &[ptr(0, 3), ptr(0, 4)]),
            ),
            object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[ptr(0, 7)])),
            object(4, ClassId::MONO_BEHAVIOUR, value_behaviour()),
            object(5, ClassId::MONO_SCRIPT, script()),
            object(
                6,
                ClassId::GAME_OBJECT,
                game_object("Child", 20_005, &[ptr(0, 7)]),
            ),
            object(7, ClassId::TRANSFORM, transform(6, ptr(0, 3), &[])),
        ],
    );
    let plan = plan_player_scene(dir.path(), "level0", &value_layouts()).unwrap();
    let tags: Vec<_> = plan.objects.iter().map(|o| o.tag.as_str()).collect();
    assert_eq!(tags, ["Enemy", "Untagged"]);
    let [PlanComponent::Script { fields, .. }] = &plan.objects[0].components[..] else {
        panic!("the root has one script: {:?}", plan.objects[0].components)
    };
    assert_eq!(
        *fields,
        json!({
            "big": "18446744073709551615",
            "letter": "233",
            "nan": "NaN",
            "low": "-Infinity",
            "pair": {"values": ["1", "-2"]},
            "shape": Json::Null,
        })
    );
    assert_eq!(
        plan.differences,
        [
            "Root.Example.Spinner.shape: [SerializeReference] values are not built",
            "Child: tag 20005 is not defined",
        ]
    );
}

/// A camera as the player serializes it: perspective, with the viewport, mask, display and
/// flags below, a clear mode, a target texture and a projection matrix mode.
fn camera_bytes(clear_flags: u32, target_texture: PPtr, matrix_mode: i32) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 0))
        .u8(1)
        .align()
        .u32(clear_flags)
        .floats(&[0.25, 0.5, 0.75, 1.0])
        .i32(matrix_mode);
    w.i32(2).i32(200).floats(&[0.005, 16.0, 10.0, 50.0]).i32(5);
    w.floats(&[2.0, 11.0, 0.25, 0.5, 36.0, 24.0, 0.1, -0.1]);
    w.floats(&[0.125, 0.25, 0.5, 0.75])
        .floats(&[0.5, 500.0, 50.0])
        .u8(0)
        .align()
        .f32(5.0);
    w.f32(2.0)
        .u32(0xffff_fffb)
        .i32(-1)
        .pptr(target_texture)
        .i32(1)
        .i32(2);
    w.bytes(&[1, 0, 1, 0, 1]).align().f32(0.022).f32(0.064);
    w.finish()
}

/// A light with the given kind and shadow mode; a spot of 30 and 20 degrees, cookie size
/// 4 by 4, color temperature 5000 K, baked.
fn light_bytes(kind: i32, shadows: i32, cookie: PPtr, flare: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 0))
        .u8(1)
        .align()
        .i32(kind)
        .floats(&[1.0, 0.5, 0.25, 1.0]);
    w.floats(&[2.0, 12.0, 30.0, 20.0]).floats(&[4.0, 4.0]);
    w.i32(shadows)
        .i32(-1)
        .i32(512)
        .floats(&[0.75, 0.0625, 0.5, 0.25]);
    w.floats(&[0.0; 16]).u8(0).align();
    w.pptr(cookie).u8(0).align();
    w.i32(-1).i32(0).i32(2).i32(0).u8(0).align();
    w.pptr(flare)
        .i32(0)
        .u32(0xff)
        .u32(3)
        .i32(2)
        .i32(0)
        .floats(&[1.0, 1.0])
        .f32(1.5);
    w.f32(5_000.0)
        .u8(1)
        .align()
        .floats(&[0.0; 4])
        .bytes(&[0, 0, 0])
        .align();
    w.i32(0).f32(1.0).u8(0).align();
    w.finish()
}

/// One object `Rig` with a camera and a light.
fn write_rig(dir: &Path, camera: Vec<u8>, light: Vec<u8>) {
    write_file(
        dir,
        "level0",
        vec![
            object(
                2,
                ClassId::GAME_OBJECT,
                game_object("Rig", 5, &[ptr(0, 3), ptr(0, 4), ptr(0, 5)]),
            ),
            object(3, ClassId::TRANSFORM, transform(2, ptr(0, 0), &[])),
            object(4, ClassId::CAMERA, camera),
            object(5, ClassId::LIGHT, light),
        ],
    );
}

fn plan_rig(
    camera: Vec<u8>,
    light: Vec<u8>,
) -> Result<worldrunner_unity_import::ScenePlan, String> {
    let dir = tempfile::tempdir().unwrap();
    write_rig(dir.path(), camera, light);
    plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new()))
        .map_err(|error| error.to_string())
}

fn null() -> PPtr {
    ptr(0, 0)
}

#[test]
fn plans_a_perspective_camera_and_a_light_with_their_decoded_values() {
    let plan = plan_rig(
        camera_bytes(2, null(), 1),
        light_bytes(0, 2, null(), null()),
    )
    .unwrap();
    let [
        PlanComponent::Camera(camera),
        PlanComponent::Light { enabled, light },
    ] = &plan.objects[0].components[..]
    else {
        panic!(
            "the rig has a camera and a light: {:?}",
            plan.objects[0].components
        )
    };
    let expected = Camera {
        enabled: true,
        clear: ClearFlags::SolidColor,
        background: [0.25, 0.5, 0.75, 1.0],
        orthographic_size: 5.0,
        near: 0.5,
        far: 500.0,
        depth: 2.0,
        orthographic: false,
        field_of_view: 50.0,
        culling_mask: 0xffff_fffb,
        rect: [0.125, 0.25, 0.5, 0.75],
        target_display: 1,
        output: CameraOutput {
            allow_hdr: true,
            allow_msaa: false,
            allow_dynamic_resolution: true,
        },
        use_occlusion_culling: true,
    };
    assert_eq!(*camera, expected);
    let expected = Light3D {
        kind: "Spot".to_owned(),
        color: [1.0, 0.5, 0.25, 1.0],
        intensity: 2.0,
        range: 12.0,
        spot_angle: 30.0,
        inner_spot_angle: 20.0,
        shadows: "Soft".to_owned(),
        shadow_strength: 0.75,
        shadow_bias: 0.0625,
        shadow_normal_bias: 0.5,
        shadow_near_plane: 0.25,
        culling_mask: 0xff,
        rendering_layer_mask: 3,
        bounce_intensity: 1.5,
        color_temperature: 5_000.0,
        use_color_temperature: true,
        cookie_size: 4.0,
        lightmap_bake_type: 2,
    };
    assert!(*enabled);
    assert_eq!(*light, expected);
    assert_eq!(plan.differences, Vec::<String>::new());
}

#[test]
fn camera_clear_flags_map_to_the_plan_and_skybox_does_not_fail() {
    let expected = [
        (1, ClearFlags::Skybox),
        (2, ClearFlags::SolidColor),
        (3, ClearFlags::Depth),
        (4, ClearFlags::Nothing),
    ];
    for (serialized, clear) in expected {
        let plan = plan_rig(
            camera_bytes(serialized, null(), 1),
            light_bytes(1, 0, null(), null()),
        )
        .unwrap();
        let PlanComponent::Camera(camera) = &plan.objects[0].components[0] else {
            panic!("the first component is the camera")
        };
        assert_eq!(camera.clear, clear, "m_ClearFlags {serialized}");
        assert_eq!(plan.differences, Vec::<String>::new());
    }
    let error = plan_rig(
        camera_bytes(9, null(), 1),
        light_bytes(1, 0, null(), null()),
    )
    .unwrap_err();
    assert!(
        error.contains("Rig") && error.contains("clear flags 9"),
        "{error}"
    );
}

#[test]
fn a_target_texture_is_left_out_and_listed() {
    let plan = plan_rig(
        camera_bytes(1, ptr(1, 77), 1),
        light_bytes(1, 0, null(), null()),
    )
    .unwrap();
    assert_eq!(
        plan.differences,
        ["Rig: camera target texture is not built"]
    );
}

#[test]
fn a_projection_matrix_mode_other_than_1_is_listed() {
    for mode in [0, 2, 7] {
        let plan = plan_rig(
            camera_bytes(1, null(), mode),
            light_bytes(1, 0, null(), null()),
        )
        .unwrap();
        assert_eq!(
            plan.differences,
            [format!(
                "Rig: camera projection matrix mode {mode} is not applied"
            )]
        );
    }
    let standard = plan_rig(
        camera_bytes(1, null(), 1),
        light_bytes(1, 0, null(), null()),
    )
    .unwrap();
    assert!(
        standard.differences.is_empty(),
        "{:?}",
        standard.differences
    );
}

#[test]
fn light_kinds_and_shadow_modes_map_to_their_names_and_unknown_ones_fail() {
    let kinds = ["Spot", "Directional", "Point", "Rectangle", "Disc"];
    for (serialized, name) in (0..).zip(kinds) {
        let plan = plan_rig(
            camera_bytes(1, null(), 1),
            light_bytes(serialized, serialized % 3, null(), null()),
        )
        .unwrap();
        let PlanComponent::Light { light, .. } = &plan.objects[0].components[1] else {
            panic!("the second component is the light")
        };
        let shadows = ["None", "Hard", "Soft"][usize::try_from(serialized % 3).unwrap()];
        assert_eq!(
            (light.kind.as_str(), light.shadows.as_str()),
            (name, shadows)
        );
    }
    let kind = plan_rig(
        camera_bytes(1, null(), 1),
        light_bytes(7, 0, null(), null()),
    )
    .unwrap_err();
    assert!(
        kind.contains("Rig") && kind.contains("light type 7"),
        "{kind}"
    );
    let shadows = plan_rig(
        camera_bytes(1, null(), 1),
        light_bytes(0, 5, null(), null()),
    )
    .unwrap_err();
    assert!(
        shadows.contains("Rig") && shadows.contains("shadow mode 5"),
        "{shadows}"
    );
}

#[test]
fn a_light_cookie_and_flare_are_left_out_and_listed() {
    let plan = plan_rig(
        camera_bytes(1, null(), 1),
        light_bytes(0, 0, ptr(1, 5), ptr(1, 6)),
    )
    .unwrap();
    assert_eq!(
        plan.differences,
        [
            "Rig: light cookie is not built",
            "Rig: light flare is not built"
        ]
    );
}

#[test]
fn carries_the_layer_names_of_the_tag_manager_by_layer_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut manager = ObjectWriter::new(false);
    manager.i32(0).i32(10);
    for layer in 0..10 {
        let name = match layer {
            5 => "UI",
            9 => "Lights",
            _ => "",
        };
        manager.string(name).unwrap();
    }
    manager.i32(0).i32(0);
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![object(1, ClassId::TAG_MANAGER, manager.finish())],
    );
    write_file(dir.path(), "level0", vec![]);
    let plan = plan_player_scene(dir.path(), "level0", &ScriptLayouts::new(Vec::new())).unwrap();
    assert_eq!(plan.layers.len(), 32);
    assert_eq!(plan.layers[9], "Lights");
    assert_eq!(plan.layers[5], "UI");
    assert!(plan.layers[10..].iter().all(String::is_empty));
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["layers"][9], "Lights");
}

#[test]
fn leaves_the_layers_out_of_the_plan_without_a_tag_manager() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path());
    let plan = plan_player_scene(dir.path(), "level0", &layouts()).unwrap();
    assert!(plan.layers.is_empty());
    assert!(serde_json::to_value(&plan).unwrap().get("layers").is_none());
}
