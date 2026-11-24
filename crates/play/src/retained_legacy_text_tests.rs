use super::ui_tests::{fixture_turn, play_fixture, rgba};

/// The text of a legacy `UnityEngine.UI.Text` is built by the engine's `TextGenerator` from a font's
/// glyph atlas and reaches the overlay as a mesh through `CanvasRenderer`. The fixture does what
/// that component does for "AB" at 20 pixels, centred in a 64 by 64 canvas, with the synthetic font:
/// each glyph is a 10 by 14 right triangle with its right angle at the bottom left and an advance of
/// 12 pixels, so the glyphs cover x 20 to 30 and 32 to 42 and y 26 to 40 (rows counted from the bottom
/// of the 64 by 64 window, as the other UI tests count them).
#[test]
fn retained_legacy_text_draws_generated_glyph_quads() {
    let _turn = fixture_turn();
    let run = play_fixture("legacy-text.fixture");
    assert_eq!(run.ui_updates, 1, "unchanged frames send no UI update");
    assert_eq!(run.captures.len(), 2);
    for image in &run.captures {
        assert_glyphs(image);
    }
    assert_eq!(run.captures[0].pixels, run.captures[1].pixels);
}

fn assert_glyphs(image: &crate::driver::Capture) {
    let background = rgba(image, 2, 2).to_vec();
    assert_ne!(
        background,
        [0, 255, 0, 255],
        "the background is not the text colour"
    );
    for (left, name) in [(20, "first"), (32, "second")] {
        for y in 26..30 {
            for x in left + 1..left + 4 {
                assert_eq!(
                    rgba(image, x, y),
                    [0, 255, 0, 255],
                    "{name} glyph lower left {x},{y}"
                );
            }
        }
        for y in 36..40 {
            for x in left + 7..left + 10 {
                assert_eq!(
                    rgba(image, x, y),
                    background,
                    "{name} glyph upper right {x},{y}"
                );
            }
        }
    }
    for y in 26..40 {
        for x in 30..32 {
            assert_eq!(rgba(image, x, y), background, "between the glyphs {x},{y}");
        }
    }
    let green = image
        .pixels
        .chunks_exact(4)
        .filter(|p| *p == [0, 255, 0, 255])
        .count();
    assert!(
        green > 2 * 30 && green < 2 * 10 * 14,
        "two triangles of glyph pixels: {green}"
    );
}
