use crate::{FxaaKernel, FxaaLuminance, FxaaQuality};

pub(super) fn quality(scale: [f32; 2]) -> FxaaQuality {
    FxaaQuality {
        edge_threshold: 0.166,
        minimum_threshold: 0.0833,
        subpixel_blend: 0.75,
        luminance: FxaaLuminance::Green,
        kernel: FxaaKernel::Quality {
            preset: 12,
            sample_scale: scale,
        },
    }
}

#[test]
fn quality_preset_and_scale_are_validated() {
    let valid = quality([0.5, 1.0]);
    assert!(valid.validate().is_ok());
    for preset in [0, 11, 13, 20, 255] {
        assert!(
            FxaaQuality {
                kernel: FxaaKernel::Quality {
                    preset,
                    sample_scale: [1.0; 2]
                },
                ..valid
            }
            .validate()
            .is_err()
        );
    }
    for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, 4.01] {
        assert!(quality([scale, 1.0]).validate().is_err());
        assert!(quality([1.0, scale]).validate().is_err());
    }
    assert_eq!(FxaaQuality::default().kernel, FxaaKernel::Sobel);
}

fn fixture(kind: u8) -> super::filter_reference::Image {
    let size = [9_u16, 7];
    let mut pixels = Vec::new();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let value = match kind {
                0 => {
                    if x > y {
                        0.75
                    } else {
                        0.125
                    }
                }
                1 => {
                    if x == y {
                        0.75
                    } else {
                        0.125
                    }
                }
                2 => {
                    if x > y {
                        0.289_062_5
                    } else {
                        0.203_125
                    }
                }
                4 => {
                    if x > y {
                        0.28125
                    } else {
                        0.203_125
                    }
                }
                _ => 0.375,
            };
            pixels.push([value, value, value, 0.625]);
        }
    }
    super::filter_reference::Image { size, pixels }
}

fn reference(
    image: &super::filter_reference::Image,
    q: FxaaQuality,
) -> super::filter_reference::Image {
    let mut pixels = Vec::new();
    for y in 0..image.size[1] {
        for x in 0..image.size[0] {
            pixels.push(super::fxaa_pc_reference::resolve(image, [x, y], q));
        }
    }
    super::filter_reference::Image {
        size: image.size,
        pixels,
    }
}

fn compare(
    gpu: (&wgpu::Device, &wgpu::Queue),
    input: &super::filter_reference::Image,
    q: FxaaQuality,
) -> Vec<u8> {
    let expected = reference(input, q);
    assert!(input.pixels.iter().all(|p| p[0] > 0.0 && p[3] > 0.0));
    assert!(expected.pixels.iter().all(|p| p[0] > 0.0 && p[3] > 0.0));
    let actual = super::filter_reference_gpu_tests::render(
        gpu,
        "fxaa",
        (input, None),
        &expected,
        wgpu::TextureFormat::Rgba8Unorm,
        &q.constants(1.0),
    );
    for (index, (&byte, value)) in actual
        .iter()
        .zip(expected.pixels.iter().flatten())
        .enumerate()
    {
        assert!(
            (f32::from(byte) - (value * 255.0).round()).abs() <= 1.0,
            "index={index} actual={byte} expected={value} quality={q:?}"
        );
    }
    actual
}

#[test]
fn quality_gpu_matches_diagonal_line_threshold_and_nonzero_flat_reference() {
    let (device, queue) = crate::gpu_tests::gpu();
    for scale in [[1.0; 2], [0.5; 2], [0.5, 1.5]] {
        for kind in 0..5 {
            let input = fixture(kind);
            let q = quality(scale);
            let expected = reference(&input, q);
            if kind >= 3 {
                assert_eq!(input.pixels, expected.pixels);
            } else {
                assert_ne!(input.pixels, expected.pixels);
            }
            compare((&device, &queue), &input, q);
        }
    }
}

#[test]
fn quality_axial_range_rejects_small_contrast_despite_bright_corners() {
    let mut input = super::filter_reference::Image {
        size: [5, 5],
        pixels: vec![[0.25, 0.25, 0.25, 0.625]; 25],
    };
    for (index, value) in [
        (6, 0.328_125),
        (7, 0.328_125),
        (8, 0.875),
        (16, 0.875),
        (18, 0.875),
    ] {
        input.pixels[index] = [value, value, value, 0.625];
    }
    let q = quality([1.0; 2]);
    let axial_range = input.pixels[7][1] - input.pixels[12][1];
    assert!(axial_range > 0.0 && axial_range < q.minimum_threshold);
    assert!(input.pixels[8][1] - input.pixels[12][1] > q.minimum_threshold);
    let axial = super::fxaa_pc_reference::resolve(&input, [2, 2], q);
    let nine = super::fxaa_pc_reference::resolve_with_search::<4>(&input, [2, 2], q, true);
    assert_eq!(axial.map(f32::to_bits), input.pixels[12].map(f32::to_bits));
    assert!(((nine[1] * 255.0).round() - (axial[1] * 255.0).round()).abs() > 1.0);
    let (device, queue) = crate::gpu_tests::gpu();
    let actual = compare((&device, &queue), &input, q);
    assert_eq!(&actual[48..52], &[64, 64, 64, 159]);
}

#[test]
fn quality_weighted_scores_choose_the_other_axis() {
    let grid = [[0.125, 0.75, 0.25], [0.5, 0.25, 0.125], [0.75, 0.125, 0.75]];
    assert_eq!(
        super::fxaa_pc_reference::scores(grid).map(f32::to_bits),
        [1.625_f32, 2.625].map(f32::to_bits)
    );
    let old_h = 1.625_f32;
    let old_v = 1.125_f32;
    assert!(old_h > old_v);
    let mut input = super::filter_reference::Image {
        size: [5, 5],
        pixels: vec![[0.25, 0.25, 0.25, 0.625]; 25],
    };
    for (y, row) in grid.iter().enumerate() {
        for (x, &value) in row.iter().enumerate() {
            input.pixels[(y + 1) * 5 + x + 1] = [value, value, value, 0.625];
        }
    }
    let q = quality([1.0; 2]);
    let expected = reference(&input, q);
    assert_ne!(expected.pixels, input.pixels);
    let (device, queue) = crate::gpu_tests::gpu();
    compare((&device, &queue), &input, q);
}

#[test]
fn quality_option_preserves_implicit_default_bytes() {
    let (device, queue) = crate::gpu_tests::gpu();
    let input = fixture(0);
    assert_ne!(
        input.pixels[0].map(f32::to_bits),
        input.pixels[8].map(f32::to_bits)
    );
    let render = |constants: &[(&str, f64)]| {
        super::filter_reference_gpu_tests::render(
            (&device, &queue),
            "fxaa",
            (&input, None),
            &input,
            wgpu::TextureFormat::Rgba8Unorm,
            constants,
        )
    };
    let implicit = render(&[]);
    assert!(implicit.iter().any(|&v| v > 0));
    assert_eq!(implicit, render(&FxaaQuality::default().constants(0.75)));
    assert_ne!(
        implicit,
        compare((&device, &queue), &input, quality([1.0; 2]))
    );
}

#[test]
fn quality_search_scale_changes_nonzero_edge_output() {
    let (device, queue) = crate::gpu_tests::gpu();
    let input = fixture(0);
    let full = compare((&device, &queue), &input, quality([1.0; 2]));
    let half = compare((&device, &queue), &input, quality([0.5; 2]));
    assert_ne!(full, half);
}

#[test]
fn quality_preset_terminal_extension_retains_last_sample() {
    let input = fixture(3);
    assert!(input.pixels.iter().all(|p| p[0] > 0.0));
    let (distance, deviation) = super::fxaa_pc_reference::endpoint(
        &input,
        [4.5, 3.5],
        [1.0, 0.0],
        0.375,
        0.5,
        quality([1.0; 2]),
    );
    assert_eq!(distance.to_bits(), 20.5_f32.to_bits());
    assert_eq!(deviation.to_bits(), 0.0_f32.to_bits());
}

fn distant_endpoint_fixture() -> super::filter_reference::Image {
    let size = [49_u16, 5];
    let mut pixels = Vec::new();
    for y in 0..size[1] {
        for x in 0..size[0] {
            let value = match x {
                0..=20 => 0.75,
                32..=33 => 0.5625,
                44..=48 => 0.25,
                _ if y < 2 => 0.75,
                _ => 0.25,
            };
            pixels.push([value, value, value, 0.625]);
        }
    }
    super::filter_reference::Image { size, pixels }
}

#[test]
fn quality_gpu_resolves_asymmetric_late_endpoints() {
    let input = distant_endpoint_fixture();
    let q = FxaaQuality {
        subpixel_blend: 0.0,
        ..quality([1.0; 2])
    };
    let origin = [24.5, 2.0];
    let positive = super::fxaa_pc_reference::endpoint(&input, origin, [1.0, 0.0], 0.5, 0.5, q);
    let negative = super::fxaa_pc_reference::endpoint(&input, origin, [-1.0, 0.0], 0.5, 0.5, q);
    assert_eq!(positive, (20.5, 0.0625));
    assert_eq!(negative, (4.5, 0.25));
    // The extension lands on the opposite sign, but must retain the 8.5 sample.
    let terminal = input.sample([45.0 / 49.0, 2.0 / 5.0])[1] - 0.5;
    assert!(terminal < 0.0 && positive.1 > 0.0);
    let pixel = [24, 2];
    let full = super::fxaa_pc_reference::resolve(&input, pixel, q);
    let short = super::fxaa_pc_reference::resolve_with_search::<2>(&input, pixel, q, false);
    let no_extension = super::fxaa_pc_reference::resolve_with_search::<3>(&input, pixel, q, false);
    assert!(((full[1] * 255.0).round() - (short[1] * 255.0).round()).abs() > 1.0);
    assert!(((full[1] * 255.0).round() - (no_extension[1] * 255.0).round()).abs() > 1.0);
    assert!((full[1] - input.at(24, 2)[1]).abs() * 255.0 > 1.0);
    let (device, queue) = crate::gpu_tests::gpu();
    compare((&device, &queue), &input, q);
}
