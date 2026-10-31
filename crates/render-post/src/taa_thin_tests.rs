//! Thin-feature detection on synthetic luminance images at `R`, read back from the candidate
//! field. Background 0.25 and line 2.0 give compressed luminances 0.2 and 0.667, a contrast of
//! 0.467, more than seven times `CONTRAST` (1/16), so the detection of a line does not depend on
//! the threshold. The ramp is monotone, so its centre lies between its neighbours: exactly zero.
use crate::taa_gpu_test_support::{f16_bits, sampled, view, write_texture};
use crate::taa_thin_gpu::{CANDIDATE_FORMAT, stage};
use crate::{FrameConfig, Operation};

const BACKGROUND: f32 = 0.25;
const LINE: f32 = 2.0;

/// Candidate strength per texel, row major, of the grey image `luma(x, y)` at `size`.
fn strengths(size: [u32; 2], luma: impl Fn(i32, i32) -> f32) -> Vec<f32> {
    let (device, queue) = crate::gpu_tests::gpu();
    let texture = sampled(
        &device,
        "thin input",
        size,
        wgpu::TextureFormat::Rgba16Float,
    );
    let mut bytes = Vec::new();
    for y in 0..i32::try_from(size[1]).unwrap() {
        for x in 0..i32::try_from(size[0]).unwrap() {
            let v = f16_bits(luma(x, y));
            for half in [v, v, v, f16_bits(1.0)] {
                bytes.extend_from_slice(&half.to_le_bytes());
            }
        }
    }
    write_texture(&queue, &texture, &bytes, 8);
    let (stage, out) = stage(&device, &view(&texture), size);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let Operation::Compute {
        pipeline,
        groups,
        dispatch,
    } = &stage.operation
    else {
        panic!("the detection is a compute pass");
    };
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &groups[0], &[]);
        pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
    }
    queue.submit([encoder.finish()]);
    let raw = crate::Readback::new(
        &device,
        FrameConfig::new(size, CANDIDATE_FORMAT, 1),
        1_000_000,
    )
    .unwrap()
    .read(&device, &crate::test_authority(&queue), &out)
    .unwrap();
    raw.chunks_exact(8)
        .map(|p| f32::from_le_bytes([p[0], p[1], p[2], p[3]]))
        .collect()
}

fn texel(v: i32) -> f32 {
    f32::from(i16::try_from(v).unwrap())
}

fn detected(size: [u32; 2], strength: &[f32]) -> Vec<(i32, i32)> {
    let width = i32::try_from(size[0]).unwrap();
    (0..strength.len())
        .filter(|i| strength[*i] > 0.0)
        .map(|i| {
            let i = i32::try_from(i).unwrap();
            (i % width, i / width)
        })
        .collect()
}

fn interior(size: [u32; 2], (x, y): (i32, i32)) -> bool {
    let (w, h) = (
        i32::try_from(size[0]).unwrap(),
        i32::try_from(size[1]).unwrap(),
    );
    x > 0 && y > 0 && x < w - 1 && y < h - 1
}

/// A one texel line is detected at every interior texel of its length and nowhere else.
fn assert_line(size: [u32; 2], on_line: impl Fn(i32, i32) -> bool + Copy) {
    let found = strengths(size, |x, y| if on_line(x, y) { LINE } else { BACKGROUND });
    let mut expected = Vec::new();
    for y in 0..i32::try_from(size[1]).unwrap() {
        for x in 0..i32::try_from(size[0]).unwrap() {
            if on_line(x, y) && interior(size, (x, y)) {
                expected.push((x, y));
            }
        }
    }
    assert!(expected.len() >= 7, "the fixture has a line");
    assert_eq!(detected(size, &found), expected);
    assert!(found.iter().all(|v| v.is_finite()));
}

#[test]
fn a_one_texel_line_is_detected_at_zero_forty_five_and_ninety_degrees() {
    assert_line([17, 11], |_, y| y == 5);
    assert_line([17, 11], |x, _| x == 8);
    assert_line([17, 11], |x, y| x - y == 3);
    assert_line([17, 11], |x, y| x + y == 12);
}

#[test]
fn a_grid_of_one_texel_lines_is_detected_with_only_crossing_corners_added() {
    let size = [25, 17];
    let on_line = |x: i32, y: i32| x % 6 == 3 || y % 6 == 3;
    let found = detected(
        size,
        &strengths(size, |x, y| if on_line(x, y) { LINE } else { BACKGROUND }),
    );
    for y in 0..17 {
        for x in 0..25 {
            if on_line(x, y) && interior(size, (x, y)) {
                assert!(found.contains(&(x, y)), "line texel {x},{y} not detected");
            }
        }
    }
    // An off line texel qualifies only as a dark spot between two lines, which meet at a crossing
    // diagonally next to it.
    for (x, y) in found.iter().filter(|(x, y)| !on_line(*x, *y)) {
        assert!(
            (-1..=1).any(|dy| (-1..=1).any(|dx| (x + dx) % 6 == 3 && (y + dy) % 6 == 3)),
            "{x},{y} is not next to a crossing"
        );
    }
}

#[test]
fn flat_regions_gradients_and_wide_features_are_not_detected() {
    let size = [17, 11];
    let flat = strengths(size, |_, _| BACKGROUND);
    assert!(detected(size, &flat).is_empty());
    let ramp = strengths(size, |x, _| 0.1 + 0.9 * texel(x) / 16.0);
    assert!(detected(size, &ramp).is_empty());
    let diagonal_ramp = strengths(size, |x, y| 0.1 + 0.05 * texel(x + y));
    assert!(detected(size, &diagonal_ramp).is_empty());
    // Slope 0.3 per texel, a compressed step of 0.19 down to 0.08 over the first eleven texels,
    // above `CONTRAST`: only the agreement of the neighbours rejects it.
    let steep = strengths(size, |x, _| 0.25 + 0.3 * texel(x));
    assert!(detected(size, &steep).is_empty());
    let edge = strengths(size, |x, _| if x < 8 { BACKGROUND } else { LINE });
    assert!(detected(size, &edge).is_empty());
    let diagonal_edge = strengths(size, |x, y| if x + y < 12 { BACKGROUND } else { LINE });
    assert!(detected(size, &diagonal_edge).is_empty());
    let wide = strengths(size, |_, y| {
        if (4..=6).contains(&y) {
            LINE
        } else {
            BACKGROUND
        }
    });
    assert!(detected(size, &wide).is_empty());
    let two = strengths(size, |_, y| {
        if (4..=5).contains(&y) {
            LINE
        } else {
            BACKGROUND
        }
    });
    assert!(detected(size, &two).is_empty());
}

#[test]
fn the_border_ring_and_tiny_extents_never_hold_candidates() {
    let size = [17, 11];
    assert!(
        detected(
            size,
            &strengths(size, |_, y| if y == 0 { LINE } else { BACKGROUND })
        )
        .is_empty()
    );
    assert!(
        detected(
            size,
            &strengths(size, |x, _| if x == 16 { LINE } else { BACKGROUND })
        )
        .is_empty()
    );
    let inner = strengths(size, |_, y| if y == 1 { LINE } else { BACKGROUND });
    assert_eq!(detected(size, &inner).len(), 15);
    for tiny in [[1, 1], [2, 2], [3, 1], [1, 5]] {
        let all = strengths(tiny, |_, _| LINE);
        assert_eq!(all.len(), (tiny[0] * tiny[1]) as usize);
        assert!(detected(tiny, &all).is_empty());
    }
    let spot = strengths(
        [3, 3],
        |x, y| if (x, y) == (1, 1) { LINE } else { BACKGROUND },
    );
    assert_eq!(detected([3, 3], &spot), [(1, 1)]);
}

#[test]
fn a_dark_wire_is_detected_like_a_bright_one() {
    let size = [17, 11];
    let found = strengths(size, |_, y| if y == 5 { 0.01 } else { 1.0 });
    assert_eq!(detected(size, &found).len(), 15);
}
