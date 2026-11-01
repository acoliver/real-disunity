//! Text on a canvas a UI camera renders keeps its world points in the UI plan, and the
//! presentation draws it on the screen where that camera puts it: the same place as text on an
//! overlay canvas that fills the screen the same way.
#![cfg(feature = "ui")]
mod ui_font;

use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::presentation::extraction::{
    SpriteQuad, SpriteTextures, append_ui_from, extract_world,
};
use worldrunner_world_simulation::ui::contribution::extract_ui;
use worldrunner_world_simulation::ui::{
    Canvas, CanvasMode, FontBook, RectTransform, TextRun, UiLimits,
};
use worldrunner_world_simulation::{ComponentDesc, ObjectDesc, ObjectHandle, World, WorldSettings};

const VIEWPORT: [u32; 2] = [800, 600];

/// The UI plan's overlay flag for the text, and the glyph quads the presentation draws, for one
/// canvas in `mode` presented by a camera that also renders a camera canvas.
fn drawn(mode: impl FnOnce(ObjectHandle) -> CanvasMode) -> (bool, Vec<SpriteQuad>) {
    presented(mode, true).unwrap()
}

/// As `drawn`; without `presenting`, the camera object has no camera, so none presents the frame.
fn presented(
    mode: impl FnOnce(ObjectHandle) -> CanvasMode,
    presenting: bool,
) -> Result<(bool, Vec<SpriteQuad>), worldrunner_camera::ViewError> {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    let font = fonts.register(ui_font::font()).unwrap();
    let camera = world.create_object(ObjectDesc::default()).unwrap();
    if presenting {
        world
            .add_component(camera, ComponentDesc::Camera(Camera::default()))
            .unwrap();
    }
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
    let ui = extract_ui(&world, &fonts, VIEWPORT).unwrap();
    let overlay = ui.texts[0].overlay;
    let mut plan = extract_world(&world, &SpriteTextures::default(), VIEWPORT).unwrap();
    append_ui_from(&mut plan, ui, 1)?;
    Ok((overlay, plan.quads))
}

fn camera_canvas(camera: ObjectHandle) -> CanvasMode {
    CanvasMode::ScreenSpaceCamera {
        camera,
        orthographic_size: 3.,
        plane_distance: 10.,
    }
}

#[test]
fn with_no_presented_camera_camera_canvas_text_keeps_its_world_points() {
    let (_, quads) = presented(camera_canvas, false).expect("nothing places it on the screen");
    assert!(!quads.is_empty());
    assert!(quads.iter().all(|quad| !quad.overlay));
}

#[test]
fn text_on_a_camera_canvas_is_drawn_where_overlay_text_is() {
    let (overlay_flag, overlay) = drawn(|_| CanvasMode::ScreenSpaceOverlay);
    let (camera_flag, camera) = drawn(camera_canvas);
    assert!(overlay_flag, "overlay text is in pixels in the UI plan");
    assert!(
        !camera_flag,
        "camera canvas text keeps its world points in the UI plan"
    );
    assert_eq!(camera.len(), overlay.len());
    assert!(!camera.is_empty());
    for (drawn, expected) in camera.iter().zip(&overlay) {
        assert!(drawn.overlay);
        for (a, e) in drawn.corners.iter().zip(expected.corners) {
            let close = a.iter().zip(e).all(|(a, e)| (a - e).abs() < 1e-3);
            assert!(close, "{a:?} against {e:?}");
        }
    }
}
