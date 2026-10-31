//! The material mip bias that a temporally reconstructed frame applies to its texture sampling.
//!
//! Rasterising below the output extent selects coarser mip levels than the output grid would.
//! Temporal reconstruction recovers the detail from jittered frames, but only if the textures it
//! accumulates were sampled sharply, so the scene samples with a negative bias. The bias is
//! derived policy: it is computed here from the rounded extents and never authored.
//!
//! The offset below `log2(s)` was chosen by measurement, not taken from the literature: a full
//! level of extra sharpness (`log2(s) - 1`) pushed the shimmer of a static stripe texture above
//! 1.10 times that of native resolution TAA at a render scale of 0.5, half a level did not (see
//! the "Temporal upscaling" section of docs/rendering.md for the table and the decision rule).

/// The mip bias, in levels, for a scene rasterised at `render` and reconstructed to `output`.
///
/// `log2(min(Rw / Ow, Rh / Oh)) - 0.5`: the finer axis of the two sets the footprint, and the
/// half level of extra sharpness is the largest offset whose shimmer stayed within 1.10 times the
/// native TAA shimmer at every tested scale (a render scale of 0.5 gives exactly -1.5, 0.67 gives
/// about -1.078 and 0.77 about -0.877). The ratio comes from the rounded integer extents, so odd
/// outputs get the bias of the extents actually rasterised.
///
/// Equal extents, a render extent above the output and an empty extent give exactly `+0.0`: the
/// bias never turns positive and never becomes infinite.
#[must_use]
pub fn material_mip_bias(render: [u32; 2], output: [u32; 2]) -> f32 {
    if render.contains(&0) || output.contains(&0) {
        return 0.0;
    }
    let ratio = render
        .into_iter()
        .zip(output)
        .map(|(r, o)| f64::from(r) / f64::from(o))
        .fold(f64::INFINITY, f64::min);
    if ratio >= 1.0 {
        return 0.0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a bias in [-1.5, 0) fits f32 with 2^-22 relative error"
    )]
    let bias = (ratio.log2() - 0.5) as f32;
    bias
}
