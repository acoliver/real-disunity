//! A player scene's canvases as plan components.

use std::path::Path;

use serde_json::json;
use worldrunner_unity_import::{plan::PlanComponent, player_plan::plan_player_scene};
use worldrunner_unity_serialized::{
    ClassId, SerializedType,
    object::PPtr,
    script_data::ScriptLayouts,
    write::{FileContents, ObjectData, ObjectWriter},
};

const TYPES: [ClassId; 4] = [
    ClassId::GAME_OBJECT,
    ClassId::TRANSFORM,
    ClassId::CANVAS,
    ClassId::CAMERA,
];

fn ptr(path_id: i64) -> PPtr {
    PPtr {
        file_id: 0,
        path_id,
    }
}

fn class(class_id: ClassId) -> SerializedType {
    SerializedType {
        class_id,
        stripped: false,
        script_type_index: -1,
        script_id: None,
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

fn game_object(name: &str, components: &[PPtr]) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptrs(components).unwrap().u32(8).string(name).unwrap();
    w.u16(0).u8(1);
    w.finish()
}

fn transform(game_object: i64) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(game_object)).floats(&[0.0, 0.0, 0.0, 1.0]);
    w.floats(&[0.0; 3]).floats(&[1.0; 3]);
    w.pptrs(&[]).unwrap().pptr(ptr(0));
    w.finish()
}

/// What a serialized canvas holds that the plan carries.
struct CanvasFields {
    render_mode: i32,
    camera: PPtr,
    plane_distance: f32,
    pixel_perfect: bool,
    sorting_layer_id: i32,
    sorting_order: i16,
    target_display: u8,
}

fn canvas_bytes(canvas: &CanvasFields) -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(2)).u8(1).align().i32(canvas.render_mode);
    w.pptr(canvas.camera).f32(canvas.plane_distance);
    w.bytes(&[u8::from(canvas.pixel_perfect), 1, 0, 0]);
    w.f32(0.0).u8(0).align().i32(0).i32(0);
    w.i32(canvas.sorting_layer_id).i16(canvas.sorting_order);
    w.u8(canvas.target_display);
    w.finish()
}

fn camera_bytes() -> Vec<u8> {
    let mut w = ObjectWriter::new(false);
    w.pptr(ptr(0)).u8(1).align().u32(1);
    w.floats(&[0.25, 0.5, 0.75, 1.0]).i32(1);
    w.i32(2).i32(200).floats(&[0.005, 16.0, 10.0, 50.0]).i32(5);
    w.floats(&[2.0, 11.0, 0.25, 0.5, 36.0, 24.0, 0.1, -0.1]);
    w.floats(&[0.125, 0.25, 0.5, 0.75]);
    w.floats(&[0.5, 500.0, 50.0]).u8(0).align().f32(5.0);
    w.f32(2.0).u32(0xffff_fffb).i32(-1).pptr(ptr(0)).i32(1).i32(2);
    w.bytes(&[1, 0, 1, 0, 1]).align().f32(0.022).f32(0.064);
    w.finish()
}

fn plan_canvases(canvas: &CanvasFields, with_camera: bool) -> Vec<PlanComponent> {
    let dir = tempfile::tempdir().unwrap();
    write_scene(dir.path(), canvas, with_camera);
    let layouts = ScriptLayouts::new(Vec::new());
    let mut plan = plan_player_scene(dir.path(), "level0", &layouts).unwrap();
    plan.objects.remove(0).components
}

fn write_scene(dir: &Path, canvas: &CanvasFields, with_camera: bool) {
    let components = if with_camera {
        vec![ptr(3), ptr(4), ptr(5)]
    } else {
        vec![ptr(3), ptr(4)]
    };
    let mut objects = vec![
        object(2, ClassId::GAME_OBJECT, game_object("Canvas", &components)),
        object(3, ClassId::TRANSFORM, transform(2)),
        object(4, ClassId::CANVAS, canvas_bytes(canvas)),
    ];
    if with_camera {
        objects.push(object(5, ClassId::CAMERA, camera_bytes()));
    }
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
    std::fs::write(dir.join("level0"), contents.write().unwrap()).unwrap();
}

fn overlay() -> CanvasFields {
    CanvasFields {
        render_mode: 0,
        camera: ptr(0),
        plane_distance: 100.0,
        pixel_perfect: false,
        sorting_layer_id: 0,
        sorting_order: 0,
        target_display: 0,
    }
}

#[test]
fn plans_a_canvas_with_its_mode_sorting_and_pixel_settings() {
    let canvas = CanvasFields {
        render_mode: 2,
        plane_distance: 12.5,
        pixel_perfect: true,
        sorting_layer_id: 934_472_021,
        sorting_order: -7,
        ..overlay()
    };
    let components = plan_canvases(&canvas, false);
    assert_eq!(
        serde_json::to_value(&components).unwrap(),
        json!([{
            "type": "Canvas", "render_mode": "WorldSpace", "camera": null,
            "plane_distance": 12.5, "pixel_perfect": true,
            "sorting_layer_id": 934_472_021, "sorting_order": -7, "target_display": 0
        }])
    );
}

#[test]
fn a_screen_space_camera_canvas_names_the_planned_camera_component() {
    let canvas = CanvasFields {
        render_mode: 1,
        camera: ptr(5),
        ..overlay()
    };
    let components = plan_canvases(&canvas, true);
    let [
        PlanComponent::Canvas { camera, .. },
        PlanComponent::Camera(_),
    ] = &components[..]
    else {
        panic!("the object has a canvas and a camera: {components:?}")
    };
    assert_eq!(*camera, Some([0, 1]));
}

#[test]
fn every_render_mode_maps_to_its_plan_name() {
    for (mode, name) in [
        (0, "ScreenSpaceOverlay"),
        (1, "ScreenSpaceCamera"),
        (2, "WorldSpace"),
    ] {
        let components = plan_canvases(
            &CanvasFields {
                render_mode: mode,
                ..overlay()
            },
            false,
        );
        let planned = serde_json::to_value(&components[0]).unwrap();
        assert_eq!(planned["render_mode"], name, "mode {mode}");
    }
}
