//! A player plans the axes of the `InputManager` in its `globalgamemanagers`, in order.

use worldrunner_unity_import::{
    AxisKind, InputAxis,
    player_resources::{ResourcesPlan, plan_player_resources},
};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

struct Axis {
    name: &'static str,
    positive: &'static str,
    alt_positive: &'static str,
    kind: i32,
    snap: bool,
}

const SUBMIT: Axis = Axis {
    name: "Submit",
    positive: "return",
    alt_positive: "space",
    kind: 0,
    snap: true,
};

const MOUSE_X: Axis = Axis {
    name: "Mouse X",
    positive: "",
    alt_positive: "",
    kind: 1,
    snap: false,
};

const PAD: Axis = Axis {
    name: "Pad",
    positive: "",
    alt_positive: "",
    kind: 2,
    snap: false,
};

fn write_axis(w: &mut ObjectWriter, axis: &Axis) {
    w.string(axis.name).unwrap();
    w.string("").unwrap().string("").unwrap();
    w.string("").unwrap().string(axis.positive).unwrap();
    w.string("").unwrap().string(axis.alt_positive).unwrap();
    w.f32(1000.0).f32(0.001).f32(0.25);
    w.u8(u8::from(axis.snap)).u8(1).align();
    w.i32(axis.kind).i32(0).i32(0);
}

fn input_manager(axes: &[Axis]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.i32(i32::try_from(axes.len()).unwrap());
    for axis in axes {
        write_axis(&mut w, axis);
    }
    w.u8(0).align();
    w.finish()
}

/// Plans a player whose `globalgamemanagers` holds an `InputManager` of `axes`, or none.
fn planned(axes: Option<&[Axis]>) -> ResourcesPlan {
    let dir = tempfile::tempdir().unwrap();
    let types = [ClassId::INPUT_MANAGER].map(|class_id| SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: None,
        old_type_hash: [0; 16],
        type_tree: None,
        dependencies: Vec::new(),
        script_class: None,
    });
    let objects = axes
        .map(|axes| ObjectData {
            path_id: 1,
            type_index: 0,
            data: input_manager(axes),
        })
        .into_iter()
        .collect();
    let contents = FileContents {
        version: 22,
        big_endian: false,
        unity_version: "6000.3.15f1".into(),
        target_platform: 19,
        type_trees: false,
        types: types.to_vec(),
        objects,
        scripts: Vec::new(),
        externals: Vec::new(),
        ref_types: Vec::new(),
        user_information: String::new(),
    };
    std::fs::write(
        dir.path().join("globalgamemanagers"),
        contents.write().unwrap(),
    )
    .unwrap();
    plan_player_resources(
        dir.path(),
        &ScriptLayouts::new(Vec::new()),
        &dir.path().join("out"),
    )
    .unwrap()
}

#[test]
fn a_player_plans_its_input_axes_in_manager_order() {
    let plan = planned(Some(&[SUBMIT, MOUSE_X, PAD, SUBMIT]));
    let names: Vec<_> = plan.axes.iter().map(|axis| axis.name.as_str()).collect();
    assert_eq!(names, ["Submit", "Mouse X", "Pad", "Submit"]);
    let kinds: Vec<_> = plan.axes.iter().map(|axis| axis.kind).collect();
    assert_eq!(
        kinds,
        [
            AxisKind::KeyOrMouseButton,
            AxisKind::MouseMovement,
            AxisKind::JoystickAxis,
            AxisKind::KeyOrMouseButton
        ]
    );
    assert_eq!(
        plan.axes[0],
        InputAxis {
            name: "Submit".into(),
            kind: AxisKind::KeyOrMouseButton,
            negative: String::new(),
            positive: "return".into(),
            alt_negative: String::new(),
            alt_positive: "space".into(),
            gravity: 1000.0,
            dead: f64::from(0.001_f32),
            sensitivity: 0.25,
            snap: true,
            invert: true,
        }
    );
    assert_eq!(plan.axes[1].kind, AxisKind::MouseMovement);
    assert!(!plan.axes[1].snap);
    assert!(plan.differences.is_empty(), "{:?}", plan.differences);
}

#[test]
fn a_player_without_an_input_manager_plans_no_axes() {
    assert!(planned(None).axes.is_empty());
}

#[test]
fn an_axis_of_an_unknown_device_is_left_out_with_a_note() {
    let odd = Axis {
        kind: 9,
        name: "Odd",
        ..SUBMIT
    };
    let plan = planned(Some(&[SUBMIT, odd]));
    assert_eq!(plan.axes.len(), 1);
    assert_eq!(
        plan.differences,
        ["input axis Odd has the unknown device kind 9; it is not planned"]
    );
}

#[test]
fn the_plan_serializes_axes_as_the_managed_loader_reads_them() {
    let plan = planned(Some(&[MOUSE_X]));
    let json = serde_json::to_value(&plan).unwrap();
    assert_eq!(json["axes"][0]["name"], "Mouse X");
    assert_eq!(json["axes"][0]["kind"], "MouseMovement");
    assert_eq!(json["axes"][0]["alt_positive"], "");
}
