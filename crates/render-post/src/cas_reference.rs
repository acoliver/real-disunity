//! Scalar f64 reference of the CAS sharpen-only kernel (`CasFilter` with `noScaling`, default
//! quality: green filter weights, no `CAS_BETTER_DIAGONALS`), with exact reciprocals and square
//! roots. Out-of-range taps clamp to the nearest edge texel.

/// An f32 image, exactly what the GPU test uploads.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Image {
    pub size: [i32; 2],
    pub pixels: Vec<[f32; 4]>,
}
impl Image {
    pub fn from_fn(size: [i32; 2], mut color: impl FnMut(i32, i32) -> [f32; 4]) -> Self {
        let pixels = (0..size[1])
            .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
            .map(|(x, y)| color(x, y))
            .collect();
        Self { size, pixels }
    }

    /// The image as f64, for comparison with `sharpen`.
    pub fn widened(&self) -> Vec<[f64; 4]> {
        self.pixels.iter().map(|p| p.map(f64::from)).collect()
    }

    pub fn load(&self, x: i32, y: i32) -> [f64; 4] {
        let x = x.clamp(0, self.size[0] - 1);
        let y = y.clamp(0, self.size[1] - 1);
        self.pixels[usize::try_from(y * self.size[0] + x).unwrap()].map(f64::from)
    }

    /// The reference kernel. `peak` is `CasSettings::peak`; `saturate` is the final `ASatF1`.
    pub fn sharpen(&self, peak: f64, saturate: bool) -> Vec<[f64; 4]> {
        (0..self.size[1])
            .flat_map(|row| (0..self.size[0]).map(move |col| [col, row]))
            .map(|at| self.sharpen_pixel(at, peak, saturate))
            .collect()
    }

    fn sharpen_pixel(&self, [col, row]: [i32; 2], peak: f64, saturate: bool) -> [f64; 4] {
        let [north, west, centre, east, south] = [(0, -1), (-1, 0), (0, 0), (1, 0), (0, 1)]
            .map(|(dx, dy)| self.load(col + dx, row + dy));
        let green = [north[1], west[1], centre[1], east[1], south[1]];
        let min = green.into_iter().fold(f64::INFINITY, f64::min);
        let max = green.into_iter().fold(f64::NEG_INFINITY, f64::max);
        let amp = (min.min(1.0 - max) / max.max(1e-20)).clamp(0.0, 1.0).sqrt();
        let weight = amp * peak;
        let channel = |c: usize| {
            let ring = north[c] + west[c] + east[c] + south[c];
            let value = (ring * weight + centre[c]) / (1.0 + 4.0 * weight);
            if saturate {
                value.clamp(0.0, 1.0)
            } else {
                value
            }
        };
        [channel(0), channel(1), channel(2), centre[3]]
    }
}

#[test]
fn constant_images_are_fixed_points_and_peak_runs_from_one_eighth_to_one_fifth() {
    use crate::CasSettings;
    assert!((f64::from(CasSettings::new(0.0).peak()) + 0.125).abs() < 1e-7);
    assert!((f64::from(CasSettings::new(1.0).peak()) + 0.2).abs() < 1e-7);
    for value in [0.0, 0.2, 0.5, 1.0] {
        let flat = Image::from_fn([5, 4], |_, _| [value, value, value, 0.5]);
        for sharpness in [0.0, 0.5, 1.0] {
            let peak = f64::from(CasSettings::new(sharpness).peak());
            for (a, b) in flat.widened().iter().zip(&flat.sharpen(peak, false)) {
                assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-12));
            }
        }
    }
}

/// An edge that reaches black or white has no headroom, so the reference leaves it alone.
#[test]
fn contrast_that_touches_the_signal_limits_is_not_sharpened() {
    let peak = -0.2;
    for (low, high) in [(0.0, 1.0), (0.0, 0.5), (0.5, 1.0)] {
        let edge = Image::from_fn([6, 3], |x, _| {
            let v = if x < 3 { low } else { high };
            [v, v, v, 1.0]
        });
        assert_eq!(edge.sharpen(peak, true), edge.widened(), "{low}..{high}");
    }
    let mid = Image::from_fn([6, 3], |x, _| {
        let v = if x < 3 { 0.3 } else { 0.6 };
        [v, v, v, 1.0]
    });
    assert_ne!(mid.sharpen(peak, true), mid.widened());
}
