use super::filter_reference::{Image, fxaa_quality};
use crate::{FxaaKernel, FxaaLuminance, FxaaQuality};

fn quality(kernel: FxaaKernel) -> FxaaQuality {
    FxaaQuality {
        edge_threshold: 0.0,
        minimum_threshold: 0.0,
        subpixel_blend: 0.0,
        luminance: FxaaLuminance::Green,
        kernel,
    }
}
fn axis_edge(vertical: bool) -> Image {
    let mut pixels = vec![[0.125; 4]; 25];
    for y in 0..5 {
        for x in 0..5 {
            let value = if (if vertical { x } else { y }) > 2 {
                0.625
            } else {
                0.125
            };
            pixels[y * 5 + x] = [value, value, value, 0.25];
        }
    }
    Image {
        size: [5, 5],
        pixels,
    }
}

pub(super) fn asymmetric_diagonals() -> Image {
    let mut image = Image {
        size: [5, 5],
        pixels: vec![[0.125, 0.125, 0.125, 0.25]; 25],
    };
    for index in [11, 13] {
        image.pixels[index] = [0.25; 4];
    }
    for index in [6, 18] {
        image.pixels[index] = [0.625, 0.625, 0.625, 0.25];
    }
    image
}

#[test]
fn asymmetric_diagonals_select_horizontal_taps() {
    let image = asymmetric_diagonals();
    // H = 1, V = 1.5. Each X tap is (0.125 + 0.25) / 2 = 0.1875.
    // The centre is 0.5 * 0.125 + 0.25 * (0.1875 + 0.1875) = 0.15625.
    let actual = fxaa_quality(
        &image,
        [0.5, 0.5],
        1.0,
        quality(FxaaKernel::SecondDifference),
    );
    assert_eq!(
        actual.map(f32::to_bits),
        [0.15625_f32, 0.15625, 0.15625, 0.25].map(f32::to_bits)
    );
}

#[test]
fn second_differences_select_cross_edge_axis_and_tie_goes_vertical() {
    let q = quality(FxaaKernel::SecondDifference);
    for vertical in [false, true] {
        let image = axis_edge(vertical);
        let actual = fxaa_quality(&image, [0.5, 0.5], 1.0, q);
        assert_eq!(actual.map(f32::to_bits), [0.25_f32; 4].map(f32::to_bits));
    }
    let mut image = axis_edge(false);
    image.pixels.fill([0.125, 0.125, 0.125, 0.25]);
    image.pixels[7] = [0.625, 0.625, 0.625, 0.25];
    image.pixels[11] = [0.625, 0.625, 0.625, 0.25];
    image.pixels[2] = [0.625, 0.125, 0.125, 0.25];
    let actual = fxaa_quality(&image, [0.5, 0.5], 1.0, q);
    assert_eq!(
        actual.map(f32::to_bits),
        [0.25_f32, 0.1875, 0.1875, 0.25].map(f32::to_bits)
    );
}

#[test]
fn both_kernels_preserve_zero_range_and_green_ignores_red_and_blue() {
    let mut image = axis_edge(true);
    for pixel in &mut image.pixels {
        pixel[1] = 0.25;
    }
    for kernel in [FxaaKernel::Sobel, FxaaKernel::SecondDifference] {
        let q = FxaaQuality {
            subpixel_blend: 1.0,
            ..quality(kernel)
        };
        for uv in [[0.5, 0.5], [0.7, 0.5]] {
            assert_eq!(
                fxaa_quality(&image, uv, 1.0, q).map(f32::to_bits),
                image.sample(uv).map(f32::to_bits)
            );
        }
    }
}

#[test]
fn both_thresholds_admit_and_reject_on_either_side() {
    let mut image = axis_edge(true);
    image.pixels[12][0] = 0.875;
    let uv = [0.5, 0.5];
    let original = image.sample(uv);
    for kernel in [FxaaKernel::Sobel, FxaaKernel::SecondDifference] {
        let q = FxaaQuality {
            subpixel_blend: 1.0,
            ..quality(kernel)
        };
        for (edge_threshold, minimum_threshold, admitted) in [
            (0.79, 0.0, true),
            (0.81, 0.0, false),
            (0.0, 0.49, true),
            (0.0, 0.51, false),
        ] {
            let result = fxaa_quality(
                &image,
                uv,
                1.0,
                FxaaQuality {
                    edge_threshold,
                    minimum_threshold,
                    ..q
                },
            );
            if admitted {
                assert_ne!(result.map(f32::to_bits), original.map(f32::to_bits));
            } else if !admitted {
                assert_eq!(result.map(f32::to_bits), original.map(f32::to_bits));
            }
        }
    }
    let q = quality(FxaaKernel::SecondDifference);
    assert_ne!(
        fxaa_quality(
            &image,
            uv,
            1.0,
            FxaaQuality {
                minimum_threshold: 0.5,
                ..q
            }
        )
        .map(f32::to_bits),
        original.map(f32::to_bits)
    );
    assert_ne!(
        fxaa_quality(
            &image,
            uv,
            1.0,
            FxaaQuality {
                edge_threshold: 0.8,
                ..q
            }
        )
        .map(f32::to_bits),
        original.map(f32::to_bits)
    );
}

#[test]
fn second_difference_neighbourhood_subpixel_blend_matches_formula() {
    let image = axis_edge(true);
    let uv = [0.5, 0.5];
    let q = quality(FxaaKernel::SecondDifference);
    let avg = (2.0 * (0.125 + 0.125 + 0.625 + 0.125) + 0.125 + 0.625 + 0.125 + 0.625) / 12.0;
    let ratio = (avg - 0.125_f32).abs() / 0.5;
    for subpixel_blend in [0.0, 0.5, 1.0] {
        let result = fxaa_quality(
            &image,
            uv,
            1.0,
            FxaaQuality {
                subpixel_blend,
                ..q
            },
        );
        let expected = 0.25 * (1.0 - subpixel_blend * ratio) + avg * subpixel_blend * ratio;
        assert!((result[0] - expected).abs() < 1e-6);
        assert_eq!(result[3].to_bits(), 0.25_f32.to_bits());
    }
}
