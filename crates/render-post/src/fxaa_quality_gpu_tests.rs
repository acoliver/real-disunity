use super::filter_reference::{Image, fxaa_quality, half_bits};
use super::filter_reference_gpu_tests::render;
use crate::{FxaaKernel, FxaaLuminance, FxaaQuality};

#[test]
fn asymmetric_diagonals_have_independently_calculated_centre() {
    let (device, queue) = crate::gpu_tests::gpu();
    let input = super::fxaa_kernel_tests::asymmetric_diagonals();
    let q = FxaaQuality {
        edge_threshold: 0.0,
        minimum_threshold: 0.0,
        subpixel_blend: 0.0,
        luminance: FxaaLuminance::Green,
        kernel: FxaaKernel::SecondDifference,
    };
    // H = 1, V = 1.5 selects X taps of 0.1875, yielding centre RGB 0.15625.
    let expected = [0.15625_f32, 0.15625, 0.15625, 0.25];
    let half = render(
        (&device, &queue),
        "fxaa",
        (&input, None),
        &input,
        wgpu::TextureFormat::Rgba16Float,
        &q.constants(1.0),
    );
    for (bytes, value) in half[12 * 8..13 * 8].chunks_exact(2).zip(expected) {
        assert_eq!(
            u16::from_le_bytes(bytes.try_into().unwrap()),
            half_bits(value)
        );
    }
    for (channel, value) in expected.into_iter().enumerate() {
        let value = f64::from(value);
        let encoded = if channel < 3 {
            1.055 * value.powf(1.0 / 2.4) - 0.055
        } else {
            value
        } * 255.0;
        assert!((encoded - (encoded.floor() + 0.5)).abs() >= 0.1);
    }
    let srgb = render(
        (&device, &queue),
        "fxaa",
        (&input, None),
        &input,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        &q.constants(1.0),
    );
    assert_eq!(&srgb[12 * 4..13 * 4], &[110, 110, 110, 64]);
}

#[test]
fn both_kernels_axis_edges_thresholds_and_zero_green_range() {
    let (device, queue) = crate::gpu_tests::gpu();
    for kernel in [FxaaKernel::Sobel, FxaaKernel::SecondDifference] {
        for vertical in [false, true] {
            let mut input = Image {
                size: [5, 5],
                pixels: vec![[0.125; 4]; 25],
            };
            for y in 0..5 {
                for x in 0..5 {
                    let value = if (if vertical { x } else { y }) > 2 {
                        0.625
                    } else {
                        0.125
                    };
                    input.pixels[y * 5 + x] = [value, value, value, 0.25];
                }
            }
            input.pixels[12][0] = 0.875;
            let q = FxaaQuality {
                kernel,
                luminance: FxaaLuminance::Green,
                ..Default::default()
            };
            for (edge_threshold, minimum_threshold) in
                [(0.79, 0.0), (0.81, 0.0), (0.0, 0.49), (0.0, 0.51)]
            {
                check(
                    (&device, &queue),
                    &input,
                    FxaaQuality {
                        edge_threshold,
                        minimum_threshold,
                        ..q
                    },
                );
            }
            for pixel in &mut input.pixels {
                pixel[1] = 0.25;
            }
            check(
                (&device, &queue),
                &input,
                FxaaQuality {
                    edge_threshold: 0.0,
                    minimum_threshold: 0.0,
                    ..q
                },
            );
        }
    }
}

#[test]
fn both_kernels_srgb_readbacks_and_default_bytes() {
    let (device, queue) = crate::gpu_tests::gpu();
    let mut input = Image {
        size: [5, 5],
        pixels: vec![[0.125, 0.125, 0.125, 0.25]; 25],
    };
    for y in 0..5 {
        for x in 3..5 {
            input.pixels[y * 5 + x] = [0.515_625, 0.515_625, 0.515_625, 0.25];
        }
    }
    for kernel in [FxaaKernel::Sobel, FxaaKernel::SecondDifference] {
        let q = FxaaQuality {
            kernel,
            subpixel_blend: 0.0,
            luminance: FxaaLuminance::Green,
            ..Default::default()
        };
        let expected = Image::render(input.size, |uv| fxaa_quality(&input, uv, 1.0, q));
        for pixel in &expected.pixels {
            for &value in &pixel[..3] {
                let encoded = (1.055 * f64::from(value).powf(1.0 / 2.4) - 0.055) * 255.0;
                assert!((encoded - (encoded.floor() + 0.5)).abs() >= 0.1);
            }
        }
        super::filter_reference_gpu_tests::run(
            (&device, &queue),
            "fxaa",
            (&input, None),
            &expected,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &q.constants(1.0),
        );
    }
    let expected = Image::render(input.size, |uv| {
        fxaa_quality(&input, uv, 0.75, FxaaQuality::default())
    });
    let old_defaults = render(
        (&device, &queue),
        "fxaa",
        (&input, None),
        &expected,
        wgpu::TextureFormat::Rgba16Float,
        &[],
    );
    let explicit = check((&device, &queue), &input, FxaaQuality::default());
    assert_eq!(old_defaults, explicit);
}

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
                "channel={channel}, distance={distance}, q={q:?}, reference={reference:?}, actual={bits:#x}"
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
    for kernel in [FxaaKernel::Sobel, FxaaKernel::SecondDifference] {
        for size in [[9, 7], [1, 1]] {
            for thin in [false, true] {
                let input = fixture(size, thin);
                for luminance in [
                    FxaaLuminance::Linear,
                    FxaaLuminance::Srgb,
                    FxaaLuminance::Green,
                ] {
                    for subpixel_blend in [0.0, 0.5, 1.0] {
                        check(
                            (&device, &queue),
                            &input,
                            FxaaQuality {
                                edge_threshold: 0.2,
                                minimum_threshold: 0.03125,
                                subpixel_blend,
                                luminance,
                                kernel,
                            },
                        );
                    }
                }
            }
        }
    }
    let input = Image {
        size: [9, 7],
        pixels: vec![[0.25, 0.5, 0.75, 0.125]; 63],
    };
    for luminance in [
        FxaaLuminance::Linear,
        FxaaLuminance::Srgb,
        FxaaLuminance::Green,
    ] {
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
