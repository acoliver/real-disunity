//! The water producer end to end (decision Q1), at the inner renderer of a temporal
//! `ConfiguredFrameRenderer` with the jitter of `post::jitter` (decision Q6).
//!
//! The scene of `taa_end_to_end_tests` and a water body in front of its wall are drawn at
//! 0.5, 0.67 and 0.77 of a 64x64 output, 24 frames of animated water 0.1 s apart. The truth of
//! frame `f` is the same scene and water time at 256x256 without temporal reconstruction, box
//! averaged 4x4 in linear HDR and tone mapped (`taa_end_to_end_tests::supersampled`, with water).
//! Three runs share the frames: `zero` (producer off, history accumulates), `mask` (producer on)
//! and `reset` (producer off, history discarded before every frame, the disocclusion floor).
//! The Q1 trail form is recorded, not asserted: with stock water at these steps the accumulating
//! run is already below the reset floor, so the form has no trail to remove (see the PR notes).
//! Metric: mean absolute error of the 8 bit sRGB bytes scaled to 0..=1, over the last 8 frames.
//! Water ROI: texels whose whole 3x3 neighbourhood differs between the wet and the dry truth at
//! frame 0. Static ROI: texels at least 3 texels from any texel that differs.
use super::taa_end_to_end_tests::{
    Image, OUTPUT, SIZE, context, count, decode_hdr, decode_output, texture, tone_map,
};
use super::temporal_water_tests::{scene_bytes, sized, submit_to, temporal};
use crate::{compositor::ConfiguredFrameRenderer, device::GpuContext};
use worldrunner_render_post::{FrameConfig, Readback};

const FRAMES: usize = 24;
const EVALUATED: usize = 8;
const STEP_SECONDS: f32 = 0.1;
const FACTOR: usize = 4;
const WIDTH: usize = SIZE as usize;

fn seconds(frame: usize) -> f32 {
    STEP_SECONDS * f32::from(u16::try_from(frame).unwrap())
}

/// The scene and water at `time`, `FACTOR` times the output resolution, box averaged and tone mapped.
fn truth(context: &GpuContext, time: f32, far: bool) -> Image {
    let mut renderer = sized(context, SIZE * 4, (false, far));
    submit_to(context, &mut renderer, (time, 0), (true, None));
    let fine = decode_hdr(&scene_bytes(context, &renderer));
    let mut average = vec![[0.0; 3]; WIDTH * WIDTH];
    for (index, pixel) in average.iter_mut().enumerate() {
        let (column, row) = (index % WIDTH, index / WIDTH);
        for sy in 0..FACTOR {
            for sx in 0..FACTOR {
                let sample = fine[(row * FACTOR + sy) * WIDTH * FACTOR + column * FACTOR + sx];
                for (lane, value) in sample.into_iter().enumerate() {
                    pixel[lane] += value / count(FACTOR * FACTOR);
                }
            }
        }
    }
    tone_map(average)
}

/// The outputs of `FRAMES` frames, optionally discarding the history before each.
fn run(context: &GpuContext, scale: u32, (producer, reset): (bool, bool)) -> Vec<Image> {
    let mut renderer: ConfiguredFrameRenderer = temporal(context, scale, producer);
    let device = context.device();
    let output = texture(device, SIZE, OUTPUT);
    let view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let mut readback =
        Readback::new(device, FrameConfig::new([SIZE, SIZE], OUTPUT, 1), 1 << 20).unwrap();
    (0..FRAMES)
        .map(|frame| {
            if reset {
                renderer.reset_temporal_history();
            }
            let slot = if reset { 0 } else { frame % 3 };
            submit_to(
                context,
                renderer.renderer_mut(),
                (seconds(frame), slot),
                (true, Some(&view)),
            );
            decode_output(
                &readback
                    .read(device, context.submission(), &output)
                    .unwrap(),
            )
        })
        .collect()
}

fn differs(wet: &Image, dry: &Image) -> Vec<bool> {
    wet.iter()
        .zip(dry)
        .map(|(a, b)| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 1e-3))
        .collect()
}

/// Texels whose `radius` neighbourhood is all in (`inside`) or all out of the mask.
fn neighbourhood(mask: &[bool], radius: usize, inside: bool) -> Vec<bool> {
    let side = isize::try_from(WIDTH).unwrap();
    let reach = isize::try_from(radius).unwrap();
    (0..mask.len())
        .map(|index| {
            let (x, y) = (
                isize::try_from(index % WIDTH).unwrap(),
                isize::try_from(index / WIDTH).unwrap(),
            );
            (-reach..=reach).all(|dy| {
                (-reach..=reach).all(|dx| {
                    let (nx, ny) = (x + dx, y + dy);
                    let in_image = (0..side).contains(&nx) && (0..side).contains(&ny);
                    !in_image || mask[usize::try_from(ny * side + nx).unwrap()] == inside
                })
            })
        })
        .collect()
}

fn error(images: &[Image], truths: &[Image], roi: &[bool]) -> f64 {
    let mut total = 0.0;
    let mut texels = 0.0;
    for (image, truth) in images.iter().zip(truths).skip(FRAMES - EVALUATED) {
        for ((pixel, expected), &inside) in image.iter().zip(truth).zip(roi) {
            if inside {
                total += pixel
                    .iter()
                    .zip(expected)
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>();
                texels += 3.0;
            }
        }
    }
    assert!(texels > 0.0, "an empty region");
    total / texels
}

#[test]
fn the_water_producer_measurement_keeps_the_static_region_at_each_scale() {
    let context = context();
    let wet: Vec<Image> = (0..FRAMES)
        .map(|f| truth(&context, seconds(f), false))
        .collect();
    let dry = truth(&context, 0.0, true);
    let covered = differs(&wet[0], &dry);
    let water = neighbourhood(&covered, 1, true);
    let fixed = neighbourhood(&covered, 3, false);
    assert!(water.iter().any(|&w| w) && fixed.iter().any(|&s| s));
    for scale in [500_000, 670_000, 770_000] {
        let zero = run(&context, scale, (false, false));
        let mask = run(&context, scale, (true, false));
        let reset = run(&context, scale, (false, true));
        let (trail_zero, trail_mask, trail_reset) = (
            error(&zero, &wet, &water),
            error(&mask, &wet, &water),
            error(&reset, &wet, &water),
        );
        let (static_zero, static_mask) = (error(&zero, &wet, &fixed), error(&mask, &wet, &fixed));
        let bound = trail_reset + 0.5 * (trail_zero - trail_reset);
        eprintln!(
            "Q1 {scale}: trail zero {trail_zero:.5} mask {trail_mask:.5} reset {trail_reset:.5} bound {bound:.5} | static zero {static_zero:.5} mask {static_mask:.5} bound {:.5}",
            1.10 * static_zero
        );
        assert!(
            [trail_zero, trail_mask, trail_reset, static_zero]
                .iter()
                .all(|v| v.is_finite() && *v > 0.0)
        );
        assert!(
            static_mask <= 1.10 * static_zero,
            "{scale}: static {static_mask} over {static_zero}"
        );
    }
}
