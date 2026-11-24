//! The pixels of a read as PNG files: the colour as an 8-bit RGBA image and the depth as a 16-bit
//! grey one, both with the top row first.
//!
//! A copy of a texture holds its rows bottom first, as the GPU stores them, so the rows are
//! flipped here, once.

use std::path::Path;

use worldrunner_render_program::texels::rect_rgba;

use crate::PlayError;
use crate::driver::write_rgba_png;

/// Whether a format holds 8-bit channels, which are written as they are stored. Every other
/// format is a float or a wider integer that is clamped to 0..1 and sRGB encoded for the file.
fn stored_as_bytes(format: wgpu::TextureFormat) -> bool {
    use wgpu::TextureFormat as F;
    matches!(
        format,
        F::Rgba8Unorm
            | F::Rgba8UnormSrgb
            | F::Bgra8Unorm
            | F::Bgra8UnormSrgb
            | F::R8Unorm
            | F::Rg8Unorm
    )
}

/// The sRGB encoding of a linear value in 0..1.
fn srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// A value clamped to 0..1, with NaN as 0.
fn unit(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// The whole number in `0..=max` nearest `value`, halves rounding up, found without a conversion
/// from float to integer: the count of `n` below `max` with `n + 0.5 <= value`.
fn nearest(value: f32, max: u16) -> u16 {
    let (mut low, mut high) = (0, max);
    while low < high {
        let middle = low + (high - low) / 2;
        if f32::from(middle) + 0.5 <= value {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

/// A value in 0..1 as a byte, sRGB encoded when `encode` says so.
fn byte(value: f32, encode: bool) -> u8 {
    let clamped = unit(value);
    let shown = if encode { srgb(clamped) } else { clamped };
    u8::try_from(nearest(shown * 255.0, 255)).unwrap_or(u8::MAX)
}

/// The colour of a copied level as RGBA bytes, rows top first. Alpha is linear and never encoded.
///
/// # Errors
/// Names the format when its texels are not given as floats.
pub(super) fn colour_rgba8(
    bytes: &[u8],
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> Result<Vec<u8>, String> {
    let [width, height] = size;
    let floats = rect_rgba(bytes, format, size, [0, 0, width, height])
        .ok_or_else(|| format!("{format:?} is not given as floats"))?;
    let encode = !stored_as_bytes(format);
    let grey = format.components() == 1;
    let mut rgba = Vec::with_capacity(floats.len());
    for row in floats.chunks_exact(width as usize * 4).rev() {
        for texel in row.chunks_exact(4) {
            let red = byte(texel[0], encode);
            let (green, blue) = if grey {
                (red, red)
            } else {
                (byte(texel[1], encode), byte(texel[2], encode))
            };
            rgba.extend([red, green, blue, byte(texel[3], false)]);
        }
    }
    Ok(rgba)
}

/// Writes the colour of a copied level to `path` as an 8-bit RGBA PNG, rows top first.
///
/// # Errors
/// Names the format when its texels are not given as floats, or the file when it cannot be written.
pub(super) fn write_colour_png(
    path: &Path,
    bytes: &[u8],
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> Result<(), String> {
    let rgba = colour_rgba8(bytes, format, size)?;
    write_rgba_png(path, size, &rgba).map_err(|error| error.to_string())
}

/// The stored depth of a copied level as big-endian 16-bit values, rows top first. A float depth
/// is clamped to 0..1 and scaled to 65535; a 16-bit depth keeps its stored value.
///
/// # Errors
/// Names the format when it is not a depth format, or when the bytes do not hold the level.
pub(super) fn depth_grey16(
    bytes: &[u8],
    format: wgpu::TextureFormat,
    size: [u32; 2],
) -> Result<Vec<u8>, String> {
    use wgpu::TextureFormat as F;
    let values: Vec<u16> = match format {
        F::Depth16Unorm => bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect(),
        F::Depth32Float | F::Depth32FloatStencil8 => bytes
            .chunks_exact(4)
            .map(|word| {
                let depth = f32::from_le_bytes([word[0], word[1], word[2], word[3]]);
                nearest(unit(depth) * 65535.0, u16::MAX)
            })
            .collect(),
        _ => return Err(format!("{format:?} is not a depth format")),
    };
    let width = size[0] as usize;
    if values.len() != width * size[1] as usize {
        return Err(format!(
            "the copy of {format:?} does not hold {width} columns of {} rows",
            size[1]
        ));
    }
    Ok(values
        .chunks_exact(width)
        .rev()
        .flatten()
        .flat_map(|value| value.to_be_bytes())
        .collect())
}

/// Writes 16-bit grey rows, big-endian, top first, as a PNG.
///
/// # Errors
/// Fails when the file cannot be made or written.
pub(super) fn write_grey16_png(path: &Path, size: [u32; 2], grey: &[u8]) -> Result<(), PlayError> {
    let file =
        std::fs::File::create(path).map_err(|error| PlayError::Io(path.to_owned(), error))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), size[0], size[1]);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Sixteen);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(grey))
        .map_err(|error| PlayError::Invalid(format!("{}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RGBA32: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

    fn floats(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn a_linear_half_becomes_188_and_values_outside_zero_to_one_are_clamped() {
        let bytes = floats(&[
            0.5,
            1.5,
            -1.0,
            1.0, //
            f32::NAN,
            0.0,
            1.0,
            0.25,
        ]);
        let rgba = colour_rgba8(&bytes, RGBA32, [2, 1]).unwrap();
        assert_eq!(&rgba[..3], [188, 255, 0], "first texel");
        assert_eq!(&rgba[4..7], [0, 0, 255], "second texel, NaN as 0");
    }

    #[test]
    fn eight_bit_formats_are_written_as_stored() {
        let bytes = [100, 7, 255, 255];
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            assert_eq!(colour_rgba8(&bytes, format, [1, 1]).unwrap(), bytes);
        }
    }

    #[test]
    fn the_rows_come_out_top_first_though_the_gpu_stores_the_bottom_first() {
        let bottom_then_top = [1, 2, 3, 255, 200, 201, 202, 255];
        let rgba = colour_rgba8(&bottom_then_top, wgpu::TextureFormat::Rgba8Unorm, [1, 2]).unwrap();
        assert_eq!(rgba, [200, 201, 202, 255, 1, 2, 3, 255]);
    }

    #[test]
    fn a_one_channel_float_is_written_as_grey() {
        let bytes = 0.5_f32.to_le_bytes();
        let rgba = colour_rgba8(&bytes, wgpu::TextureFormat::R32Float, [1, 1]).unwrap();
        assert_eq!(&rgba[..3], [188, 188, 188]);
    }

    #[test]
    fn an_integer_format_is_refused_by_name() {
        let error = colour_rgba8(&[0; 4], wgpu::TextureFormat::R32Uint, [1, 1]).unwrap_err();
        assert!(error.contains("R32Uint"), "{error}");
    }

    #[test]
    fn a_float_depth_of_a_half_becomes_32768_and_the_rows_flip() {
        let bytes = floats(&[0.0, 0.5, 2.0, -1.0]);
        let grey = depth_grey16(&bytes, wgpu::TextureFormat::Depth32Float, [2, 2]).unwrap();
        let values: Vec<u16> = grey
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(values, [65535, 0, 0, 32768], "top row first, clamped");
    }

    #[test]
    fn a_sixteen_bit_depth_keeps_its_stored_value() {
        let bytes: Vec<u8> = [0_u16, 513].iter().flat_map(|v| v.to_le_bytes()).collect();
        let grey = depth_grey16(&bytes, wgpu::TextureFormat::Depth16Unorm, [1, 2]).unwrap();
        assert_eq!(grey, [2, 1, 0, 0]);
    }

    #[test]
    fn a_grey_png_is_sixteen_bit_and_holds_the_values_it_was_given() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("depth.png");
        write_grey16_png(&file, [2, 1], &[0x80, 0x00, 0x12, 0x34]).unwrap();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&file).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        assert_eq!(info.color_type, png::ColorType::Grayscale);
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        assert_eq!(data, [0x80, 0x00, 0x12, 0x34]);
    }

    /// The colour type and bytes of a PNG.
    fn decoded(path: &Path) -> (png::ColorType, Vec<u8>) {
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        data.truncate(info.buffer_size());
        (info.color_type, data)
    }

    fn written(bytes: &[u8], format: wgpu::TextureFormat, size: [u32; 2]) -> Vec<u8> {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("colour.png");
        write_colour_png(&file, bytes, format, size).unwrap();
        let (color_type, data) = decoded(&file);
        assert_eq!(color_type, png::ColorType::Rgba);
        data
    }

    #[test]
    fn a_float_target_keeps_its_alpha_linear_while_the_colour_is_encoded() {
        let bytes = floats(&[
            0.5,
            0.5,
            0.5,
            0.5,
            0.0,
            0.0,
            0.0,
            2.0,
            0.0,
            0.0,
            0.0,
            f32::NAN,
        ]);
        let data = written(&bytes, RGBA32, [3, 1]);
        assert_eq!(data, [188, 188, 188, 128, 0, 0, 0, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn an_eight_bit_target_round_trips_its_alpha() {
        let bytes = [10, 20, 30, 77, 40, 50, 60, 0];
        let data = written(&bytes, wgpu::TextureFormat::Rgba8Unorm, [2, 1]);
        assert_eq!(data, bytes);
    }

    #[test]
    fn a_format_with_no_alpha_channel_writes_255() {
        let data = written(&[100, 200], wgpu::TextureFormat::Rg8Unorm, [1, 1]);
        assert_eq!(data, [100, 200, 0, 255]);
        let data = written(&[9], wgpu::TextureFormat::R8Unorm, [1, 1]);
        assert_eq!(data, [9, 9, 9, 255]);
    }

    #[test]
    fn a_one_channel_float_is_grey_with_full_alpha_in_the_file() {
        let bytes = 0.5_f32.to_le_bytes();
        let data = written(&bytes, wgpu::TextureFormat::R32Float, [1, 1]);
        assert_eq!(data, [188, 188, 188, 255]);
    }
}
