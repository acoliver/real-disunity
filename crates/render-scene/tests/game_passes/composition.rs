//! The strip game in a forward frame: its passes sit before Opaque, share the frame's depth, and
//! the stock object composes with them by depth.
use super::fixture::{Rig, SIZE, STOCK_TEXEL, quad_scene};
use super::strip::{LIGHT, LIGHT_AFTER, STRIP, StripGame, knobs};
use std::sync::Arc;
use worldrunner_render_gpu::tolerance_test_support::{assert_code_within, encoded_steps};
use worldrunner_render_scene::compositor::GamePasses;

/// The strip in clip space: from the middle of the screen to 80% of the way right, full height.
const RECT: [f32; 4] = [0.0, -1.0, 0.8, 1.0];
const FRONT: f32 = 0.2;
const BEHIND: f32 = 0.8;

fn rig(depth: f32) -> (Rig, super::strip::Shared) {
    let knobs = knobs(RECT, depth);
    let game = Arc::new(StripGame {
        knobs: knobs.clone(),
    });
    let rig = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(game);
    });
    (rig, knobs)
}

/// What the light pass writes where the strip is: the red of the first target, the green of the
/// second, the blue 128/255 of the third (sRGB encoded by the frame's format), and an opaque
/// alpha because the fourth holds the identifier the light pass checks for.
fn assert_strip(pixel: [u8; 4], context: &str) {
    assert_code_within(
        pixel[0],
        encoded_steps(1.0, true),
        0.0,
        &format!("{context} red"),
    );
    assert_code_within(
        pixel[1],
        encoded_steps(1.0, true),
        0.0,
        &format!("{context} green"),
    );
    assert_code_within(
        pixel[2],
        encoded_steps(128.0 / 255.0, true),
        0.0,
        &format!("{context} blue"),
    );
    assert_eq!(pixel[3], 255, "{context} alpha");
}

/// Every pixel of a block of columns and rows well inside one region of the frame.
fn region(image: &[[u8; 4]], columns: std::ops::RangeInclusive<usize>) -> Vec<[u8; 4]> {
    (14..=34)
        .flat_map(|row| {
            columns
                .clone()
                .map(move |column| Rig::pixel(image, column, row))
        })
        .collect()
}

fn stock_frame() -> Vec<[u8; 4]> {
    Rig::new(&quad_scene(), SIZE, |_| {}).render().unwrap()
}

#[test]
fn the_stock_frame_is_the_quad_on_black() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let image = stock_frame();
    for pixel in region(&image, 18..=28) {
        assert_eq!(pixel[3], 255);
        assert_code_within(
            pixel[0],
            f64::from(STOCK_TEXEL[0]),
            0.0,
            "stock red (an sRGB texel through an sRGB frame returns its own code)",
        );
    }
    for pixel in region(&image, 2..=8) {
        assert_eq!(pixel, [0, 0, 0, 255], "outside the quad");
    }
}

#[test]
fn a_strip_in_front_of_the_stock_object_hides_it_where_they_overlap() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let reference = stock_frame();
    let (mut rig, knobs) = rig(FRONT);
    let image = rig.render().unwrap();
    for pixel in region(&image, 36..=46) {
        assert_strip(pixel, "overlap, strip in front");
    }
    for pixel in region(&image, 52..=55) {
        assert_strip(pixel, "strip alone");
    }
    assert_eq!(
        region(&image, 18..=28),
        region(&reference, 18..=28),
        "quad alone"
    );
    assert_eq!(region(&image, 2..=8), region(&reference, 2..=8), "neither");
    assert_eq!(knobs.lock().unwrap().counts.recorded, vec![STRIP, LIGHT]);
}

#[test]
fn a_strip_behind_the_stock_object_shows_through_only_where_the_object_is_absent() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let reference = stock_frame();
    let (mut rig, _) = rig(BEHIND);
    let image = rig.render().unwrap();
    assert_eq!(
        region(&image, 36..=46),
        region(&reference, 36..=46),
        "overlap, strip behind"
    );
    for pixel in region(&image, 52..=55) {
        assert_strip(pixel, "strip alone");
    }
    assert_eq!(
        region(&image, 18..=28),
        region(&reference, 18..=28),
        "quad alone"
    );
}

#[test]
fn a_light_pass_after_opaque_draws_over_the_stock_object_whatever_the_strip_depth() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let reference = stock_frame();
    let (mut rig, knobs) = rig(BEHIND);
    knobs.lock().unwrap().light_after_opaque = true;
    let image = rig.render().unwrap();
    // The light pass runs once the stock object is in the frame and has no depth test, so the
    // strip shows over the object even though the strip's depth is behind it.
    for pixel in region(&image, 36..=46) {
        assert_strip(pixel, "overlap, light after Opaque");
    }
    for pixel in region(&image, 52..=55) {
        assert_strip(pixel, "strip alone");
    }
    assert_eq!(
        region(&image, 18..=28),
        region(&reference, 18..=28),
        "quad alone"
    );
    assert_eq!(region(&image, 2..=8), region(&reference, 2..=8), "neither");
    assert_eq!(
        knobs.lock().unwrap().counts.recorded,
        vec![STRIP, LIGHT_AFTER]
    );
}

#[test]
fn a_strip_that_leaves_the_frame_depth_alone_lets_the_stock_object_cover_it() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let reference = stock_frame();
    let (mut rig, knobs) = rig(FRONT);
    knobs.lock().unwrap().without_depth = true;
    let image = rig.render().unwrap();
    // Without the frame's depth nothing of the strip is tested or kept, so Opaque draws over the
    // lit strip wherever the object is.
    assert_eq!(
        region(&image, 36..=46),
        region(&reference, 36..=46),
        "overlap, no strip depth"
    );
    for pixel in region(&image, 52..=55) {
        assert_strip(pixel, "strip alone");
    }
}
