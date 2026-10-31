use crate::{PostError, ProjectionJitter};
use worldrunner_render_plan::post::RenderScale;

fn bits(offset: [f32; 2]) -> [u32; 2] {
    offset.map(f32::to_bits)
}

fn halton(index: u32, base: u32) -> f64 {
    let (mut result, mut fraction, mut index) = (0.0, 1.0, index);
    while index > 0 {
        fraction /= f64::from(base);
        result += fraction * f64::from(index % base);
        index /= base;
    }
    result
}

#[test]
fn default_sequence_has_eight_samples() {
    assert_eq!(ProjectionJitter::default().length(), 8);
    assert_eq!(
        ProjectionJitter::default(),
        ProjectionJitter::new(8).unwrap()
    );
}

#[test]
fn first_eight_offsets_are_the_halton_two_three_points_centred_on_the_pixel() {
    let expected: [(f64, f64); 8] = [
        (1.0 / 2.0, 1.0 / 3.0),
        (1.0 / 4.0, 2.0 / 3.0),
        (3.0 / 4.0, 1.0 / 9.0),
        (1.0 / 8.0, 4.0 / 9.0),
        (5.0 / 8.0, 7.0 / 9.0),
        (3.0 / 8.0, 2.0 / 9.0),
        (7.0 / 8.0, 5.0 / 9.0),
        (1.0 / 16.0, 8.0 / 9.0),
    ];
    let jitter = ProjectionJitter::default();
    for (index, (x, y)) in expected.into_iter().enumerate() {
        let [got_x, got_y] = jitter.offset_pixels(index as u64);
        assert!((f64::from(got_x) - (x - 0.5)).abs() < 1e-7, "x at {index}");
        assert!((f64::from(got_y) - (y - 0.5)).abs() < 1e-7, "y at {index}");
    }
}

#[test]
fn offsets_wrap_after_the_sequence_length() {
    let jitter = ProjectionJitter::new(5).unwrap();
    for index in 0..5_u64 {
        assert_eq!(
            bits(jitter.offset_pixels(index)),
            bits(jitter.offset_pixels(index + 5))
        );
        assert_eq!(
            bits(jitter.offset_pixels(index)),
            bits(jitter.offset_pixels(index + 5 * 1_000_000_007))
        );
    }
    assert_ne!(bits(jitter.offset_pixels(0)), bits(jitter.offset_pixels(1)));
}

#[test]
fn every_offset_lies_strictly_inside_the_pixel_for_every_length() {
    for length in 1..=ProjectionJitter::MAX_LENGTH {
        let jitter = ProjectionJitter::new(length).unwrap();
        for index in 0..u64::from(length) {
            for value in jitter.offset_pixels(index) {
                assert!(value > -0.5 && value < 0.5, "{value} at {index}/{length}");
            }
        }
    }
}

#[test]
fn offsets_follow_the_halton_sequence_beyond_the_first_eight() {
    let jitter = ProjectionJitter::new(64).unwrap();
    for index in 0..64_u32 {
        let [x, y] = jitter.offset_pixels(u64::from(index));
        assert!((f64::from(x) - (halton(index + 1, 2) - 0.5)).abs() < 1e-6);
        assert!((f64::from(y) - (halton(index + 1, 3) - 0.5)).abs() < 1e-6);
    }
}

#[test]
fn a_sequence_of_one_is_a_fixed_offset() {
    let jitter = ProjectionJitter::new(1).unwrap();
    assert_eq!(
        bits(jitter.offset_pixels(0)),
        bits(jitter.offset_pixels(41))
    );
}

#[test]
fn clip_offset_maps_pixels_to_ndc_with_screen_y_down() {
    let jitter = ProjectionJitter::default();
    let size = [1920, 1080];
    for index in 0..8_u64 {
        let [px, py] = jitter.offset_pixels(index);
        let [cx, cy] = jitter.clip_offset(index, size);
        assert!((cx - px * 2.0 / 1920.0).abs() < 1e-9);
        assert!((cy + py * 2.0 / 1080.0).abs() < 1e-9);
    }
}

#[test]
fn clip_offset_inverts_back_to_the_pixel_offset() {
    let jitter = ProjectionJitter::default();
    for size in [[1, 1], [640, 360], [1921, 1081], [7680, 4320]] {
        for index in 0..8_u64 {
            let [px, py] = jitter.offset_pixels(index);
            let [cx, cy] = jitter.clip_offset(index, size);
            let [width, height] = size.map(|extent| f32::from(u16::try_from(extent).unwrap()));
            let back = [cx * width / 2.0, -cy * height / 2.0];
            assert!((back[0] - px).abs() < 1e-5, "x {size:?} {index}");
            assert!((back[1] - py).abs() < 1e-5, "y {size:?} {index}");
        }
    }
}

#[test]
fn a_single_sample_target_keeps_the_sequence_and_a_multisampled_one_is_not_jittered() {
    let jitter = ProjectionJitter::default();
    for index in 0..8_u64 {
        assert_eq!(
            bits(jitter.offset_pixels_for(index, 1)),
            bits(jitter.offset_pixels(index))
        );
        assert_eq!(
            bits(jitter.clip_offset_for(index, [64, 64], 1)),
            bits(jitter.clip_offset(index, [64, 64]))
        );
        for samples in [2, 4, 8] {
            let offsets = [
                jitter.offset_pixels_for(index, samples),
                jitter.clip_offset_for(index, [64, 64], samples),
            ];
            assert!(offsets.iter().flatten().all(|v| v.abs() < 1e-12));
        }
    }
}

#[test]
fn lengths_outside_one_to_sixty_four_are_rejected() {
    for length in [0, 65, 128, u32::MAX] {
        assert!(
            matches!(
                ProjectionJitter::new(length),
                Err(PostError::UnsupportedProfile(_))
            ),
            "{length}"
        );
    }
    assert!(ProjectionJitter::new(1).is_ok());
    assert!(ProjectionJitter::new(64).is_ok());
}

const OUTPUTS: [[u32; 2]; 2] = [[1920, 1080], [1452, 921]];

fn scaled(output: [u32; 2], millionths: u32) -> [u32; 2] {
    RenderScale::new(millionths)
        .unwrap()
        .extent(output)
        .unwrap()
}

fn length_at(output: [u32; 2], millionths: u32) -> u32 {
    ProjectionJitter::for_extents(scaled(output, millionths), output)
        .unwrap()
        .length()
}

#[test]
fn for_extents_lengths_match_the_requested_scales_at_both_outputs() {
    for output in OUTPUTS {
        assert_eq!(length_at(output, 1_000_000), 8, "{output:?} native");
        assert_eq!(length_at(output, 500_000), 32, "{output:?} at 0.5");
        assert_eq!(length_at(output, 670_000), 18, "{output:?} at 0.67");
        assert_eq!(length_at(output, 770_000), 14, "{output:?} at 0.77");
    }
}

#[test]
fn for_extents_handles_odd_rounded_extents() {
    assert_eq!(scaled([1453, 919], 670_000), [974, 616]);
    assert_eq!(length_at([1453, 919], 670_000), 18);
    assert_eq!(scaled([1453, 919], 500_000), [727, 460]);
    assert_eq!(length_at([1453, 919], 500_000), 32);
}

#[test]
fn for_extents_uses_the_axis_with_the_larger_ratio() {
    // Width ratio 1.0, height ratio 2.0: the height axis sets the length.
    let jitter = ProjectionJitter::for_extents([100, 50], [100, 100]).unwrap();
    assert_eq!(jitter.length(), 32);
    let jitter = ProjectionJitter::for_extents([50, 100], [100, 100]).unwrap();
    assert_eq!(jitter.length(), 32);
}

#[test]
fn for_extents_rounds_the_length_up_from_exact_integer_arithmetic() {
    // 8 * 3^2 / 2^2 = 18 exactly: no rounding up.
    assert_eq!(
        ProjectionJitter::for_extents([2, 2], [3, 3])
            .unwrap()
            .length(),
        18
    );
    // 8 * 4^2 / 3^2 = 14.2..: rounds up to 15.
    assert_eq!(
        ProjectionJitter::for_extents([3, 3], [4, 4])
            .unwrap()
            .length(),
        15
    );
    // Extents near u32::MAX must not overflow the product, and a ratio a hair under 2 stays 32.
    let half_up = 1_u32 << 31;
    let jitter = ProjectionJitter::for_extents([half_up, half_up], [u32::MAX, u32::MAX]).unwrap();
    assert_eq!(jitter.length(), 32);
    let jitter = ProjectionJitter::for_extents([u32::MAX, u32::MAX], [u32::MAX, u32::MAX]);
    assert_eq!(jitter.unwrap().length(), 8);
}

#[test]
fn for_extents_at_equal_extents_is_exactly_the_default_sequence() {
    for extent in [[1920, 1080], [1, 1], [1453, 919]] {
        let jitter = ProjectionJitter::for_extents(extent, extent).unwrap();
        assert_eq!(jitter, ProjectionJitter::default());
        for index in 0..20_u64 {
            assert_eq!(
                bits(jitter.offset_pixels(index)),
                bits(ProjectionJitter::default().offset_pixels(index))
            );
        }
    }
}

#[test]
fn for_extents_offsets_are_a_prefix_of_the_halton_generator_inside_half_a_pixel() {
    let longest = ProjectionJitter::new(ProjectionJitter::MAX_LENGTH).unwrap();
    for output in OUTPUTS {
        for millionths in [500_000, 670_000, 770_000] {
            let jitter = ProjectionJitter::for_extents(scaled(output, millionths), output).unwrap();
            for index in 0..u64::from(jitter.length()) {
                let offset = jitter.offset_pixels(index);
                assert_eq!(bits(offset), bits(longest.offset_pixels(index)));
                let point = u32::try_from(index).unwrap() + 1;
                assert!((f64::from(offset[0]) - (halton(point, 2) - 0.5)).abs() < 1e-6);
                assert!((f64::from(offset[1]) - (halton(point, 3) - 0.5)).abs() < 1e-6);
                assert!(offset.iter().all(|value| value.abs() < 0.5), "{offset:?}");
            }
        }
    }
}

#[test]
fn for_extents_rejects_empty_extents() {
    for (render, output) in [
        ([0, 10], [20, 20]),
        ([10, 0], [20, 20]),
        ([10, 10], [0, 20]),
        ([10, 10], [20, 0]),
        ([0, 0], [0, 0]),
    ] {
        assert_eq!(
            ProjectionJitter::for_extents(render, output),
            Err(PostError::EmptyExtent),
            "{render:?} in {output:?}"
        );
    }
}

#[test]
fn for_extents_rejects_a_render_extent_larger_than_the_output_on_either_axis() {
    for (render, output) in [
        ([21, 20], [20, 20]),
        ([20, 21], [20, 20]),
        ([40, 40], [20, 20]),
    ] {
        assert_eq!(
            ProjectionJitter::for_extents(render, output),
            Err(PostError::RenderExtent),
            "{render:?} in {output:?}"
        );
    }
}

#[test]
fn for_extents_rejects_a_ratio_below_one_half_on_either_axis() {
    let rejected = [
        ([9, 20], [20, 20]),
        ([20, 9], [20, 20]),
        ([1, 3], [3, 3]),
        ([1919, 1079], [3840, 2160]),
        ([2_147_483_647, 2_147_483_648], [u32::MAX, u32::MAX]),
    ];
    for (render, output) in rejected {
        assert!(
            matches!(
                ProjectionJitter::for_extents(render, output),
                Err(PostError::UnsupportedProfile(_))
            ),
            "{render:?} in {output:?}"
        );
    }
    assert!(ProjectionJitter::for_extents([10, 10], [20, 20]).is_ok());
    assert!(ProjectionJitter::for_extents([1920, 1080], [3840, 2160]).is_ok());
}
