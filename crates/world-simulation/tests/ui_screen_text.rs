//! Text on a canvas a UI camera renders is drawn on the screen where that camera puts it, the
//! same pixels as on an overlay canvas that fills the screen the same way.
#![cfg(feature = "ui")]
mod ui_font;

use worldrunner_world_simulation::ui::contribution::{UiTextContribution, extract_ui};
use worldrunner_world_simulation::ui::{
    Canvas, CanvasMode, FontBook, RectTransform, TextRun, UiLimits,
};
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, World, WorldSettings};

/// The text of one canvas in `mode`, given the camera object a camera canvas uses.
fn text(
    mode: impl FnOnce(worldrunner_world_simulation::ObjectHandle) -> CanvasMode,
) -> UiTextContribution {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let camera = world.create_object(ObjectDesc::default()).unwrap();
    let canvas = world.create_object(ObjectDesc::default()).unwrap();
    for desc in [
        ComponentDesc::RectTransform(RectTransform::default()),
        ComponentDesc::Canvas(Canvas {
            mode: mode(camera),
            ..Default::default()
        }),
        ComponentDesc::Text(TextRun {
            text: "AB".into(),
            ..TextRun::new(font)
        }),
    ] {
        world.add_component(canvas, desc).unwrap();
    }
    let mut texts = extract_ui(&world, &fonts, [800, 600]).unwrap().texts;
    assert_eq!(texts.len(), 1);
    texts.remove(0)
}

#[test]
fn text_on_a_camera_canvas_is_drawn_at_the_overlay_pixels() {
    let overlay = text(|_| CanvasMode::ScreenSpaceOverlay);
    let camera = text(|camera| CanvasMode::ScreenSpaceCamera {
        camera,
        orthographic_size: 3.,
        plane_distance: 10.,
    });
    assert!(overlay.overlay && camera.overlay);
    assert_eq!(camera.quads.len(), overlay.quads.len());
    assert!(!camera.quads.is_empty());
    for (drawn, expected) in camera.quads.iter().zip(&overlay.quads) {
        for (a, e) in drawn.corners.iter().zip(expected.corners) {
            assert!(
                (a[0] - e[0]).abs() < 0.01 && (a[1] - e[1]).abs() < 0.01,
                "{a:?} against {e:?}"
            );
        }
    }
}
