use super::filter_reference::Image;
use super::filter_reference_gpu_tests::run;

#[test]
fn odd_bloom_reconstruction_matches_hand_computed_edges() {
    let (device, queue) = crate::gpu_tests::gpu();
    let high = Image {
        size: [3, 3],
        pixels: vec![[2.0, 4.0, 6.0, 0.25]; 9],
    };
    let low = Image {
        size: [2, 2],
        pixels: vec![
            [0.0, 0.0, 0.0, 0.5],
            [48.0, 0.0, 24.0, 0.5],
            [0.0, 48.0, 24.0, 0.5],
            [48.0, 48.0, 48.0, 0.5],
        ],
    };
    // High centers map to low coordinates -1/6, 1/2, 7/6. Clamped
    // [1,2,1]/4 taps give right-texel weights 5/24, 1/2, 19/24.
    // A 48-unit step therefore reconstructs 10, 24, 38 before averaging.
    let expected = Image {
        size: [3, 3],
        pixels: vec![
            [6.0, 7.0, 8.0, 1.0],
            [13.0, 7.0, 11.5, 1.0],
            [20.0, 7.0, 15.0, 1.0],
            [6.0, 14.0, 11.5, 1.0],
            [13.0, 14.0, 15.0, 1.0],
            [20.0, 14.0, 18.5, 1.0],
            [6.0, 21.0, 15.0, 1.0],
            [13.0, 21.0, 18.5, 1.0],
            [20.0, 21.0, 22.0, 1.0],
        ],
    };
    run(
        (&device, &queue),
        "upsample",
        (&high, Some(&low)),
        &expected,
        wgpu::TextureFormat::Rgba16Float,
        &[],
    );
}

fn colored_pixel(luminance: f32, offset: f32) -> [f32; 4] {
    [
        luminance + 2.0 * offset,
        luminance - offset,
        luminance,
        0.75,
    ]
}

fn cutoff_case(base: f32, step: f32, filters_center: bool, transpose: bool) -> (Image, Image) {
    let mut input = Image {
        size: [3, 3],
        pixels: Vec::new(),
    };
    let mut expected = input.clone();
    for y in 0..3 {
        for x in 0..3 {
            let (across, along) = if transpose { (y, x) } else { (x, y) };
            let luminance = base + [0.0, step, 2.0 * step][across];
            let original_offset = [0.0, 1.0 / 32.0, 0.0][along];
            input.pixels.push(colored_pixel(luminance, original_offset));
            // Luma is exactly base + across*step: the chromatic offset cancels.
            // The gradient points across; its tangent reaches 1.5 pixels along.
            // At an edge the side sample has offset 1/64, weighted by 1/4.
            // At the center both side taps clamp to zero, leaving half of 1/32.
            let offset = if across == 1 && filters_center {
                [1.0 / 256.0, 1.0 / 64.0, 1.0 / 256.0][along]
            } else {
                original_offset
            };
            expected.pixels.push(colored_pixel(luminance, offset));
        }
    }
    (input, expected)
}

#[test]
fn channel_varying_fxaa_matches_hand_computed_cutoffs_and_tangents() {
    let (device, queue) = crate::gpu_tests::gpu();
    // Center contrast is twice the step; border contrast is only one step.
    // The first three straddle 1/64 by 1/512. The last three straddle
    // maximum/10, with exact equality at contrast 1/16 and maximum 5/8.
    // All border pixels remain below their cutoff in every case.
    for (base, step, filters_center) in [
        (1.0 / 16.0, 7.0 / 1024.0, false),
        (1.0 / 16.0, 8.0 / 1024.0, false),
        (1.0 / 16.0, 9.0 / 1024.0, true),
        (9.0 / 16.0, 31.0 / 1024.0, false),
        (9.0 / 16.0, 32.0 / 1024.0, false),
        (9.0 / 16.0, 33.0 / 1024.0, true),
    ] {
        for transpose in [false, true] {
            let (input, expected) = cutoff_case(base, step, filters_center, transpose);
            run(
                (&device, &queue),
                "fxaa",
                (&input, None),
                &expected,
                wgpu::TextureFormat::Rgba16Float,
                &[("FXAA", 1.0)],
            );
        }
    }
}

#[test]
fn configurable_fxaa_cutoffs_match_hand_computed_neighbors() {
    let (device, queue) = crate::gpu_tests::gpu();
    for (base, step, edge, minimum, active) in [
        (1.0 / 16.0, 15.0 / 1024.0, 0.0, 1.0 / 32.0, false),
        (1.0 / 16.0, 16.0 / 1024.0, 0.0, 1.0 / 32.0, false),
        (1.0 / 16.0, 17.0 / 1024.0, 0.0, 1.0 / 32.0, true),
        (3.0 / 8.0, 63.0 / 1024.0, 0.25, 0.0, false),
        (3.0 / 8.0, 64.0 / 1024.0, 0.25, 0.0, false),
        (3.0 / 8.0, 65.0 / 1024.0, 0.25, 0.0, true),
    ] {
        for transpose in [false, true] {
            let (input, expected) = cutoff_case(base, step, active, transpose);
            run(
                (&device, &queue),
                "fxaa",
                (&input, None),
                &expected,
                wgpu::TextureFormat::Rgba16Float,
                &[
                    ("FXAA", 1.0),
                    ("EDGE_THRESHOLD", edge),
                    ("MINIMUM_THRESHOLD", minimum),
                ],
            );
        }
    }
}
