//! Sprite physics shapes: generated rectangles for opaque sprites, and refusal otherwise.
#[path = "support/fixture.rs"]
mod fixture;

use fixture::Fixture;
use worldrunner_original_games::{Game, checkout};
use worldrunner_unity_import::{Project, TextureError, import_texture};

const SHEET: &str = "b0000000000000000000000000000001";

/// An 8x4 RGBA PNG, fully opaque except a transparent texel at column 7, row 0 from the top.
fn png() -> Vec<u8> {
    let mut data = vec![u8::MAX; 8 * 4 * 4];
    data[7 * 4 + 3] = 0;
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 8, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
    bytes
}

fn entry(name: &str, id: i64, rect: [u32; 4], pivot: [f64; 2], shape: &str) -> String {
    let [x, y, w, h] = rect;
    format!(
        "    - name: {name}\n      rect:\n        x: {x}\n        y: {y}\n        width: {w}\n        height: {h}\n      alignment: 9\n      pivot: {{x: {}, y: {}}}\n      physicsShape: {shape}\n      internalID: {id}\n",
        pivot[0], pivot[1]
    )
}

/// A sprite sheet with an opaque sprite (1), one covering the transparent texel (2) and one
/// with an authored shape (3), at 2 pixels per unit.
fn sheet(fallback: u8) -> Fixture {
    let fixture = Fixture::new("6000.3.5f2");
    fixture.file("Assets/sheet.png", png());
    let authored = "\n      - - {x: 0, y: 0}\n        - {x: 1, y: 0}\n        - {x: 0, y: 1}";
    let entries = [
        entry("opaque", 1, [0, 0, 4, 2], [0.25, 0.5], "[]"),
        entry("holed", 2, [4, 2, 4, 2], [0.5, 0.5], "[]"),
        entry("authored", 3, [0, 2, 2, 2], [0.5, 0.5], authored),
    ]
    .concat();
    fixture.file(
        "Assets/sheet.png.meta",
        format!(
            "fileFormatVersion: 2\nguid: {SHEET}\nTextureImporter:\n  spriteMode: 2\n  spritePixelsToUnits: 2\n  spriteGenerateFallbackPhysicsShape: {fallback}\n  spriteSheet:\n    sprites:\n{entries}"
        ),
    );
    fixture
}

fn shape(fixture: &Fixture, file_id: i64) -> Result<Vec<Vec<[f64; 2]>>, TextureError> {
    let texture = import_texture(&fixture.project(), SHEET.parse().unwrap()).unwrap();
    texture.physics_shape(file_id)
}

#[test]
fn opaque_sprites_collide_with_their_rectangle_around_the_pivot() {
    let fixture = sheet(1);
    // 4x2 pixels at 2 per unit is 2x1 units; a pivot at x 0.25 puts a quarter left of it.
    let expected = vec![vec![[-0.5, -0.5], [1.5, -0.5], [1.5, 0.5], [-0.5, 0.5]]];
    assert_eq!(shape(&fixture, 1).unwrap(), expected);
}

#[test]
fn sprites_without_a_reproducible_shape_are_refused() {
    let fixture = sheet(1);
    let message = |id| shape(&fixture, id).unwrap_err().to_string();
    assert!(
        message(2).contains("sprite `holed`") && message(2).contains("fully opaque"),
        "{}",
        message(2)
    );
    assert!(
        message(3).contains("authored physics shapes"),
        "{}",
        message(3)
    );
    assert!(matches!(
        shape(&fixture, 9),
        Err(TextureError::NoSprite { file_id: 9, .. })
    ));
    let without_fallback = sheet(0);
    assert!(
        shape(&without_fallback, 1).unwrap().is_empty(),
        "no fallback means no shape"
    );
    assert!(shape(&without_fallback, 2).unwrap().is_empty());
}

#[test]
fn breakouts_brick_tile_collides_with_its_whole_rectangle() {
    let project = Project::open(&checkout(Game::Breakout).unwrap()).unwrap();
    let texture = import_texture(
        &project,
        "e137195f441c425a79bb0d5da5dd2655".parse().unwrap(),
    )
    .unwrap();
    let expected = vec![vec![[-1., -0.5], [1., -0.5], [1., 0.5], [-1., 0.5]]];
    assert_eq!(texture.physics_shape(-2_105_052_758).unwrap(), expected);
}
