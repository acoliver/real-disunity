use std::io::Cursor;

/// Largest image the importer decodes, in pixels.
pub const MAX_PIXELS: u64 = 8192 * 8192;

/// Decoded 8-bit RGBA texels in rows from top to bottom. Sprite rects measure from the bottom, so
/// a rect's row `y` is texel row `height - 1 - y`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

/// Decodes a PNG or baseline/progressive JPEG, chosen by the file's signature.
pub(super) fn decode(bytes: &[u8]) -> Result<Image, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        jpeg(bytes)
    } else {
        Err("unsupported image format; PNG and JPEG are supported".into())
    }
}

fn checked(width: u32, height: u32) -> Result<usize, String> {
    let pixels = u64::from(width) * u64::from(height);
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err(format!(
            "image size {width}x{height} is outside the supported range"
        ));
    }
    usize::try_from(pixels * 4).map_err(|_| "image is too large".to_owned())
}

fn png(bytes: &[u8]) -> Result<Image, String> {
    let error = |e: png::DecodingError| format!("invalid PNG: {e}");
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: usize::try_from(MAX_PIXELS * 8).unwrap_or(usize::MAX),
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(error)?;
    let size = reader.output_buffer_size().ok_or("PNG is too large")?;
    let mut buffer = vec![0; size];
    let info = reader.next_frame(&mut buffer).map_err(error)?;
    let capacity = checked(info.width, info.height)?;
    buffer.truncate(info.buffer_size());
    let rgba8 = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => expand(&buffer, 3, capacity, |p| [p[0], p[1], p[2], 255]),
        png::ColorType::GrayscaleAlpha => {
            expand(&buffer, 2, capacity, |p| [p[0], p[0], p[0], p[1]])
        }
        png::ColorType::Grayscale => expand(&buffer, 1, capacity, |p| [p[0], p[0], p[0], 255]),
        png::ColorType::Indexed => return Err("PNG palette was not expanded".into()),
    };
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba8,
    })
}

fn jpeg(bytes: &[u8]) -> Result<Image, String> {
    let error = |e: jpeg_decoder::Error| format!("invalid JPEG: {e}");
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    decoder.set_max_decoding_buffer_size(usize::try_from(MAX_PIXELS * 4).unwrap_or(usize::MAX));
    decoder.read_info().map_err(error)?;
    let info = decoder.info().ok_or("JPEG has no frame")?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let capacity = checked(width, height)?;
    let pixels = decoder.decode().map_err(error)?;
    let rgba8 = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            expand(&pixels, 3, capacity, |p| [p[0], p[1], p[2], 255])
        }
        jpeg_decoder::PixelFormat::L8 => expand(&pixels, 1, capacity, |p| [p[0], p[0], p[0], 255]),
        other => return Err(format!("JPEG pixel format {other:?} is not supported")),
    };
    Ok(Image {
        width,
        height,
        rgba8,
    })
}

/// Converts packed pixels of `channels` bytes each to RGBA8.
fn expand(
    pixels: &[u8],
    channels: usize,
    capacity: usize,
    rgba: impl Fn(&[u8]) -> [u8; 4],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(capacity);
    for pixel in pixels.chunks_exact(channels) {
        out.extend(rgba(pixel));
    }
    out
}
