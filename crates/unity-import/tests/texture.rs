use std::{fs, path::Path};

use worldrunner_unity_import::{
    Filter, Guid, SpriteMode, TextureError, Textures, Wrap, import_texture,
};

const SINGLE: &str = "a0000000000000000000000000000001";
const SHEET: &str = "a0000000000000000000000000000002";
const LEGACY: &str = "a0000000000000000000000000000003";
const PHOTO: &str = "a0000000000000000000000000000004";

#[path = "support/assets.rs"]
mod assets;
#[path = "support/fixture.rs"]
mod fixture;

use fixture::Fixture;

const EDITOR: &str = "6000.3.5f2";

/// Writes an image and a `.meta` file with `settings` under its `TextureImporter` section.
fn texture(fixture: &Fixture, path: &str, guid: &str, image: &[u8], settings: &str) {
    fixture.file(path, image);
    fixture.file(
        &format!("{path}.meta"),
        format!("fileFormatVersion: 2\nguid: {guid}\nTextureImporter:\n{settings}"),
    );
}

/// Exact comparison: every expected value is a binary fraction the importer parses exactly.
fn assert_exact<const N: usize>(actual: &[f64; N], expected: &[f64; N], context: &str) {
    let bits = |values: &[f64; N]| values.map(f64::to_bits);
    assert_eq!(
        bits(actual),
        bits(expected),
        "{context}: {actual:?} != {expected:?}"
    );
}

fn guid(text: &str) -> Guid {
    text.parse().unwrap()
}

/// A PNG whose pixel at column `x`, row `y` (from the top) is `[x, y, 7, 200]`.
fn png(width: u32, height: u32, color: png::ColorType) -> Vec<u8> {
    let channels = match color {
        png::ColorType::Grayscale => 1,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => panic!("{other:?}"),
    };
    let mut data = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let pixel = [u8::try_from(x).unwrap(), u8::try_from(y).unwrap(), 7, 200];
            data.extend_from_slice(&pixel[..channels]);
        }
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
    bytes
}

const SINGLE_SETTINGS: &str = "  mipmaps:\n    sRGBTexture: 0\n  textureSettings:\n    filterMode: 0\n    wrapU: 1\n    wrapV: 2\n  spriteMode: 1\n  alignment: 7\n  spritePivot: {x: 0.1, y: 0.9}\n  spritePixelsToUnits: 32\n  spriteGenerateFallbackPhysicsShape: 0\n";

#[test]
fn single_sprite_textures_use_the_whole_image_and_aligned_pivot() {
    let fixture = Fixture::new(EDITOR);
    texture(
        &fixture,
        "Assets/Ball.png",
        SINGLE,
        &png(4, 2, png::ColorType::Rgba),
        SINGLE_SETTINGS,
    );
    let texture = import_texture(&fixture.project(), guid(SINGLE)).unwrap();
    let settings = texture.settings();
    assert_eq!(settings.sprite_mode, SpriteMode::Single);
    assert_eq!(settings.filter, Filter::Point);
    assert_eq!(settings.wrap, [Wrap::Clamp, Wrap::Mirror]);
    assert!(!settings.srgb);
    assert!(!settings.generate_fallback_physics_shape);
    let sprite = texture.sprite(21_300_000).unwrap();
    assert_eq!(sprite.name, "Ball");
    assert_exact(&sprite.rect, &[0.0, 0.0, 4.0, 2.0], "");
    assert_exact(
        &sprite.pivot,
        &[0.5, 0.0],
        "bottom-center alignment ignores spritePivot",
    );
    assert_exact(&[sprite.pixels_per_unit], &[32.0], "");
    let image = texture.image();
    assert_eq!((image.width, image.height), (4, 2));
    assert_eq!(&image.rgba8[..4], [0, 0, 7, 200]);
    assert_eq!(
        &image.rgba8[(4 + 3) * 4..],
        [3, 1, 7, 200],
        "rows run from the top"
    );
}

fn sheet_entry(name: &str, id: i64, rect: [u32; 4], alignment: u8, pivot: [f32; 2]) -> String {
    let [x, y, w, h] = rect;
    format!(
        "    - serializedVersion: 2\n      name: {name}\n      rect:\n        serializedVersion: 2\n        x: {x}\n        y: {y}\n        width: {w}\n        height: {h}\n      alignment: {alignment}\n      pivot: {{x: {}, y: {}}}\n      border: {{x: 1, y: 2, z: 3, w: 4}}\n      outline: []\n      physicsShape: []\n      internalID: {id}\n",
        pivot[0], pivot[1]
    )
}

fn sheet(entries: &[String]) -> String {
    format!(
        "  spriteMode: 2\n  spritePixelsToUnits: 16\n  spriteSheet:\n    serializedVersion: 2\n    sprites:\n{}",
        entries.concat()
    )
}

#[test]
fn sprite_sheets_list_sprites_by_internal_id_with_alignment_pivots() {
    let fixture = Fixture::new(EDITOR);
    let entries = [
        sheet_entry(
            "ball",
            -7_845_977_501_651_575_310,
            [0, 0, 2, 2],
            0,
            [0.0, 0.0],
        ),
        sheet_entry(
            "brick",
            8_482_151_852_852_566_296,
            [2, 1, 6, 3],
            9,
            [0.25, 0.75],
        ),
    ];
    texture(
        &fixture,
        "Assets/Sheet.png",
        SHEET,
        &png(8, 4, png::ColorType::Rgb),
        &sheet(&entries),
    );
    let texture = import_texture(&fixture.project(), guid(SHEET)).unwrap();
    assert_eq!(
        texture.settings().filter,
        Filter::Bilinear,
        "-1 and absent mean bilinear"
    );
    assert_eq!(texture.settings().wrap, [Wrap::Repeat; 2]);
    assert_eq!(texture.sprites().len(), 2);
    let ball = texture.sprite(-7_845_977_501_651_575_310).unwrap();
    assert_exact(
        &ball.pivot,
        &[0.5, 0.5],
        "center alignment ignores the stored pivot",
    );
    assert_exact(&[ball.pixels_per_unit], &[16.0], "");
    assert_exact(&ball.border, &[1.0, 2.0, 3.0, 4.0], "");
    let brick = texture.sprite(8_482_151_852_852_566_296).unwrap();
    assert_exact(&brick.rect, &[2.0, 1.0, 6.0, 3.0], "");
    assert_exact(
        &brick.pivot,
        &[0.25, 0.75],
        "custom alignment uses the stored pivot",
    );
    assert_eq!(
        &texture.image().rgba8[4..8],
        [1, 0, 7, 255],
        "RGB expands with opaque alpha"
    );
    assert!(texture.sprite(21_300_000).is_none());
}

#[test]
fn legacy_sheets_take_file_ids_from_the_recycle_table() {
    let fixture = Fixture::new(EDITOR);
    let entry =
        sheet_entry("paddle", 0, [0, 0, 1, 1], 6, [0.5, 0.5]).replace("      internalID: 0\n", "");
    let settings = format!(
        "  fileIDToRecycleName:\n    21300000: ball\n    21300002: paddle\n{}",
        sheet(&[entry])
    );
    texture(
        &fixture,
        "Assets/Old.png",
        LEGACY,
        &png(2, 2, png::ColorType::Grayscale),
        &settings,
    );
    let texture = import_texture(&fixture.project(), guid(LEGACY)).unwrap();
    let paddle = texture.sprite(21_300_002).unwrap();
    assert_exact(&paddle.pivot, &[0.0, 0.0], "");
    assert_eq!(
        &texture.image().rgba8[4..8],
        [1, 1, 1, 255],
        "gray expands to RGB"
    );
}

#[test]
fn jpeg_textures_decode_to_rgba() {
    let fixture = Fixture::new(EDITOR);
    let jpeg = include_bytes!("fixtures/solid-16x8.jpg");
    texture(
        &fixture,
        "Assets/Photo.jpg",
        PHOTO,
        jpeg,
        "  spriteMode: 1\n",
    );
    let texture = import_texture(&fixture.project(), guid(PHOTO)).unwrap();
    let image = texture.image();
    assert_eq!((image.width, image.height), (16, 8));
    assert_eq!(image.rgba8.len(), 16 * 8 * 4);
    let pixel = &image.rgba8[(3 * 16 + 5) * 4..][..4];
    let expected = [200, 40, 40, 255];
    for (actual, expected) in pixel.iter().zip(expected) {
        assert!(actual.abs_diff(expected) <= 2, "{pixel:?}");
    }
}

#[test]
fn the_texture_cache_imports_once_and_resolves_sprite_references() {
    let fixture = Fixture::new(EDITOR);
    texture(
        &fixture,
        "Assets/Ball.png",
        SINGLE,
        &png(4, 2, png::ColorType::Rgba),
        SINGLE_SETTINGS,
    );
    let project = fixture.project();
    let mut textures = Textures::default();
    let (_, sprite) = textures.sprite(&project, guid(SINGLE), 21_300_000).unwrap();
    assert_eq!(sprite.name, "Ball");
    fs::remove_file(fixture.0.path().join("Assets/Ball.png")).unwrap();
    assert!(
        textures.get(&project, guid(SINGLE)).is_ok(),
        "second lookup uses the cache"
    );
    let error = textures.sprite(&project, guid(SINGLE), 5).unwrap_err();
    assert!(
        matches!(error, TextureError::NoSprite { file_id: 5, .. }),
        "{error}"
    );
    let error = textures.get(&project, guid(SHEET)).unwrap_err();
    assert!(matches!(error, TextureError::Missing(_)), "{error}");
}

fn import_error(image: &[u8], settings: &str) -> String {
    let fixture = Fixture::new(EDITOR);
    texture(&fixture, "Assets/Bad.png", SINGLE, image, settings);
    let error = import_texture(&fixture.project(), guid(SINGLE))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Bad.png"), "{error}");
    error
}

#[test]
fn unsupported_or_inconsistent_textures_fail_with_their_path() {
    let image = png(8, 4, png::ColorType::Rgba);
    let outside = sheet(&[sheet_entry("big", 1, [4, 0, 6, 4], 0, [0.5, 0.5])]);
    assert!(import_error(&image, &outside).contains("outside the 8x4 image"));
    let twice = sheet(&[
        sheet_entry("a", 1, [0, 0, 1, 1], 0, [0.5, 0.5]),
        sheet_entry("b", 1, [1, 0, 1, 1], 0, [0.5, 0.5]),
    ]);
    assert!(import_error(&image, &twice).contains("used twice"));
    assert!(import_error(&image, "  spriteMode: 3\n").contains("sprite mode 3"));
    assert!(import_error(&image, "  spriteMode: 1\n  alignment: 12\n").contains("alignment 12"));
    assert!(import_error(&image, "  spriteMode: 1\n  maxTextureSize: 4\n").contains("downscales"));
    assert!(import_error(&image, "  spritePixelsToUnits: 0\n").contains("must be positive"));
    assert!(
        import_error(b"BM not an image", "  spriteMode: 1\n").contains("unsupported image format")
    );
    assert!(
        import_error(b"\x89PNG\r\n\x1a\ntruncated", "  spriteMode: 1\n").contains("invalid PNG")
    );
    let no_id = sheet(&[sheet_entry("x", 0, [0, 0, 1, 1], 0, [0.5, 0.5])]);
    assert!(import_error(&image, &no_id).contains("sprite `x`"));
}

#[test]
fn assets_without_texture_settings_are_not_textures() {
    let fixture = Fixture::new(EDITOR);
    fixture.asset(
        "Assets/Notes.png",
        SINGLE,
        "DefaultImporter",
        png(1, 1, png::ColorType::Rgba),
    );
    let error = import_texture(&fixture.project(), guid(SINGLE)).unwrap_err();
    assert!(
        error.to_string().contains("not imported as a texture"),
        "{error}"
    );
    assert_eq!(
        fixture.project().guid_of(Path::new("Assets/Notes.png")),
        Some(guid(SINGLE))
    );
}
