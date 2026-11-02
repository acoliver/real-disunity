//! A font a resource names is planned with the byte range of its font file, so the engine can load
//! a face from it.
use std::path::Path;

use worldrunner_unity_import::player_resources::{ResourceAsset, plan_player_resources};
use worldrunner_unity_serialized::{
    ClassId, External, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 2] = [ClassId::RESOURCE_MANAGER, ClassId::FONT];
const FONT_FILE: &[u8] = b"\x00\x01\x00\x00 not a real font, only bytes to find again";

fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 1,
        path_id,
    }
}

fn write_file(dir: &Path, name: &str, objects: Vec<(i64, usize, Vec<u8>)>, externals: &[&str]) {
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
        objects: objects
            .into_iter()
            .map(|(path_id, type_index, data)| ObjectData {
                path_id,
                type_index: i32::try_from(type_index).unwrap(),
                data,
            })
            .collect(),
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
    w.i32(2);
    for (path, target) in [("fonts/sans", ptr(2)), ("fonts/empty", ptr(3))] {
        w.string(path).unwrap().pptr(target);
    }
    w.i32(0);
    w.finish()
}

fn font(name: &str, file: &[u8]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    let none = PPtr {
        file_id: 0,
        path_id: 0,
    };
    w.string(name)
        .unwrap()
        .f32(1.)
        .pptr(none)
        .f32(16.)
        .pptr(none);
    w.i32(0).f32(0.).i32(0).i32(0).i32(-2).i32(0).i32(0);
    w.f32(1.).byte_array(file).unwrap();
    w.f32(12.).f32(-3.).u32(0).i32(0).pptrs(&[]).unwrap();
    w.i32(0).bytes(&[0, 0]);
    w.finish()
}

#[test]
fn a_resource_font_is_planned_with_the_range_of_its_file() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "globalgamemanagers",
        vec![(1, 0, resource_manager())],
        &["resources.assets"],
    );
    write_file(
        dir.path(),
        "resources.assets",
        vec![(2, 1, font("Sans", FONT_FILE)), (3, 1, font("Empty", &[]))],
        &[],
    );
    let plan = plan_player_resources(
        dir.path(),
        &ScriptLayouts::new(Vec::new()),
        &dir.path().join("out"),
    )
    .unwrap();
    let entries: Vec<_> = plan
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.class.as_str(), e.asset))
        .collect();
    assert_eq!(
        entries,
        [
            ("fonts/sans", "Font", Some(0)),
            ("fonts/empty", "Font", Some(1))
        ]
    );
    let ResourceAsset::Font {
        name,
        file,
        offset,
        length,
    } = &plan.assets[0]
    else {
        panic!("the first asset is a font: {:?}", plan.assets[0])
    };
    let bytes = std::fs::read(file).unwrap();
    let start = usize::try_from(*offset).unwrap();
    let end = start + usize::try_from(*length).unwrap();
    assert_eq!((name.as_str(), &bytes[start..end]), ("Sans", FONT_FILE));
    let ResourceAsset::Font { name, length, .. } = &plan.assets[1] else {
        panic!("the second asset is a font: {:?}", plan.assets[1])
    };
    assert_eq!((name.as_str(), *length), ("Empty", 0));
}
