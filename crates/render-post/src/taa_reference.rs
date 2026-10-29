//! Scalar f64 reference of one TAA resolve output pixel.
//!
//! The resolve follows Karis 2014, "High Quality Temporal Supersampling" (SIGGRAPH Advances in
//! Real-Time Rendering): luminance-compressed color, a neighborhood color box that history is
//! clipped to, and `1 / (1 + luma)` weights in the blend. The box is the mean plus or minus a
//! multiple of the standard deviation of the 3x3 neighborhood, as in Salvi 2016, "An
//! Excursion in Temporal Supersampling" (GDC), and history is clipped toward the box centre
//! along the ray, not per channel.
//!
//! Conventions: pixel `(x, y)` has its centre at `(x + 0.5, y + 0.5)`, y down. The current
//! image holds samples rendered with a projection shifted by `jitter` pixels, so the sample
//! stored at pixel `q` lies at `q + 0.5 + jitter`. The history image is on the unjittered
//! grid. Depth is single-sample device depth in 0..1 (not reversed, 1 is far) rendered with
//! the same jitter. NDC y is up, so `uv.y = 0.5 - 0.5 * ndc.y`.
//!
//! History is fetched with a bilinear filter at the reprojected position, clamped at the
//! edges, which is what a linear clamp sampler does on the GPU. A higher-order filter is a
//! later refinement.
use crate::TaaSettings;
use crate::cas_reference::Image;

/// A 4x4 matrix as columns: `m[column][row]`, applied to column vectors.
pub(crate) type Mat4 = [[f64; 4]; 4];

/// Standard deviation, in pixels, of the Gaussian that reconstructs the current color at the
/// pixel centre from the 3x3 jittered samples around it.
pub(crate) const KERNEL_SIGMA: f64 = 0.47;

/// Smallest half-extent of the clip box per `YCoCg` axis. It keeps the clip ray finite when the
/// neighborhood is flat; history that is off by more than this is pulled to the box centre.
pub(crate) const MIN_BOX_EXTENT: f64 = 1e-4;

/// Largest luma of a compressed color. Compression maps luma `l` to `l / (1 + l)`, below 1.
pub(crate) const MAX_COMPRESSED_LUMA: f64 = 1.0 - 1e-5;

/// Single-sample device depth, row-major.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Depth {
    pub size: [i32; 2],
    pub values: Vec<f32>,
}
impl Depth {
    pub fn from_fn(size: [i32; 2], mut depth: impl FnMut(i32, i32) -> f32) -> Self {
        let values = (0..size[1])
            .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
            .map(|(x, y)| depth(x, y))
            .collect();
        Self { size, values }
    }

    pub fn load(&self, x: i32, y: i32) -> f64 {
        let x = x.clamp(0, self.size[0] - 1);
        let y = y.clamp(0, self.size[1] - 1);
        f64::from(self.values[usize::try_from(y * self.size[0] + x).unwrap()])
    }
}

/// Column-major identity.
pub(crate) const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// Everything one resolve reads.
pub(crate) struct Frame<'a> {
    pub color: &'a Image,
    pub depth: &'a Depth,
    pub history: &'a Image,
    /// Previous unjittered view-projection times the inverse of the current one.
    pub reprojection: Mat4,
    /// Current projection jitter in pixels, +x right and +y down.
    pub jitter: [f64; 2],
    pub history_valid: bool,
    pub settings: TaaSettings,
}

/// Luma of linear Rec. 709 color.
pub(crate) fn luma(c: [f64; 3]) -> f64 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Karis luminance compression `c / (1 + luma)`, which bounds HDR values below 1.
pub(crate) fn compress(c: [f64; 3]) -> [f64; 3] {
    let k = 1.0 / (1.0 + luma(c));
    c.map(|v| v * k)
}

/// The inverse of [`compress`]; luma is held below [`MAX_COMPRESSED_LUMA`] so a clipped value
/// cannot divide by zero.
pub(crate) fn decompress(c: [f64; 3]) -> [f64; 3] {
    let k = 1.0 / (1.0 - luma(c).clamp(0.0, MAX_COMPRESSED_LUMA));
    c.map(|v| v * k)
}

pub(crate) fn to_ycocg([r, g, b]: [f64; 3]) -> [f64; 3] {
    [
        0.25 * r + 0.5 * g + 0.25 * b,
        0.5 * r - 0.5 * b,
        -0.25 * r + 0.5 * g - 0.25 * b,
    ]
}

pub(crate) fn from_ycocg([y, co, cg]: [f64; 3]) -> [f64; 3] {
    [y + co - cg, y + cg, y - co - cg]
}

/// Clips `history` onto the box `mean +- scale * sigma` along the line to `mean`. A point
/// inside is unchanged; one outside is scaled toward the centre by the largest per-axis
/// ratio of its offset to the half-extent, so hue is kept, unlike a per-channel clamp.
pub(crate) fn clip_toward_centre(
    history: [f64; 3],
    mean: [f64; 3],
    sigma: [f64; 3],
    scale: f64,
) -> [f64; 3] {
    let offset: [f64; 3] = std::array::from_fn(|i| history[i] - mean[i]);
    let ratio = (0..3)
        .map(|i| offset[i].abs() / (scale * sigma[i]).max(MIN_BOX_EXTENT))
        .fold(0.0, f64::max);
    if ratio <= 1.0 {
        history
    } else {
        std::array::from_fn(|i| mean[i] + offset[i] / ratio)
    }
}

/// Non-finite values become 0 and negative values become 0.
fn sanitize(v: f64) -> f64 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// Weight of the jittered sample at neighbour offset `d` for the pixel centre. The sample at
/// `p + d` lies at `p + d + jitter` from the centre of `p`.
fn kernel_weight(d: [i32; 2], jitter: [f64; 2]) -> f64 {
    let x = f64::from(d[0]) + jitter[0];
    let y = f64::from(d[1]) + jitter[1];
    (-(x * x + y * y) / (2.0 * KERNEL_SIGMA * KERNEL_SIGMA)).exp()
}

const OFFSETS: [[i32; 2]; 9] = [
    [-1, -1],
    [0, -1],
    [1, -1],
    [-1, 0],
    [0, 0],
    [1, 0],
    [-1, 1],
    [0, 1],
    [1, 1],
];

impl Frame<'_> {
    /// The current sample at a texel, edge-clamped and sanitised.
    fn sample(&self, x: i32, y: i32) -> [f64; 4] {
        self.color.load(x, y).map(sanitize)
    }

    /// The current color at the pixel centre: the 3x3 jittered samples under the normalised
    /// Gaussian [`KERNEL_SIGMA`]. This undoes the jitter and filters the frame's own aliasing.
    pub fn reconstruct(&self, at: [i32; 2]) -> [f64; 4] {
        let mut sum = [0.0; 4];
        let mut total = 0.0;
        for d in OFFSETS {
            let w = kernel_weight(d, self.jitter);
            let s = self.sample(at[0] + d[0], at[1] + d[1]);
            total += w;
            for (acc, v) in sum.iter_mut().zip(s) {
                *acc += w * v;
            }
        }
        sum.map(|v| v / total)
    }

    /// Mean and standard deviation of the 3x3 jittered samples, equally weighted, in `YCoCg` of
    /// luminance-compressed color.
    pub fn statistics(&self, at: [i32; 2]) -> ([f64; 3], [f64; 3]) {
        let samples = OFFSETS.map(|d| {
            let s = self.sample(at[0] + d[0], at[1] + d[1]);
            to_ycocg(compress([s[0], s[1], s[2]]))
        });
        let n = 9.0;
        let mean: [f64; 3] = std::array::from_fn(|i| samples.iter().map(|s| s[i]).sum::<f64>() / n);
        let sigma = std::array::from_fn(|i| {
            (samples
                .iter()
                .map(|s| (s[i] - mean[i]).powi(2))
                .sum::<f64>()
                / n)
                .sqrt()
        });
        (mean, sigma)
    }

    /// The texel of the 3x3 neighborhood with the smallest depth; the first in row-major order
    /// wins a tie. Edge neighbors clamp to the image.
    fn closest(&self, at: [i32; 2]) -> [i32; 2] {
        let clamp = |p: [i32; 2]| {
            [
                p[0].clamp(0, self.depth.size[0] - 1),
                p[1].clamp(0, self.depth.size[1] - 1),
            ]
        };
        let mut best = clamp(at);
        for d in OFFSETS {
            let q = clamp([at[0] + d[0], at[1] + d[1]]);
            if self.depth.load(q[0], q[1]) < self.depth.load(best[0], best[1]) {
                best = q;
            }
        }
        best
    }

    /// UV in the history image for the pixel, or `None` when the point cannot be reprojected
    /// or lands outside the image. The motion is that of the closest-depth neighbor: its
    /// unjittered NDC (the depth texel was rasterised `jitter` pixels off its centre) and its
    /// depth go through the reprojection matrix, and the uv displacement is applied at the
    /// pixel centre.
    fn history_uv(&self, at: [i32; 2]) -> Option<[f64; 2]> {
        let q = self.closest(at);
        let size = self.depth.size.map(f64::from);
        let uv = |p: [i32; 2]| {
            [
                (f64::from(p[0]) + 0.5) / size[0],
                (f64::from(p[1]) + 0.5) / size[1],
            ]
        };
        let texel = uv(q);
        let ndc = [
            2.0 * texel[0] - 1.0 - 2.0 * self.jitter[0] / size[0],
            1.0 - 2.0 * texel[1] + 2.0 * self.jitter[1] / size[1],
        ];
        let m = &self.reprojection;
        let v = [ndc[0], ndc[1], self.depth.load(q[0], q[1]), 1.0];
        let row = |r: usize| (0..4).map(|c| m[c][r] * v[c]).sum::<f64>();
        let w = row(3);
        if !(w.is_finite() && w > 0.0) {
            return None;
        }
        let previous = [0.5 + 0.5 * row(0) / w, 0.5 - 0.5 * row(1) / w];
        let unjittered = [0.5 + 0.5 * ndc[0], 0.5 - 0.5 * ndc[1]];
        let centre = uv(at);
        let out = [
            centre[0] + previous[0] - unjittered[0],
            centre[1] + previous[1] - unjittered[1],
        ];
        out.iter().all(|v| (0.0..=1.0).contains(v)).then_some(out)
    }

    /// Bilinear history color at `uv`, `None` when any contributing texel is not finite.
    fn fetch_history(&self, uv: [f64; 2]) -> Option<[f64; 3]> {
        let size = self.history.size.map(f64::from);
        let pos = [uv[0] * size[0] - 0.5, uv[1] * size[1] - 0.5];
        let base = pos.map(f64::floor);
        let frac = [pos[0] - base[0], pos[1] - base[1]];
        let mut sum = [0.0; 3];
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let wx = if dx == 0 { 1.0 - frac[0] } else { frac[0] };
            let wy = if dy == 0 { 1.0 - frac[1] } else { frac[1] };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "bounded by the image extent"
            )]
            let (x, y) = (base[0] as i32 + dx, base[1] as i32 + dy);
            let t = self.history.load(x, y);
            for (acc, v) in sum.iter_mut().zip(t) {
                *acc += wx * wy * v;
            }
        }
        sum.iter()
            .all(|v| v.is_finite())
            .then(|| sum.map(|v| v.max(0.0)))
    }

    /// Blends clipped history into the reconstructed color with Karis `1 / (1 + luma)` weights.
    fn blend(&self, current: [f64; 3], history: [f64; 3], box_: ([f64; 3], [f64; 3])) -> [f64; 3] {
        let scale = f64::from(self.settings.clip_scale);
        let clipped = clip_toward_centre(to_ycocg(compress(history)), box_.0, box_.1, scale);
        let history = decompress(from_ycocg(clipped));
        let weight = f64::from(self.settings.history_weight);
        let wc = (1.0 - weight) / (1.0 + luma(current).max(0.0));
        let wh = weight / (1.0 + luma(history).max(0.0));
        std::array::from_fn(|i| (current[i] * wc + history[i] * wh) / (wc + wh))
    }

    /// One output pixel; alpha is the reconstructed alpha. The output is finite and not negative.
    pub fn resolve_pixel(&self, at: [i32; 2]) -> [f64; 4] {
        let current = self.reconstruct(at);
        let rgb = [current[0], current[1], current[2]];
        let history = self
            .history_valid
            .then(|| self.history_uv(at))
            .flatten()
            .and_then(|uv| self.fetch_history(uv));
        let out = history.map_or(rgb, |h| self.blend(rgb, h, self.statistics(at)));
        [
            sanitize(out[0]),
            sanitize(out[1]),
            sanitize(out[2]),
            sanitize(current[3]),
        ]
    }

    /// The whole image, row-major.
    #[expect(dead_code, reason = "used by the GPU comparison tests")]
    pub fn resolve(&self) -> Vec<[f64; 4]> {
        (0..self.color.size[1])
            .flat_map(|y| (0..self.color.size[0]).map(move |x| [x, y]))
            .map(|at| self.resolve_pixel(at))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "the test stores f32 pixels"
    )]
    fn narrow(v: f64) -> f32 {
        v as f32
    }

    fn gray(v: f32) -> [f32; 4] {
        [v, v, v, 1.0]
    }

    fn flat(size: [i32; 2], v: f32) -> Image {
        Image::from_fn(size, |_, _| gray(v))
    }

    fn near(a: &[f64], b: &[f64]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < EPS)
    }

    fn settings(history_weight: f32) -> TaaSettings {
        TaaSettings::new(history_weight)
    }

    struct Scene {
        color: Image,
        depth: Depth,
        history: Image,
        reprojection: Mat4,
        jitter: [f64; 2],
        valid: bool,
        settings: TaaSettings,
    }
    impl Scene {
        fn new(color: Image, history: Image) -> Self {
            let depth = Depth::from_fn(color.size, |_, _| 0.5);
            Self {
                color,
                depth,
                history,
                reprojection: IDENTITY,
                jitter: [0.0; 2],
                valid: true,
                settings: settings(0.9),
            }
        }

        fn resolve(&self, at: [i32; 2]) -> [f64; 4] {
            Frame {
                color: &self.color,
                depth: &self.depth,
                history: &self.history,
                reprojection: self.reprojection,
                jitter: self.jitter,
                history_valid: self.valid,
                settings: self.settings,
            }
            .resolve_pixel(at)
        }
    }

    /// Gaussian weight of a 3x3 tap at integer offset distance squared `r2`, zero jitter.
    fn weight(r2: f64) -> f64 {
        (-r2 / (2.0 * KERNEL_SIGMA * KERNEL_SIGMA)).exp()
    }

    /// Value 1 along the row above the centre pixel and 0 elsewhere.
    fn top_row(size: [i32; 2]) -> Image {
        Image::from_fn(size, |_, y| gray(if y == 1 { 1.0 } else { 0.0 }))
    }

    /// The zero-jitter reconstruction of `top_row` at pixel (2, 2), by hand: the two upper
    /// corners and the upper edge tap carry the 1s.
    fn top_row_centre() -> f64 {
        let (edge, corner) = (weight(1.0), weight(2.0));
        (2.0 * corner + edge) / (1.0 + 4.0 * edge + 4.0 * corner)
    }

    fn ramp(size: [i32; 2], along_y: bool) -> Image {
        Image::from_fn(size, |x, y| {
            let t = f32::from(u8::try_from(if along_y { y } else { x }).unwrap());
            gray(0.2 + 0.1 * t)
        })
    }

    /// History shifted so that a lookup moved by `+shift` pixels along the axis lands on the
    /// current value.
    fn shifted_ramp(size: [i32; 2], along_y: bool, shift: i32) -> Image {
        Image::from_fn(size, |x, y| {
            let t = if along_y { y } else { x } - shift;
            gray(0.2 + 0.1 * f32::from(i16::try_from(t).unwrap()))
        })
    }

    fn translation(ndc: [f64; 2]) -> Mat4 {
        let mut m = IDENTITY;
        m[3] = [ndc[0], ndc[1], 0.0, 1.0];
        m
    }

    #[test]
    fn constant_image_with_valid_identity_history_is_unchanged() {
        let colour = [0.5, 0.25, 0.125, 0.75];
        let image = Image::from_fn([5, 5], |_, _| colour);
        let mut scene = Scene::new(image.clone(), image);
        for jitter in [[0.0, 0.0], [0.3, -0.2], [-0.5, 0.5]] {
            scene.jitter = jitter;
            let out = scene.resolve([2, 2]);
            assert!(near(&out, &colour.map(f64::from)), "{jitter:?}: {out:?}");
        }
    }

    #[test]
    fn invalid_history_returns_the_reconstructed_current() {
        let mut scene = Scene::new(top_row([5, 5]), flat([5, 5], 40.0));
        scene.valid = false;
        let out = scene.resolve([2, 2]);
        let expected = top_row_centre();
        assert!(near(&out[..3], &[expected; 3]), "{out:?} vs {expected}");
        assert!((out[3] - 1.0).abs() < EPS);
    }

    #[test]
    fn jitter_weights_each_sample_by_its_distance_from_the_pixel_centre() {
        let mut scene = Scene::new(top_row([5, 5]), flat([5, 5], 0.0));
        scene.valid = false;
        let at_rest = scene.resolve([2, 2])[0];
        scene.jitter = [0.0, 0.5];
        let moved = scene.resolve([2, 2])[0];
        // Samples sit 0.5 pixel lower, so the row above is 0.5 pixel from the centre, not 1.
        assert!(moved > at_rest, "{moved} {at_rest}");
        let by_hand = {
            let w = |dx: f64, dy: f64| weight(dx * dx + dy * dy);
            let mut top = 0.0;
            let mut all = 0.0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let weight = w(f64::from(dx), f64::from(dy) + 0.5);
                    all += weight;
                    if dy == -1 {
                        top += weight;
                    }
                }
            }
            top / all
        };
        assert!((moved - by_hand).abs() < EPS);
    }

    #[test]
    fn neighbourhood_statistics_are_mean_and_deviation_of_the_compressed_ycocg_samples() {
        // Three samples of 1 (compressed luma 0.5) and six of 0 in the 3x3 block.
        let scene = Scene::new(top_row([5, 5]), flat([5, 5], 0.0));
        let frame = Frame {
            color: &scene.color,
            depth: &scene.depth,
            history: &scene.history,
            reprojection: IDENTITY,
            jitter: [0.0; 2],
            history_valid: true,
            settings: scene.settings,
        };
        let (mean, sigma) = frame.statistics([2, 2]);
        assert!(near(&mean, &[1.0 / 6.0, 0.0, 0.0]), "{mean:?}");
        assert!(
            near(&sigma, &[(1.0_f64 / 18.0).sqrt(), 0.0, 0.0]),
            "{sigma:?}"
        );
    }

    #[test]
    fn clipping_moves_history_toward_the_centre_along_the_ray_not_per_channel() {
        let mean = [0.5, 0.0, 0.0];
        let sigma = [0.1, 0.05, 0.0];
        // Offsets (0.2, 0.2): 2 and 4 box half-extents, so the ray scale is 4 and the point
        // lands on the Co face. A per-channel clamp would give Y = 0.6.
        let clipped = clip_toward_centre([0.7, 0.2, 0.0], mean, sigma, 1.0);
        assert!(near(&clipped, &[0.55, 0.05, 0.0]), "{clipped:?}");
        let clipped = clip_toward_centre([0.9, 0.0, 0.0], mean, sigma, 1.0);
        assert!(near(&clipped, &[0.6, 0.0, 0.0]), "{clipped:?}");
        let clipped = clip_toward_centre([0.9, 0.0, 0.0], mean, sigma, 2.0);
        assert!(near(&clipped, &[0.7, 0.0, 0.0]), "{clipped:?}");
        let inside = clip_toward_centre([0.55, 0.02, 0.0], mean, sigma, 1.0);
        assert!(near(&inside, &[0.55, 0.02, 0.0]), "{inside:?}");
    }

    #[test]
    fn history_outside_the_box_is_clipped_onto_it_before_the_blend() {
        let mut scene = Scene::new(top_row([5, 5]), flat([5, 5], 100.0));
        scene.settings = settings(0.5);
        let out = scene.resolve([2, 2]);
        // Gray: the clipped history has compressed luma mean + sigma, then decompresses.
        let clipped_luma = 1.0 / 6.0 + (1.0_f64 / 18.0).sqrt();
        let history = clipped_luma / (1.0 - clipped_luma);
        let current = top_row_centre();
        let (wc, wh) = (0.5 / (1.0 + current), 0.5 / (1.0 + history));
        let expected = (current * wc + history * wh) / (wc + wh);
        assert!(near(&out[..3], &[expected; 3]), "{out:?} vs {expected}");
        assert!(out[0] < 5.0, "unclipped history would dominate: {out:?}");
    }

    #[test]
    fn identity_reprojection_without_jitter_keeps_history_equal_to_the_reconstruction() {
        let size = [7, 7];
        let ramp = ramp(size, false);
        let mut scene = Scene::new(ramp.clone(), ramp.clone());
        scene.valid = false;
        let history = Image::from_fn(size, |x, y| {
            let g = scene.resolve([x, y]);
            [narrow(g[0]), narrow(g[1]), narrow(g[2]), 1.0]
        });
        scene.history = history;
        scene.valid = true;
        for x in 1..6 {
            let out = scene.resolve([x, 3]);
            let expected = f64::from(scene.history.pixels[3 * 7 + usize::try_from(x).unwrap()][0]);
            assert!(
                (out[0] - expected).abs() < 1e-6,
                "{x}: {out:?} vs {expected}"
            );
        }
    }

    #[test]
    fn a_translation_matrix_moves_the_history_lookup_by_the_expected_pixels() {
        let size = [9, 9];
        for (along_y, shift, ndc) in [
            (false, 2, [4.0 / 9.0, 0.0]),
            (false, -1, [-2.0 / 9.0, 0.0]),
            (true, 2, [0.0, -4.0 / 9.0]),
            (true, -2, [0.0, 4.0 / 9.0]),
        ] {
            let mut scene = Scene::new(ramp(size, along_y), shifted_ramp(size, along_y, shift));
            scene.reprojection = translation(ndc);
            let exact = scene.resolve([4, 4]);
            assert!(
                (exact[0] - 0.6).abs() < 1e-6,
                "axis y {along_y} shift {shift}: {exact:?}"
            );
            scene.reprojection = IDENTITY;
            let unmoved = scene.resolve([4, 4]);
            assert!(
                (unmoved[0] - 0.6).abs() > 1e-3,
                "history is off by {shift} pixels without the translation: {unmoved:?}"
            );
        }
    }

    #[test]
    fn a_lookup_outside_the_image_ignores_history() {
        let size = [5, 5];
        let mut scene = Scene::new(top_row(size), flat(size, 100.0));
        // One pixel to the right: pixel x = 4 looks up past the edge, x = 2 does not.
        scene.reprojection = translation([2.0 / 5.0, 0.0]);
        let outside = scene.resolve([4, 1]);
        scene.valid = false;
        let current = scene.resolve([4, 1]);
        assert!(near(&outside, &current), "{outside:?} vs {current:?}");
        scene.valid = true;
        let inside = scene.resolve([2, 1]);
        scene.valid = false;
        assert!(!near(&inside, &scene.resolve([2, 1])));
    }

    #[test]
    fn a_point_behind_the_previous_camera_ignores_history() {
        let size = [5, 5];
        let mut scene = Scene::new(top_row(size), flat(size, 100.0));
        scene.reprojection[2][3] = 0.0;
        scene.reprojection[3][3] = -1.0;
        let behind = scene.resolve([2, 2]);
        scene.valid = false;
        assert!(near(&behind, &scene.resolve([2, 2])), "{behind:?}");
    }

    #[test]
    fn non_finite_history_never_reaches_the_output() {
        let size = [5, 5];
        for poison in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut history = flat(size, 0.5);
            history.pixels[2 * 5 + 2] = [poison, 0.5, 0.5, 1.0];
            let mut scene = Scene::new(top_row(size), history);
            let out = scene.resolve([2, 2]);
            assert!(out.iter().all(|v| v.is_finite() && *v >= 0.0), "{out:?}");
            scene.valid = false;
            assert!(near(&out, &scene.resolve([2, 2])), "{poison}: {out:?}");
        }
    }

    #[test]
    fn non_finite_and_negative_current_samples_are_sanitised() {
        let size = [5, 5];
        let mut color = top_row(size);
        color.pixels[2 * 5 + 2] = [f32::NAN, -3.0, f32::INFINITY, f32::NAN];
        let scene = Scene::new(color, flat(size, 0.25));
        let out = scene.resolve([2, 2]);
        assert!(out.iter().all(|v| v.is_finite() && *v >= 0.0), "{out:?}");
    }

    #[test]
    fn the_closest_neighbour_supplies_the_motion() {
        let size = [9, 9];
        let mut scene = Scene::new(ramp(size, false), shifted_ramp(size, false, 2));
        // Parallax: the previous NDC x moves by 16/9 times depth, i.e. 8/9 uv per unit depth.
        // Depth 0.25 gives a 2 pixel lookup shift, the pixel's own depth 0.5 gives 4.
        scene.reprojection[2] = [16.0 / 9.0, 0.0, 1.0, 0.0];
        scene.depth = Depth::from_fn(size, |x, y| if (x, y) == (3, 3) { 0.25 } else { 0.5 });
        let out = scene.resolve([4, 4]);
        assert!((out[0] - 0.6).abs() < 1e-6, "{out:?}");
        scene.depth = Depth::from_fn(size, |_, _| 0.5);
        assert!((scene.resolve([4, 4])[0] - 0.6).abs() > 1e-3);
    }
}
