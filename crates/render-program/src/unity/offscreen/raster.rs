//! The raster state a record sets for its draws, resolved against the attachments they draw into.
//!
//! The scissor of a record counts texels from row 0 and column 0 of the attachments, which is the
//! corner a script calls the bottom left: the engine draws with the position output unflipped, so
//! the lowest part of a scene is in row 0 and the pass's rectangle, which also counts from row 0,
//! needs no flip. A scissor rectangle is rounded to whole texels and clipped to the attachments; a
//! viewport is used as given, and one that is not inside the attachments draws nothing. The
//! depth bias is the script's pair of constant and slope terms as the pipeline takes them, negated
//! because the buffer is reversed.

use super::frame::DrawRaster;

/// The raster state of the draws of one pass, in the terms of the pipeline and the pass.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PassRaster {
    /// The rectangle the pass clips its draws to: x, y, width and height in texels from row 0 and
    /// column 0 of the attachments. `None` leaves the whole attachments.
    pub(super) scissor: Option<[u32; 4]>,
    /// The rectangle clip space maps to, in texels from row 0 and column 0. `None` maps it to the
    /// whole attachments.
    pub(super) viewport: Option<[f32; 4]>,
    /// The depth bias of the pipelines of the triangle draws.
    pub(super) bias: wgpu::DepthBiasState,
    /// Whether the draws blend their alpha as coverage (see `DrawRaster::coverage_alpha`).
    pub(super) coverage_alpha: bool,
}

/// The state of the draws of `raster` over attachments of `size` texels, or `None` when its
/// scissor leaves no texel or its viewport is not inside the attachments, and the draws draw
/// nothing.
pub(super) fn resolve(raster: &DrawRaster, size: [u32; 2]) -> Option<PassRaster> {
    let scissor = match raster.scissor {
        Some(rect) => Some(clipped(rect, size)?),
        None => None,
    };
    let viewport = match raster.viewport {
        Some(rect) if viewport_inside(rect, size) => Some(rect),
        Some(_) => return None,
        None => None,
    };
    let [constant, slope] = raster.depth_bias;
    Some(PassRaster {
        scissor,
        viewport,
        bias: wgpu::DepthBiasState {
            constant: round_to_i32(-f64::from(constant)),
            slope_scale: -slope,
            clamp: 0.0,
        },
        coverage_alpha: raster.coverage_alpha,
    })
}

/// Whether `rect` (x, y, width and height) has an area and lies inside attachments of `size`.
fn viewport_inside(rect: [f32; 4], size: [u32; 2]) -> bool {
    let [x, y, width, height] = rect.map(f64::from);
    let [target_width, target_height] = size.map(f64::from);
    x >= 0.0
        && y >= 0.0
        && width > 0.0
        && height > 0.0
        && x + width <= target_width
        && y + height <= target_height
}

/// `value` rounded half away from zero, held to the range of an `i32`. Found by bisection over the
/// integers so that no narrowing cast is needed.
fn round_to_i32(value: f64) -> i32 {
    let rounded = if value.is_nan() { 0.0 } else { value.round() };
    let (mut low, mut high) = (i64::from(i32::MIN), i64::from(i32::MAX));
    while low < high {
        let middle = (low + high).div_euclid(2);
        let below = i32::try_from(middle).is_ok_and(|candidate| f64::from(candidate) < rounded);
        if below {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    i32::try_from(low).unwrap_or(0)
}

/// The rectangle `rect` (x, y, width and height) as whole texels of attachments of `size`, or `None`
/// when no texel is left.
fn clipped(rect: [f32; 4], size: [u32; 2]) -> Option<[u32; 4]> {
    let [x, y, width, height] = rect.map(f64::from);
    let edge = |from: f64, length: f64, limit: u32| {
        let held = |value: f64| {
            u32::try_from(round_to_i32(value.clamp(0.0, f64::from(limit)))).unwrap_or(0)
        };
        (held(from), held(from + length))
    };
    let (left, right) = edge(x, width, size[0]);
    let (bottom, top) = edge(y, height, size[1]);
    if right <= left || top <= bottom {
        return None;
    }
    Some([left, bottom, right - left, top - bottom])
}
