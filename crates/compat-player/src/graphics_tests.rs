use super::write_png;
use std::io::Cursor;
use worldrunner::scene::gpu::{Viewport, offscreen::RgbaImage};

#[test]
fn write_png_preserves_rgba_pixels_dimensions_alpha_and_row_order() {
    let image = RgbaImage {
        viewport: Viewport {
            width: 5,
            height: 3,
        },
        pixels: (0_u8..15)
            .flat_map(|index| {
                [
                    index.wrapping_mul(37),
                    index.wrapping_mul(61).wrapping_add(9),
                    255 - index * 11,
                    if index % 4 == 0 { 0 } else { index * 17 },
                ]
            })
            .collect(),
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("capture.png");

    write_png(&path, &image).unwrap();

    let bytes = std::fs::read(path).unwrap();
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    assert_eq!(reader.info().width, image.viewport.width);
    assert_eq!(reader.info().height, image.viewport.height);
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut decoded).unwrap();
    decoded.truncate(frame.buffer_size());
    assert_eq!(decoded, image.pixels);
}

#[test]
fn write_png_propagates_invalid_rgba_length() {
    let image = RgbaImage {
        viewport: Viewport {
            width: 2,
            height: 2,
        },
        pixels: vec![1, 2, 3, 4],
    };
    let directory = tempfile::tempdir().unwrap();

    let error = write_png(&directory.path().join("invalid.png"), &image).unwrap_err();

    assert!(!error.is_empty());
}
