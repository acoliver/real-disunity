//! The supersampled reference and the display conversions of the qualification matrix.
//!
//! The reference is the composite scene rasterised unjittered at `factor` times the output
//! width and height, box averaged in linear HDR and then tone mapped once (Reinhard, exposure 1),
//! which is what the renderer's output stage does before the sRGB transfer. Renderer output
//! bytes are decoded back to display-linear values, so both sides of every metric are
//! display-linear. The renderer's 8 bit output has a quantisation floor of about 1/255 that the
//! reference does not have.
use super::{
    qualification_scene::{CompositeScene, HDR, depth_target, frame, target},
    taa_end_to_end_tests::OUTPUT,
};
use crate::{device::GpuContext, quality_metrics::Pixel};
use worldrunner_render_post::{FrameConfig, Readback, ToneMap};

fn half_to_f32(bits: u16) -> f32 {
    let exponent = i32::from((bits >> 10) & 0x1f);
    let fraction = f32::from(bits & 0x3ff);
    let magnitude = if exponent == 0 {
        fraction * 2f32.powi(-24)
    } else {
        (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15)
    };
    if bits & 0x8000 == 0 {
        magnitude
    } else {
        -magnitude
    }
}

fn srgb_to_linear(byte: u8) -> f32 {
    let encoded = f32::from(byte) / 255.0;
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// Decodes the renderer's `OUTPUT` bytes into display-linear pixels.
pub(super) fn decode_display(bytes: &[u8]) -> Vec<Pixel> {
    debug_assert_eq!(OUTPUT, wgpu::TextureFormat::Rgba8UnormSrgb);
    bytes
        .chunks_exact(4)
        .map(|t| {
            [
                srgb_to_linear(t[0]),
                srgb_to_linear(t[1]),
                srgb_to_linear(t[2]),
                1.0,
            ]
        })
        .collect()
}

/// Renders the reference for the camera at `eye_x` and returns display-linear pixels.
pub(super) fn reference(
    context: &GpuContext,
    scene: &CompositeScene,
    output: [u32; 2],
    (eye_x, factor): (i64, u32),
) -> Vec<Pixel> {
    let device = context.device();
    let big = [output[0] * factor, output[1] * factor];
    let color = target(device, big, HDR);
    let depth = depth_target(device, big);
    let mut encoder = context.submission().begin(device).unwrap();
    scene.draw(
        device,
        &mut encoder,
        (
            &color.create_view(&wgpu::TextureViewDescriptor::default()),
            &depth.create_view(&wgpu::TextureViewDescriptor::default()),
        ),
        frame(eye_x, big).view_projection,
    );
    encoder.finish().submit().unwrap();
    let limit = 8 * u64::from(big[0]) * u64::from(big[1]) + (1 << 20);
    let bytes = Readback::new(device, FrameConfig::new(big, HDR, 1), limit)
        .unwrap()
        .read(device, context.submission(), &color)
        .unwrap();
    box_average(&bytes, big[0] as usize, factor as usize)
}

/// Averages `factor` x `factor` blocks of a tightly packed `Rgba16Float` image in linear HDR and
/// tone maps the mean. The tone map is applied once, to the average.
fn box_average(bytes: &[u8], big_width: usize, factor: usize) -> Vec<Pixel> {
    let width = big_width / factor;
    let rows = bytes.len() / 8 / big_width / factor;
    #[expect(clippy::cast_precision_loss, reason = "factor is a small block size")]
    let weight = 1.0 / (factor * factor) as f32;
    let at = |x: usize, y: usize, lane: usize| -> f32 {
        let i = 8 * (y * big_width + x) + 2 * lane;
        half_to_f32(u16::from_le_bytes([bytes[i], bytes[i + 1]]))
    };
    let mut out = Vec::with_capacity(width * rows);
    for row in 0..rows {
        for column in 0..width {
            let mut mean = [0.0_f32; 3];
            for sy in 0..factor {
                for sx in 0..factor {
                    for (lane, value) in mean.iter_mut().enumerate() {
                        *value += weight * at(column * factor + sx, row * factor + sy, lane);
                    }
                }
            }
            #[expect(clippy::cast_possible_truncation, reason = "display values are 0..=1")]
            let mapped = mean.map(|v| ToneMap::Reinhard.evaluate(f64::from(v), 1.0) as f32);
            out.push([mapped[0], mapped[1], mapped[2], 1.0]);
        }
    }
    out
}

/// Pixels within one pixel (right and down) of a luma step above `threshold` in the reference.
pub(super) fn edge_mask(image: &[Pixel], width: usize, threshold: f32) -> Vec<bool> {
    let luma = |p: &Pixel| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
    let mut mask = vec![false; image.len()];
    for i in 0..image.len() {
        let x = i % width;
        let steps = [
            (x + 1 < width).then_some(i + 1),
            (i + width < image.len()).then_some(i + width),
        ];
        for j in steps.into_iter().flatten() {
            if (luma(&image[i]) - luma(&image[j])).abs() > threshold {
                mask[i] = true;
                mask[j] = true;
            }
        }
    }
    mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_block_averages_to_its_tone_mapped_value() {
        // Two by two block of 1.0 in a 2x2 image: the average is 1.0, Reinhard maps it to 0.5.
        let one = 0x3c00_u16.to_le_bytes();
        let texel = [
            one[0], one[1], one[0], one[1], one[0], one[1], one[0], one[1],
        ];
        let bytes: Vec<u8> = (0..4).flat_map(|_| texel).collect();
        let image = box_average(&bytes, 2, 2);
        assert_eq!(image.len(), 1);
        assert!((image[0][0] - 0.5).abs() < 1e-6, "{:?}", image[0]);
    }

    #[test]
    fn the_edge_mask_marks_both_sides_of_a_step_and_nothing_flat() {
        let image: Vec<Pixel> = [0.0, 0.0, 1.0, 1.0].map(|v| [v; 4]).to_vec();
        assert_eq!(edge_mask(&image, 4, 0.1), vec![false, true, true, false]);
        assert_eq!(edge_mask(&image[..2], 2, 0.1), vec![false, false]);
    }

    #[test]
    fn srgb_bytes_decode_to_linear() {
        assert!(srgb_to_linear(0).abs() < 1e-12);
        assert!((srgb_to_linear(255) - 1.0).abs() < 1e-6);
        assert!((srgb_to_linear(188) - 0.5).abs() < 0.01);
    }
}
