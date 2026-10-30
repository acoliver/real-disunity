//! History fetch, neighborhood box and clipped blend shared by the f64 TAA references.
//!
//! The native resolve and the temporal upscaler differ in where the current color is sampled,
//! not in how history is filtered, clipped and blended, so both references call these.
use crate::cas_reference::Image;
use crate::taa_reference::{
    catmull_rom_weights, clip_toward_centre, compress, decompress, from_ycocg, to_ycocg,
};

/// Catmull-Rom (bicubic) history color at `uv` over the 4x4 texels around it, edge-clamped.
/// The GPU builds the same filter from nine bilinear taps. `None` when any contributing
/// texel is not finite; negative lobes that undershoot are clamped to zero.
pub(crate) fn catmull_rom_history(history: &Image, uv: [f64; 2]) -> Option<[f64; 3]> {
    let size = history.size.map(f64::from);
    let pos = [uv[0] * size[0] - 0.5, uv[1] * size[1] - 0.5];
    let base = pos.map(f64::floor);
    let frac = [pos[0] - base[0], pos[1] - base[1]];
    let (wx, wy) = (catmull_rom_weights(frac[0]), catmull_rom_weights(frac[1]));
    let mut sum = [0.0; 3];
    for (dy, wy) in (-1_i32..).zip(wy) {
        for (dx, wx) in (-1_i32..).zip(wx) {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "bounded by the image extent"
            )]
            let (x, y) = (base[0] as i32 + dx, base[1] as i32 + dy);
            let t = history.load(x, y);
            for (acc, v) in sum.iter_mut().zip(t) {
                *acc += wx * wy * v;
            }
        }
    }
    sum.iter()
        .all(|v| v.is_finite())
        .then(|| sum.map(|v| v.max(0.0)))
}

/// Centre and half-extent of the min/max box of nine samples in `YCoCg` of luminance-compressed
/// color.
pub(crate) fn neighbourhood_box(samples: &[[f64; 4]; 9]) -> ([f64; 3], [f64; 3]) {
    let samples = samples.map(|s| to_ycocg(compress([s[0], s[1], s[2]])));
    let lo: [f64; 3] =
        std::array::from_fn(|i| samples.iter().map(|s| s[i]).fold(f64::INFINITY, f64::min));
    let hi: [f64; 3] = std::array::from_fn(|i| {
        samples
            .iter()
            .map(|s| s[i])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    (
        std::array::from_fn(|i| 0.5 * (lo[i] + hi[i])),
        std::array::from_fn(|i| 0.5 * (hi[i] - lo[i])),
    )
}

/// Clips `history` to the neighborhood box and blends it into `current` with `weight`.
pub(crate) fn blend_clipped(
    current: [f64; 3],
    history: [f64; 3],
    box_: ([f64; 3], [f64; 3]),
    (clip_scale, weight): (f64, f64),
) -> [f64; 3] {
    let clipped = clip_toward_centre(to_ycocg(compress(history)), box_.0, box_.1, clip_scale);
    let history = decompress(from_ycocg(clipped));
    std::array::from_fn(|i| current[i] * (1.0 - weight) + history[i] * weight)
}
