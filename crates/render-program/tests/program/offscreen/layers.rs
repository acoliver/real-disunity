//! Property layers: the first layer that names a value wins, highest first.

use worldrunner_render_program::ids::{MaterialId, RendererId};
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::unity::offscreen::frame::OffscreenFrame;

use super::camera::fixture;
use super::rig::{BLUE, GREEN, Rig, layer, pixel};

const TARGET: worldrunner_render_program::ids::TextureId =
    worldrunner_render_program::ids::TextureId(10);
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];

/// The tint of the whole 8x8 target for `frame`.
fn tint(rig: &mut Rig, frame: &OffscreenFrame) -> [u8; 4] {
    let image = rig.run(frame, &[(TARGET, 0, 0)]).unwrap().remove(0);
    pixel(&image, 8, (4, 4))
}

#[test]
fn a_value_comes_from_the_highest_layer_that_names_it() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, 1 << 1);
    let renderer = frame.renderers.get_mut(&RendererId(1)).unwrap();
    renderer.per_draw = layer(&[
        ("_Place", [1.0, 1.0, 0.0, 0.0]),
        ("_Tint", [0.0, 1.0, 0.0, 1.0]),
    ]);
    renderer.block = layer(&[("_Tint", [0.0, 0.0, 1.0, 1.0])]);
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties =
        layer(&[("_Tint", [1.0, 0.0, 0.0, 1.0])]);
    frame.globals = layer(&[("_Tint", [1.0, 0.0, 1.0, 1.0])]);
    assert_eq!(
        tint(&mut rig, &frame),
        GREEN,
        "the caller's per-draw values"
    );
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw =
        layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    assert_eq!(
        tint(&mut rig, &frame),
        BLUE,
        "the renderer's property block"
    );
    frame.renderers.get_mut(&RendererId(1)).unwrap().block = PropertyLayer::default();
    assert_eq!(tint(&mut rig, &frame), RED, "the material");
    frame.materials.get_mut(&MaterialId(1)).unwrap().properties = PropertyLayer::default();
    assert_eq!(tint(&mut rig, &frame), MAGENTA, "the globals");
}

#[test]
fn the_camera_values_sit_below_the_renderers_per_draw_values() {
    let mut rig = Rig::new();
    let mut frame = fixture(&mut rig, 1 << 1);
    // The camera supplies `_Place` and the renderer does not: the camera's value is used. When
    // the renderer names it too, the renderer's wins.
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw = PropertyLayer::default();
    frame
        .cameras
        .get_mut(&worldrunner_render_program::ids::CameraId(1))
        .unwrap()
        .values = layer(&[("_Place", [1.0, 1.0, 0.0, 0.0])]);
    assert_eq!(tint(&mut rig, &frame), RED);
    frame.renderers.get_mut(&RendererId(1)).unwrap().per_draw =
        layer(&[("_Place", [0.0, 0.0, 0.0, 0.0])]);
    assert_eq!(
        tint(&mut rig, &frame),
        [255, 255, 0, 255],
        "collapsed: only the clear color"
    );
}
