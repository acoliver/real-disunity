//! The CAS pass alone on synthetic f32 images, compared with the scalar reference.
use crate::cas_reference::Image;
use crate::{
    CasSettings, FrameConfig, FrameTargets, PostChain, PostError, Readback, Resources,
    StageFactory, StageSet, StageTarget, color_texture,
};

/// Absolute per-channel bound between the f32 GPU kernel and the f64 reference. The kernel
/// is a few multiplies and two divisions per channel, so the error is a few f32 ulps.
const BOUND: f64 = 1e-5;

struct CasOnly {
    settings: CasSettings,
    source: wgpu::TextureView,
}
impl StageFactory for CasOnly {
    fn build(
        &self,
        device: &wgpu::Device,
        _: &FrameTargets,
        _: &Resources,
    ) -> Result<StageSet, PostError> {
        let mut set = StageSet::default();
        crate::cas::append(
            device,
            &mut set,
            &self.source,
            self.settings,
            (wgpu::TextureFormat::Rgba32Float, StageTarget::Output),
        );
        Ok(set)
    }
}

fn extent(image: &Image) -> [u32; 2] {
    image.size.map(|v| u32::try_from(v).unwrap())
}

fn upload(device: &wgpu::Device, queue: &wgpu::Queue, image: &Image) -> wgpu::TextureView {
    let [width, height] = extent(image);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("CAS test source"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 16),
            rows_per_image: Some(height),
        },
        texture.size(),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Runs the pass on `image` and returns the `Rgba32Float` result.
fn gpu_sharpen(
    (device, queue): (&wgpu::Device, &wgpu::Queue),
    image: &Image,
    sharpness: f32,
) -> Vec<[f64; 4]> {
    let size = extent(image);
    let chain = PostChain::new(
        device,
        FrameConfig::new(size, wgpu::TextureFormat::Rgba16Float, 1),
        Resources::default(),
        CasOnly {
            settings: CasSettings::new(sharpness),
            source: upload(device, queue, image),
        },
    )
    .unwrap();
    let target = color_texture(
        device,
        "CAS test output",
        size,
        wgpu::TextureFormat::Rgba32Float,
    );
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    chain.encode(
        &mut encoder,
        &target.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    queue.submit([encoder.finish()]);
    let raw = Readback::new(
        device,
        FrameConfig::new(size, target.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(device, &crate::test_authority(queue), &target)
    .unwrap();
    raw.chunks_exact(16)
        .map(|p| {
            std::array::from_fn(|c| {
                f64::from(f32::from_le_bytes(p[c * 4..c * 4 + 4].try_into().unwrap()))
            })
        })
        .collect()
}

/// Deterministic value in 0..=1 that varies per pixel and channel.
fn noise(x: i32, y: i32, channel: u32) -> f32 {
    let mut h = u32::try_from(x).unwrap().wrapping_mul(0x9E37_79B1)
        ^ u32::try_from(y).unwrap().wrapping_mul(0x85EB_CA6B)
        ^ channel.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    f32::from(u16::try_from(h & 0xffff).unwrap()) / 65535.0
}

/// Texture-like content: independent noise per channel with some pixels at exactly 0 and 1.
fn noisy(size: [i32; 2]) -> Image {
    Image::from_fn(size, |x, y| {
        let mut p = [0, 1, 2, 3].map(|c| noise(x, y, c));
        if (x + 2 * y) % 7 == 0 {
            p[1] = 1.0;
        }
        if (x + y) % 11 == 0 {
            p[0] = 0.0;
        }
        p
    })
}

/// A horizontal ramp from `low` to `high` across `width` pixels starting at column 3.
fn ramp(size: [i32; 2], low: f32, high: f32, width: i32) -> Image {
    Image::from_fn(size, |x, _| {
        let t = f32::from(u16::try_from((x - 3).clamp(0, width)).unwrap())
            / f32::from(u16::try_from(width).unwrap());
        let v = low + (high - low) * t;
        [v, v, v, 0.5]
    })
}

fn assert_close(actual: &[[f64; 4]], expected: &[[f64; 4]], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}");
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        for c in 0..4 {
            assert!(
                (a[c] - e[c]).abs() <= BOUND,
                "{context}: pixel {index} channel {c}: gpu {} reference {}",
                a[c],
                e[c]
            );
        }
    }
}

fn run(image: &Image, sharpness: f32) -> Vec<[f64; 4]> {
    let (device, queue) = crate::gpu_tests::gpu();
    gpu_sharpen((&device, &queue), image, sharpness)
}

#[test]
fn gpu_kernel_matches_the_scalar_reference_on_odd_extents() {
    let (device, queue) = crate::gpu_tests::gpu();
    for size in [[1, 1], [2, 1], [1, 5], [3, 3], [17, 9], [31, 13]] {
        let image = noisy(size);
        for sharpness in [0.0, 0.5, 1.0] {
            let expected = image.sharpen(f64::from(CasSettings::new(sharpness).peak()), true);
            let actual = gpu_sharpen((&device, &queue), &image, sharpness);
            assert_close(
                &actual,
                &expected,
                &format!("{size:?} sharpness {sharpness}"),
            );
        }
    }
}

#[test]
fn constant_images_are_unchanged_at_every_sharpness() {
    let (device, queue) = crate::gpu_tests::gpu();
    for value in [0.0, 0.18, 0.5, 0.97, 1.0] {
        let image = Image::from_fn([9, 5], |_, _| [value, value * 0.5, 1.0 - value, 0.25]);
        for sharpness in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let actual = gpu_sharpen((&device, &queue), &image, sharpness);
            assert_close(
                &actual,
                &image.widened(),
                &format!("{value} at {sharpness}"),
            );
        }
    }
}

/// The widest step between neighboring pixels along the middle row.
fn steepest(pixels: &[[f64; 4]], width: usize, row: usize) -> f64 {
    pixels[row * width..(row + 1) * width]
        .windows(2)
        .map(|pair| (pair[1][1] - pair[0][1]).abs())
        .fold(0.0, f64::max)
}

#[test]
fn a_soft_edge_gets_steeper_as_sharpness_rises() {
    let (device, queue) = crate::gpu_tests::gpu();
    let image = ramp([12, 3], 0.3, 0.6, 4);
    let input = steepest(&image.widened(), 12, 1);
    let mut previous = input;
    for sharpness in [0.0, 0.5, 1.0] {
        let sharp = steepest(&gpu_sharpen((&device, &queue), &image, sharpness), 12, 1);
        assert!(
            sharp > previous + 1e-3,
            "{sharpness}: {sharp} after {previous}"
        );
        previous = sharp;
    }
    assert!(input < 0.0751, "the ramp rises 0.075 per pixel");
}

#[test]
fn sharpness_one_moves_every_pixel_at_least_as_far_as_sharpness_zero() {
    let (device, queue) = crate::gpu_tests::gpu();
    let image = noisy([17, 9]);
    let input = image.widened();
    let weak = gpu_sharpen((&device, &queue), &image, 0.0);
    let strong = gpu_sharpen((&device, &queue), &image, 1.0);
    let mut stronger = 0;
    for ((i, w), s) in input.iter().zip(&weak).zip(&strong) {
        for c in 0..3 {
            let (w, s) = ((w[c] - i[c]).abs(), (s[c] - i[c]).abs());
            // Saturation can end a larger excursion early, never reverse the order.
            assert!(
                s + BOUND >= w,
                "sharpness 1 moved {s}, sharpness 0 moved {w}"
            );
            stronger += usize::from(s > w + 1e-4);
        }
    }
    assert!(stronger > 50, "only {stronger} channels differ");
}

#[test]
fn alpha_is_copied_and_color_stays_in_the_display_range() {
    let image = noisy([17, 9]);
    for sharpness in [0.0, 1.0] {
        for (input, output) in image.widened().iter().zip(&run(&image, sharpness)) {
            assert_eq!(
                input[3].to_bits(),
                output[3].to_bits(),
                "alpha is not filtered"
            );
            assert!(
                output[..3].iter().all(|v| (0.0..=1.0).contains(v)),
                "{output:?}"
            );
        }
    }
}

/// The limiter divides by the local maximum and the distance to the signal limit, so contrast
/// that already reaches black or white is kept and the same contrast in mid-range is sharpened.
#[test]
fn contrast_that_reaches_the_signal_limits_is_kept_and_mid_range_contrast_is_sharpened() {
    let (device, queue) = crate::gpu_tests::gpu();
    for (low, high) in [(0.0, 1.0), (0.0, 0.5), (0.5, 1.0)] {
        let image = ramp([10, 3], low, high, 1);
        assert_close(
            &gpu_sharpen((&device, &queue), &image, 1.0),
            &image.widened(),
            &format!("{low}..{high}"),
        );
    }
    let mid = ramp([10, 3], 0.3, 0.6, 1);
    let moved = gpu_sharpen((&device, &queue), &mid, 1.0)
        .iter()
        .zip(mid.widened())
        .map(|(a, b)| (a[1] - b[1]).abs())
        .fold(0.0, f64::max);
    assert!(moved > 0.01, "mid-range edge moved {moved}");
}
