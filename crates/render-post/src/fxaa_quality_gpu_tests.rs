use super::filter_reference::{Image, fxaa_quality, half_bits};
use super::filter_reference_gpu_tests::render;
use crate::{FxaaLuminance, FxaaQuality};

fn check(gpu: (&wgpu::Device, &wgpu::Queue), input: &Image, q: FxaaQuality) -> Vec<u8> {
    let expected = Image::render(input.size, |uv| fxaa_quality(input, uv, 0.75, q));
    let actual = render(
        gpu,
        "fxaa",
        (input, None),
        &expected,
        wgpu::TextureFormat::Rgba16Float,
        &q.constants(0.75),
    );
    for (pixel, reference) in actual.chunks_exact(8).zip(&expected.pixels) {
        for (channel, (&value, bytes)) in reference.iter().zip(pixel.chunks_exact(2)).enumerate() {
            let bits = u16::from_le_bytes(bytes.try_into().unwrap());
            let distance = bits.abs_diff(half_bits(value));
            assert!(
                distance <= u16::from(channel < 3),
                "channel={channel}, distance={distance}"
            );
        }
    }
    actual
}

fn fixture(size: [u16; 2], thin: bool) -> Image {
    let mut pixels = Vec::new();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let bright = if thin { x == y } else { x > y };
            let v = if bright {
                0.75
            } else if y % 3 == 0 {
                0.25
            } else {
                0.125
            };
            pixels.push([v, v * 0.5, v * 0.25, 0.25]);
        }
    }
    Image { size, pixels }
}

#[test]
fn fxaa_quality_diagonals_thin_lines_constants_and_odd_extents() {
    let (device, queue) = crate::gpu_tests::gpu();
    for size in [[9, 7], [1, 1]] {
        for thin in [false, true] {
            let input = fixture(size, thin);
            for luminance in [FxaaLuminance::Linear, FxaaLuminance::Srgb] {
                for subpixel_blend in [0.0, 0.5, 1.0] {
                    check(
                        (&device, &queue),
                        &input,
                        FxaaQuality {
                            edge_threshold: 0.2,
                            minimum_threshold: 0.03125,
                            subpixel_blend,
                            luminance,
                        },
                    );
                }
            }
        }
    }
    let input = Image {
        size: [9, 7],
        pixels: vec![[0.25, 0.5, 0.75, 0.125]; 63],
    };
    for luminance in [FxaaLuminance::Linear, FxaaLuminance::Srgb] {
        check(
            (&device, &queue),
            &input,
            FxaaQuality {
                luminance,
                ..Default::default()
            },
        );
    }
}

#[test]
fn fxaa_quality_thresholds_and_transfer_change_edge_admission() {
    let (device, queue) = crate::gpu_tests::gpu();
    let input = fixture([9, 7], true);
    let q = FxaaQuality {
        edge_threshold: 0.0,
        minimum_threshold: 0.0,
        ..Default::default()
    };
    let active = check((&device, &queue), &input, q);
    let off = check(
        (&device, &queue),
        &input,
        FxaaQuality {
            subpixel_blend: 0.0,
            ..q
        },
    );
    assert_ne!(active, off);
    for quality in [
        FxaaQuality {
            edge_threshold: 1.0,
            ..q
        },
        FxaaQuality {
            minimum_threshold: 1.0,
            ..q
        },
    ] {
        assert_eq!(check((&device, &queue), &input, quality), off);
    }
    let srgb = check(
        (&device, &queue),
        &input,
        FxaaQuality {
            luminance: FxaaLuminance::Srgb,
            ..q
        },
    );
    assert_ne!(
        srgb, active,
        "nonlinear luminance changes the edge direction"
    );
}

#[test]
fn srgb_luminance_toe_changes_absolute_threshold_admission() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut input = fixture([9, 7], false);
    for pixel in &mut input.pixels {
        let value = if pixel[0] > 0.5 {
            1.0 / 512.0
        } else {
            1.0 / 1024.0
        };
        *pixel = [value, value, value, 0.25];
    }
    let q = FxaaQuality {
        edge_threshold: 0.0,
        minimum_threshold: 0.01,
        ..Default::default()
    };
    let off = check(
        (&device, &queue),
        &input,
        FxaaQuality {
            subpixel_blend: 0.0,
            ..q
        },
    );
    assert_eq!(check((&device, &queue), &input, q), off);
    let srgb = FxaaQuality {
        luminance: FxaaLuminance::Srgb,
        ..q
    };
    assert_ne!(check((&device, &queue), &input, srgb), off);
}
