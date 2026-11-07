//! A pixel perfect canvas moves its text so the first glyph starts on a whole screen pixel,
//! whatever other canvases the world holds.
#![cfg(feature = "ui")]
mod ui_font;

use glam::Vec2;
use worldrunner_world_simulation::ui::contribution::extract_ui;
use worldrunner_world_simulation::ui::{
    Canvas, CanvasMode, FontBook, RectTransform, TextRun, UiLimits,
};
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, ParentSpace, World, WorldSettings};

const VIEWPORT: [u32; 2] = [800, 600];

/// The screen pixel of the first glyph's top-left corner for a text on a canvas that has
/// `pixel_perfect` set, in a world where an earlier canvas has it clear.
fn first_glyph_pixel(pixel_perfect: bool) -> Vec2 {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let earlier = world.create_object(ObjectDesc::default()).unwrap();
    for desc in [
        ComponentDesc::RectTransform(RectTransform::default()),
        ComponentDesc::Canvas(Canvas::default()),
    ] {
        world.add_component(earlier, desc).unwrap();
    }
    let canvas = world.create_object(ObjectDesc::default()).unwrap();
    for desc in [
        ComponentDesc::RectTransform(RectTransform::default()),
        ComponentDesc::Canvas(Canvas {
            mode: CanvasMode::ScreenSpaceOverlay,
            pixel_perfect,
            ..Default::default()
        }),
    ] {
        world.add_component(canvas, desc).unwrap();
    }
    let label = world.create_object(ObjectDesc::default()).unwrap();
    world
        .set_parent(label, Some(canvas), ParentSpace::KeepLocal)
        .unwrap();
    for desc in [
        ComponentDesc::RectTransform(RectTransform {
            anchored_position: Vec2::new(0.37, 0.61),
            ..Default::default()
        }),
        ComponentDesc::Text(TextRun {
            text: "AB".into(),
            // Keep the snapping probe away from the rectangle's clip edges.
            horizontal: worldrunner_world_simulation::ui::HorizontalAlign::Center,
            ..TextRun::new(font)
        }),
    ] {
        world.add_component(label, desc).unwrap();
    }
    let ui = extract_ui(&world, &fonts, VIEWPORT).unwrap();
    let pixels = ui
        .layout
        .rect(label)
        .and_then(|rect| rect.pixels_from_world)
        .expect("an overlay canvas has a pixel map");
    let corner = ui.texts[0].quads[0].corners[3];
    pixels.transform_point3(corner.into()).truncate()
}

fn on_whole_pixel(pixel: Vec2) -> bool {
    pixel
        .to_array()
        .iter()
        .all(|v| (v - v.round()).abs() < 1e-3)
}

#[test]
fn a_pixel_perfect_canvas_starts_its_text_on_a_whole_pixel() {
    assert!(on_whole_pixel(first_glyph_pixel(true)));
}

#[test]
fn a_canvas_that_is_not_pixel_perfect_leaves_its_text_where_it_falls() {
    assert!(!on_whole_pixel(first_glyph_pixel(false)));
}
