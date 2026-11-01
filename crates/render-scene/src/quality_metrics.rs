//! Image metrics for the temporal upscaler qualification matrix. The same module and the same
//! pinned fixture live in `worldrunner-render-post` and `worldrunner-render-scene` (one copy per
//! crate, no shared test crate); the fixture test pins identical results in both.
//!
//! Images are `[r, g, b, a]` linear HDR pixels in row-major order. Alpha is ignored. An optional
//! mask selects the pixels of a region of interest.

pub type Pixel = [f32; 4];

/// Absolute differences of the mapped RGB channels over the selected pixels.
fn differences(a: &[Pixel], b: &[Pixel], mask: Option<&[bool]>, map: fn(f32) -> f64) -> Vec<f64> {
    assert_eq!(a.len(), b.len(), "images differ in size");
    assert!(mask.is_none_or(|m| m.len() == a.len()), "mask size");
    let mut out = Vec::new();
    for (i, (p, q)) in a.iter().zip(b).enumerate() {
        if mask.is_none_or(|m| m[i]) {
            out.extend((0..3).map(|c| (map(p[c]) - map(q[c])).abs()));
        }
    }
    out
}

fn mean(values: &[f64]) -> f64 {
    assert!(!values.is_empty(), "metric over an empty region");
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts are far below 2^52"
    )]
    let count = values.len() as f64;
    values.iter().sum::<f64>() / count
}

fn linear(x: f32) -> f64 {
    f64::from(x)
}

/// A fixed display-linear mapping shared by every mode: `x / (1 + x)` on non-negative values.
fn display_linear(x: f32) -> f64 {
    let x = f64::from(x).max(0.0);
    x / (1.0 + x)
}

/// The 8-bit sRGB code of a linear value after clamping to 0..=1, divided by 255.
fn srgb_byte(x: f32) -> f64 {
    let x = f64::from(x).clamp(0.0, 1.0);
    let encoded = if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() / 255.0
}

pub fn linear_mae(a: &[Pixel], b: &[Pixel], mask: Option<&[bool]>) -> f64 {
    mean(&differences(a, b, mask, linear))
}

pub fn linear_rmse(a: &[Pixel], b: &[Pixel], mask: Option<&[bool]>) -> f64 {
    let squares: Vec<f64> = differences(a, b, mask, linear)
        .iter()
        .map(|d| d * d)
        .collect();
    mean(&squares).sqrt()
}

pub fn display_linear_mae(a: &[Pixel], b: &[Pixel], mask: Option<&[bool]>) -> f64 {
    mean(&differences(a, b, mask, display_linear))
}

/// Mean absolute error of the 8-bit sRGB codes, in units of one code (1/255 is 1/255).
pub fn srgb_byte_mae(a: &[Pixel], b: &[Pixel], mask: Option<&[bool]>) -> f64 {
    mean(&differences(a, b, mask, srgb_byte))
}

/// The peak of a per-frame error series.
pub fn peak_error(series: &[f64]) -> f64 {
    series.iter().copied().fold(0.0, f64::max)
}

/// The sum of a per-frame error series, the integrated trail error.
pub fn integrated_error(series: &[f64]) -> f64 {
    series.iter().sum()
}

/// The linear MAE of the first `count` frames against the matching reference frames.
pub fn convergence(
    frames: &[Vec<Pixel>],
    truth: &[Vec<Pixel>],
    count: usize,
    mask: Option<&[bool]>,
) -> Vec<f64> {
    frames
        .iter()
        .zip(truth)
        .take(count)
        .map(|(frame, reference)| linear_mae(frame, reference, mask))
        .collect()
}

/// Reference-relative temporal instability: the mean over consecutive frame pairs of the mean
/// absolute difference between the frame-to-frame change and the reference's own change.
pub fn temporal_instability(
    frames: &[Vec<Pixel>],
    truth: &[Vec<Pixel>],
    mask: Option<&[bool]>,
) -> f64 {
    assert_eq!(frames.len(), truth.len(), "sequences differ in length");
    let pairs: Vec<f64> = frames
        .windows(2)
        .zip(truth.windows(2))
        .map(|(f, t)| mean(&change_difference(&f[0], &f[1], &t[0], &t[1], mask)))
        .collect();
    mean(&pairs)
}

fn change_difference(
    f0: &[Pixel],
    f1: &[Pixel],
    t0: &[Pixel],
    t1: &[Pixel],
    mask: Option<&[bool]>,
) -> Vec<f64> {
    let mut out = Vec::new();
    for i in (0..f0.len()).filter(|&i| mask.is_none_or(|m| m[i])) {
        out.extend((0..3).map(|c| {
            let seen = f64::from(f1[i][c]) - f64::from(f0[i][c]);
            let wanted = f64::from(t1[i][c]) - f64::from(t0[i][c]);
            (seen - wanted).abs()
        }));
    }
    out
}

/// The position and contrast of one edge crossing a row of luma samples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeProfile {
    /// Sample index (pixel-centre units) where the row first crosses the mid level, interpolated.
    pub position: f64,
    /// The difference between the highest and the lowest sample.
    pub contrast: f64,
}

/// Rec. 709 luma of the pixels `x0..x1` of row `y` in an image `width` pixels wide.
pub fn luma_row(image: &[Pixel], width: usize, y: usize, x0: usize, x1: usize) -> Vec<f32> {
    image[y * width + x0..y * width + x1]
        .iter()
        .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
        .collect()
}

/// Locates the first crossing of the mid level between the row's extremes. A flat row has no edge.
pub fn edge_profile(row: &[f32]) -> Option<EdgeProfile> {
    let low = row.iter().copied().fold(f32::INFINITY, f32::min);
    let high = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if high.partial_cmp(&low) != Some(std::cmp::Ordering::Greater) {
        return None;
    }
    let mid = f32::midpoint(low, high);
    row.windows(2).enumerate().find_map(|(i, w)| {
        let crosses = (w[0] < mid && w[1] >= mid) || (w[0] > mid && w[1] <= mid);
        #[expect(clippy::cast_precision_loss, reason = "row indices are far below 2^52")]
        let start = i as f64;
        crosses.then(|| EdgeProfile {
            position: start + f64::from(mid - w[0]) / f64::from(w[1] - w[0]),
            contrast: f64::from(high - low),
        })
    })
}

/// Reference-relative temporal instability after compensating a known whole-pixel motion: the
/// previous frame is read `(dx, dy)` pixels back before it is differenced against the current
/// frame, and the reference is compensated the same way. Pixels whose source falls outside the
/// image are skipped; the mask selects current-frame pixels.
pub fn motion_compensated_instability(
    frames: &[Vec<Pixel>],
    truth: &[Vec<Pixel>],
    width: usize,
    shift: (isize, isize),
    mask: Option<&[bool]>,
) -> f64 {
    assert_eq!(frames.len(), truth.len(), "sequences differ in length");
    let mut errors = Vec::new();
    for (f, t) in frames.windows(2).zip(truth.windows(2)) {
        let height = f[0].len() / width;
        for (i, (cur, reference)) in f[1].iter().zip(&t[1]).enumerate() {
            let (x, y) = (i % width, i / width);
            let sx = x.checked_add_signed(-shift.0).filter(|&sx| sx < width);
            let sy = y.checked_add_signed(-shift.1).filter(|&sy| sy < height);
            let (Some(sx), Some(sy)) = (sx, sy) else {
                continue;
            };
            if mask.is_some_and(|m| !m[i]) {
                continue;
            }
            let src = sy * width + sx;
            errors.extend((0..3).map(|c| {
                let seen = f64::from(cur[c]) - f64::from(f[0][src][c]);
                let wanted = f64::from(reference[c]) - f64::from(t[0][src][c]);
                (seen - wanted).abs()
            }));
        }
    }
    mean(&errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: [Pixel; 2] = [[0.5, 0.5, 0.5, 1.0], [2.0, 0.0, 0.0, 1.0]];
    const B: [Pixel; 2] = [[0.25, 0.5, 0.75, 0.0], [1.0, 0.0, 0.0, 0.0]];
    const FIRST_ONLY: [bool; 2] = [true, false];

    /// Pixels are `f32`, so sums of a few values carry about 1e-7 of representation error.
    fn close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-6, "{got} vs {want}");
    }

    /// The numbers below are pinned in `worldrunner-render-post` and `worldrunner-render-scene`.
    #[test]
    fn the_pinned_fixture_gives_the_same_numbers_in_every_crate() {
        close(linear_mae(&A, &B, None), 0.25);
        close(linear_rmse(&A, &B, None), 0.1875_f64.sqrt());
        let tone = |x: f64| x / (1.0 + x);
        let sum = (tone(0.5) - tone(0.25)).abs()
            + (tone(0.5) - tone(0.75)).abs()
            + (tone(2.0) - tone(1.0)).abs();
        close(display_linear_mae(&A, &B, None), sum / 6.0);
        // Codes: 0.5 -> 188, 0.25 -> 137, 0.75 -> 225, 2 and 1 -> 255.
        close(srgb_byte_mae(&A, &B, None), (51.0 + 37.0) / 255.0 / 6.0);
        close(linear_mae(&A, &B, Some(&FIRST_ONLY)), 0.5 / 3.0);
    }

    #[test]
    fn series_metrics_peak_and_integrate() {
        close(peak_error(&[0.25, 0.75, 0.5]), 0.75);
        close(integrated_error(&[0.25, 0.75, 0.5]), 1.5);
        close(peak_error(&[]), 0.0);
    }

    #[test]
    fn a_sequence_that_follows_the_reference_has_no_instability_and_flicker_shows() {
        let truth = vec![vec![[0.5; 4]; 2]; 3];
        let steady = vec![vec![[0.4; 4]; 2]; 3];
        close(temporal_instability(&steady, &truth, None), 0.0);
        let mut flicker = steady.clone();
        flicker[1] = vec![[0.6, 0.4, 0.4, 1.0]; 2];
        // Each of the two pairs changes one channel by 0.2: 0.2 over three channels on average.
        close(temporal_instability(&flicker, &truth, None), 0.2 / 3.0);
        close(
            convergence(&flicker, &truth, 2, None)[1],
            (0.1 + 0.1 + 0.1) / 3.0,
        );
    }

    #[test]
    fn an_edge_reports_its_interpolated_position_and_contrast() {
        let edge = edge_profile(&[0.0, 0.0, 0.25, 1.0, 1.0]).expect("edge");
        close(edge.position, 2.0 + 0.25 / 0.75);
        close(edge.contrast, 1.0);
        let falling = edge_profile(&[1.0, 0.5, 0.0]).expect("edge");
        close(falling.position, 1.0);
        assert!(edge_profile(&[0.5; 4]).is_none());
        let image = [
            [1.0, 1.0, 1.0, 0.0],
            [0.0; 4],
            [0.0, 1.0, 0.0, 0.0],
            [0.0; 4],
        ];
        let row = luma_row(&image, 2, 1, 0, 2);
        close(f64::from(row[0]), 0.7152_f64);
        close(f64::from(row[1]), 0.0);
    }

    #[test]
    fn a_pan_followed_exactly_has_no_compensated_instability_and_wrong_motion_shows() {
        // A one pixel wide image row of four pixels, content moving right by one pixel per frame.
        let frame = |k: usize| -> Vec<Pixel> {
            (0..4)
                .map(|x| if x == k { [1.0; 4] } else { [0.0; 4] })
                .collect()
        };
        let sequence = vec![frame(0), frame(1), frame(2)];
        close(
            motion_compensated_instability(&sequence, &sequence, 4, (1, 0), None),
            0.0,
        );
        // A frame that lags by one pixel against the reference: wrong at two pixels per pair.
        let lagging = vec![frame(0), frame(0), frame(1)];
        let error = motion_compensated_instability(&lagging, &sequence, 4, (1, 0), None);
        assert!(error > 0.0, "{error}");
        // Without compensation the correct sequence still matches itself.
        close(
            motion_compensated_instability(&sequence, &sequence, 4, (0, 0), None),
            0.0,
        );
    }
}
