//! Scalar f64 reference of one temporal-upscale output pixel.
//!
//! The current frame is an internal image of extent `R`, rendered with the projection shifted by
//! `jitter` internal pixels. The history and the output are on the output grid of extent `O`.
//! Output pixel `p` is at `u = (p + 0.5) / O`; the current color is reconstructed at
//! `q = u * R + jitter - 0.5` in internal texel-index coordinates, because the sample stored at
//! integer `s` lies at `(s + 0.5 - jitter) / R`. See `taa_reference` for the shared conventions.
use crate::TaaSettings;
use crate::cas_reference::Image;
use crate::taa_history_reference::{blend_clipped, catmull_rom_history, neighbourhood_box};
use crate::taa_reference::{MOTION_K, sanitize};

/// One frame of temporal reconstruction.
pub(crate) struct Temporal<'a> {
    /// Jittered current frame at the internal extent.
    pub color: &'a Image,
    /// Accumulated history at the output extent.
    pub history: &'a Image,
    pub output: [i32; 2],
    /// Current projection jitter in internal pixels, +x right and +y down.
    pub jitter: [f64; 2],
    pub history_valid: bool,
    pub settings: TaaSettings,
    /// Backward total motion in uv units: the history position is `u + motion_uv`.
    pub motion_uv: [f64; 2],
}

/// Lanczos kernel of radius two, `sinc(x) * sinc(x / 2)`, zero at and beyond two texels.
fn lanczos2(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    if x.abs() >= 2.0 {
        return 0.0;
    }
    let a = std::f64::consts::PI * x;
    2.0 * (a.sin() * (0.5 * a).sin()) / (a * a)
}

impl Temporal<'_> {
    /// Position of the current-color sample for output pixel `at`, in internal texel-index
    /// coordinates: `(p + 0.5) / O * R + jitter - 0.5`.
    pub fn sample_position(&self, at: [i32; 2]) -> [f64; 2] {
        std::array::from_fn(|i| {
            let u = (f64::from(at[i]) + 0.5) / f64::from(self.output[i]);
            u.mul_add(f64::from(self.color.size[i]), self.jitter[i]) - 0.5
        })
    }

    fn sample(&self, x: i32, y: i32) -> [f64; 4] {
        self.color.load(x, y).map(sanitize)
    }

    /// The internal texel nearest the sample position, centre of the clipping neighbourhood.
    fn nearest(&self, at: [i32; 2]) -> [i32; 2] {
        self.sample_position(at).map(|v| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "bounded by the image extent"
            )]
            let texel = (v + 0.5).floor() as i32;
            texel
        })
    }

    /// The current color at the output pixel: normalised Lanczos2 over the 4x4 internal texels
    /// around the sample position, edge-clamped and sanitised.
    pub fn reconstruct(&self, at: [i32; 2]) -> [f64; 4] {
        let q = self.sample_position(at);
        let base = q.map(f64::floor);
        let mut sum = [0.0; 4];
        let mut total = 0.0;
        for dy in -1..=2 {
            for dx in -1..=2 {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "bounded by the image extent"
                )]
                let (x, y) = (base[0] as i32 + dx, base[1] as i32 + dy);
                let w = lanczos2(q[0] - f64::from(x)) * lanczos2(q[1] - f64::from(y));
                total += w;
                for (acc, v) in sum.iter_mut().zip(self.sample(x, y)) {
                    *acc += w * v;
                }
            }
        }
        sum.map(|v| v / total)
    }

    fn neighbourhood(&self, at: [i32; 2]) -> ([f64; 3], [f64; 3]) {
        let centre = self.nearest(at);
        let mut samples = [[0.0; 4]; 9];
        for (slot, (dy, dx)) in samples
            .iter_mut()
            .zip((-1..=1).flat_map(|y| (-1..=1).map(move |x| (y, x))))
        {
            *slot = self.sample(centre[0] + dx, centre[1] + dy);
        }
        neighbourhood_box(&samples)
    }

    /// One output pixel. History is looked up at `u + motion_uv` and ignored when that lies
    /// outside the image; the weight falls with the output-pixel displacement as in native TAA.
    pub fn resolve_pixel(&self, at: [i32; 2]) -> [f64; 4] {
        let current = self.reconstruct(at);
        let uv: [f64; 2] = std::array::from_fn(|i| {
            (f64::from(at[i]) + 0.5) / f64::from(self.output[i]) + self.motion_uv[i]
        });
        let inside = uv.iter().all(|v| (0.0..=1.0).contains(v));
        let rgb = [current[0], current[1], current[2]];
        let history = (self.history_valid && inside)
            .then(|| catmull_rom_history(self.history, uv))
            .flatten();
        let out = history.map_or(rgb, |history| {
            let pixels = (self.motion_uv[0] * f64::from(self.output[0]))
                .hypot(self.motion_uv[1] * f64::from(self.output[1]));
            let weight = f64::from(self.settings.history_weight)
                * MOTION_K.mul_add(-pixels, 1.0).clamp(0.0, 1.0);
            let scale = f64::from(self.settings.clip_scale);
            blend_clipped(rgb, history, self.neighbourhood(at), (scale, weight))
        });
        [
            sanitize(out[0]),
            sanitize(out[1]),
            sanitize(out[2]),
            sanitize(current[3]),
        ]
    }
}
