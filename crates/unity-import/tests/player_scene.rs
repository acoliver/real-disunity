use std::path::Path;
use worldrunner_unity_import::{PlayerError, player_objects::OpenFile};

use worldrunner_unity_import::{player_objects::decode_census, player_scene::read_scene_hierarchy};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    write::{FileContents, ObjectData, ObjectWriter},
};

fn ptr(file_id: i32, path_id: i64) -> PPtr {
    PPtr { file_id, path_id }
}

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

fn file(objects: Vec<ObjectData>) -> FileContents {
    FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: [
            ClassId::GAME_OBJECT,
            ClassId::TRANSFORM,
            ClassId::MONO_BEHAVIOUR,
            ClassId::MONO_SCRIPT,
        ]
        .into_iter()
        .map(class)
        .collect(),
        objects,
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    }
}

fn object(path_id: i64, type_index: i32, data: Vec<u8>) -> ObjectData {
    ObjectData {
        path_id,
        type_index,
        data,
    }
}

fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components)
        .unwrap()
        .u32(0)
        .string(name)
        .unwrap()
        .u16(0)
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
        .string("Example.Game")
        .unwrap();
    w.finish()
}

fn behaviour(script: PPtr) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0, 2))
        .u8(1)
        .align()
        .pptr(script)
        .string("")
        .unwrap()
        .u32(42);
    w.finish()
}

fn scene(script: PPtr) -> FileContents {
    file(vec![
        object(2, 0, game_object("Root", &[ptr(0, 3), ptr(0, 4)])),
        object(3, 1, transform(2, ptr(0, 0), &[ptr(0, 7)])),
        object(4, 2, behaviour(script)),
        object(6, 0, game_object("Child", &[ptr(0, 7)])),
        object(7, 1, transform(6, ptr(0, 3), &[])),
    ])
}

fn write_scene(dir: &Path, scene: &FileContents) {
    std::fs::write(dir.join("level0"), scene.write().unwrap()).unwrap();
}

#[test]
fn resolves_same_file_scripts_and_preserves_hierarchy() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = scene(ptr(0, 5));
    data.objects.push(object(5, 3, script()));
    write_scene(dir.path(), &data);
    let roots = read_scene_hierarchy(dir.path(), "level0").unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].name, "Root");
    assert_eq!(roots[0].components, ["Transform", "Example.Spinner"]);
    assert_eq!(
        roots[0].local_position.map(f32::to_bits),
        [1.0_f32, 2.0, 3.0].map(f32::to_bits)
    );
    assert_eq!(roots[0].children[0].name, "Child");
}

#[test]
fn resolves_external_scripts() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = scene(ptr(1, 5));
    data.externals.push(External {
        guid: [0; 16],
        kind: 0,
        path: "globalgamemanagers.assets".into(),
    });
    write_scene(dir.path(), &data);
    std::fs::write(
        dir.path().join("globalgamemanagers.assets"),
        file(vec![object(5, 3, script())]).write().unwrap(),
    )
    .unwrap();
    let roots = read_scene_hierarchy(dir.path(), "level0").unwrap();
    assert_eq!(roots[0].components[1], "Example.Spinner");
}

#[test]
fn rejects_invalid_external_indices() {
    for file_id in [-1, i32::MIN, 1, i32::MAX] {
        let dir = tempfile::tempdir().unwrap();
        write_scene(dir.path(), &scene(ptr(file_id, 5)));
        let error = read_scene_hierarchy(dir.path(), "level0").unwrap_err();
        let message = error.to_string();
        assert!(message.contains(&format!("file {file_id}")), "{message}");
    }
}

#[test]
fn rejects_cross_file_children_instead_of_reading_a_local_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = scene(ptr(0, 5));
    data.objects.push(object(5, 3, script()));
    data.objects[1].data = transform(2, ptr(0, 0), &[ptr(1, 7)]);
    write_scene(dir.path(), &data);
    let error = read_scene_hierarchy(dir.path(), "level0").unwrap_err();
    assert!(error.to_string().contains("file 1"), "{error}");
}

#[test]
fn rejects_a_rootless_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = scene(ptr(0, 5));
    data.objects[1].data = transform(2, ptr(0, 7), &[ptr(0, 7)]);
    data.objects[4].data = transform(6, ptr(0, 3), &[ptr(0, 3)]);
    write_scene(dir.path(), &data);
    let error = read_scene_hierarchy(dir.path(), "level0").unwrap_err();
    assert!(error.to_string().contains("hierarchy"), "{error}");
}

#[test]
fn rejects_duplicate_children_and_inconsistent_parents() {
    for children in [vec![ptr(0, 7), ptr(0, 7)], Vec::new()] {
        let dir = tempfile::tempdir().unwrap();
        let mut data = scene(ptr(0, 5));
        data.objects.push(object(5, 3, script()));
        data.objects[1].data = transform(2, ptr(0, 0), &children);
        write_scene(dir.path(), &data);
        let error = read_scene_hierarchy(dir.path(), "level0").unwrap_err();
        assert!(error.to_string().contains("hierarchy"), "{error}");
    }
}

#[test]
fn census_counts_failures_and_keeps_examples_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let objects = (1..=8).map(|id| object(id, 0, vec![0; 1])).collect();
    write_scene(dir.path(), &file(objects));
    let counts = decode_census(&[dir.path().join("level0")]).unwrap();
    let entry = &counts[&ClassId::GAME_OBJECT];
    assert_eq!(
        (entry.decoded, entry.failed, entry.examples.len()),
        (0, 8, 5)
    );
}

#[test]
fn object_reads_report_missing_ids_and_actual_file_bounds() {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path(), &scene(ptr(0, 5)));
    let mut open = OpenFile::open(&dir.path().join("level0")).unwrap();
    let go: worldrunner_unity_serialized::classes::GameObject = open.decode(2).unwrap();
    assert_eq!(go.name, "Root");
    let error = open
        .decode::<worldrunner_unity_serialized::classes::GameObject>(999)
        .unwrap_err();
    assert!(matches!(
        error,
        PlayerError::MissingObject { path_id: 999, .. }
    ));
    let original = *open.file.object(2).unwrap();
    for (offset, size) in [(u64::MAX, original.size), (original.offset, u32::MAX)] {
        let mut object = original;
        object.offset = offset;
        object.size = size;
        let error = open.bytes(&object).unwrap_err();
        assert!(
            matches!(
                error,
                PlayerError::Serialized {
                    source: worldrunner_unity_serialized::SerializedError::ObjectOutOfBounds { .. },
                    ..
                }
            ),
            "{error}"
        );
    }
}

#[test]
fn rejects_excessive_hierarchy_depth_before_recursive_construction() {
    let dir = tempfile::tempdir().unwrap();
    let mut objects = Vec::new();
    for i in 0..300 {
        let go = i * 2 + 1;
        let tr = go + 1;
        objects.push(object(go, 0, game_object("Node", &[ptr(0, tr)])));
        let children = if i == 299 {
            vec![]
        } else {
            vec![ptr(0, tr + 2)]
        };
        objects.push(object(
            tr,
            1,
            transform(go, ptr(0, if i == 0 { 0 } else { tr - 2 }), &children),
        ));
    }
    write_scene(dir.path(), &file(objects));
    let error = read_scene_hierarchy(dir.path(), "level0").unwrap_err();
    assert!(error.to_string().contains("depth"), "{error}");
}

#[test]
fn resolves_windows_external_names_and_rejects_unsafe_names() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("scripts.assets"),
        file(vec![object(5, 3, script())]).write().unwrap(),
    )
    .unwrap();
    for name in ["Library\\scripts.assets", "archive:/CAB/scripts.assets"] {
        let mut data = scene(ptr(1, 5));
        data.externals.push(External {
            guid: [0; 16],
            kind: 0,
            path: name.into(),
        });
        write_scene(dir.path(), &data);
        let roots = read_scene_hierarchy(dir.path(), "level0").unwrap();
        assert_eq!(roots[0].components[1], "Example.Spinner");
    }
    for name in ["..", ".", "", "C:secret.assets", "Library/"] {
        let mut data = scene(ptr(1, 5));
        data.externals.push(External {
            guid: [0; 16],
            kind: 0,
            path: name.into(),
        });
        write_scene(dir.path(), &data);
        let error = read_scene_hierarchy(dir.path(), "level0").unwrap_err();
        assert!(
            matches!(error, PlayerError::UnresolvedReference { .. }),
            "{error}"
        );
    }
}

#[test]
fn decode_cli_fails_on_invalid_objects_and_census_ignores_unknown_classes() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = file(vec![
        object(1, 0, vec![0]),
        object(2, 0, game_object("Valid", &[])),
    ]);
    data.types.push(class(ClassId::AUDIO_CLIP));
    data.objects.push(object(3, 4, vec![0xff]));
    write_scene(dir.path(), &data);
    std::fs::write(
        dir.path().join("level1"),
        file(vec![object(4, 0, game_object("Second", &[]))])
            .write()
            .unwrap(),
    )
    .unwrap();
    let census = decode_census(&[dir.path().join("level0"), dir.path().join("level1")]).unwrap();
    assert_eq!(census.len(), 1);
    assert_eq!(
        (
            census[&ClassId::GAME_OBJECT].decoded,
            census[&ClassId::GAME_OBJECT].failed
        ),
        (2, 1)
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_unity-import"))
        .arg("player-decode")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("1 objects failed to decode"));
}

#[test]
fn player_object_describes_supported_objects_and_rejects_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let mut data = scene(ptr(0, 5));
    data.types.push(class(ClassId::AUDIO_CLIP));
    data.objects.push(object(9, 4, vec![0; 4]));
    write_scene(dir.path(), &data);
    let path = dir.path().join("level0");
    let run = |id: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_unity-import"))
            .args(["player-object".as_ref(), path.as_os_str(), id.as_ref()])
            .output()
            .unwrap()
    };
    let described = run("2");
    assert!(described.status.success());
    let text = String::from_utf8_lossy(&described.stdout);
    assert!(
        text.contains("GameObject {") && text.contains("\"Root\""),
        "{text}"
    );
    for (id, message) in [
        ("9", "no object layout is implemented for AudioClip"),
        ("999", "no object with path ID 999"),
    ] {
        let failed = run(id);
        assert!(!failed.status.success());
        let stderr = String::from_utf8_lossy(&failed.stderr);
        assert!(stderr.contains(message), "{stderr}");
    }
}
