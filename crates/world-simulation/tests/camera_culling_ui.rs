//! A camera's culling mask hides the text of world-space canvases on layers it leaves out, and
//! never hides an overlay canvas, which no camera draws.
#![cfg(feature = "ui")]
mod ui_font;

use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::ui::contribution::extract_ui;
use worldrunner_world_simulation::ui::{
    Canvas, CanvasMode, FontBook, RectTransform, TextRun, UiLimits,
};
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, World, WorldSettings};

fn text_world(mode: CanvasMode, mask: u32) -> usize {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let camera = world.create_object(ObjectDesc::default()).unwrap();
    let camera_desc = Camera {
        culling_mask: mask,
        ..Default::default()
    };
    world
        .add_component(camera, ComponentDesc::Camera(camera_desc))
        .unwrap();
    let canvas = world.create_object(ObjectDesc::default()).unwrap();
    world.set_layer(canvas, 5).unwrap();
    for desc in [
        ComponentDesc::RectTransform(RectTransform::default()),
        ComponentDesc::Canvas(Canvas {
            mode,
            ..Default::default()
        }),
        ComponentDesc::Text(TextRun {
            text: "A".into(),
            ..TextRun::new(font)
        }),
    ] {
        world.add_component(canvas, desc).unwrap();
    }
    extract_ui(&world, &fonts, [800, 600]).unwrap().texts.len()
}

#[test]
fn a_world_canvas_follows_the_culling_mask() {
    assert_eq!(text_world(CanvasMode::WorldSpace, 1 << 5), 1);
    assert_eq!(text_world(CanvasMode::WorldSpace, 1), 0);
    assert_eq!(text_world(CanvasMode::WorldSpace, u32::MAX), 1);
}

#[test]
fn an_overlay_canvas_ignores_the_culling_mask() {
    assert_eq!(text_world(CanvasMode::ScreenSpaceOverlay, 1), 1);
}
