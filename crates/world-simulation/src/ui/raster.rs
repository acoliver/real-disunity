//! Shared top-row-first coverage and bounded 4x distance encoding for both font owners.
use super::UiError;
use ab_glyph::OutlinedGlyph;
use glam::{DVec2, UVec2, Vec2};

pub(super) const SDF_SCALE: u32 = 4;
pub(super) const MAX_SDF_SIDE: u32 = 256;

pub(super) fn check_distance(size: UVec2, pixels: usize) -> Result<(), UiError> {
    let area = u64::from(size.x) * u64::from(size.y);
    // 16 oversampled cells/pixel: coverage, mask and three live distance/grid arrays.
    let scratch = area.checked_mul(465).ok_or(UiError::DistanceFieldLimit)?;
    let cap = u64::try_from(pixels).unwrap_or(u64::MAX).min(256 * 256) * 465;
    if size.min_element() == 0 || size.max_element() > MAX_SDF_SIDE || scratch > cap {
        return Err(UiError::DistanceFieldLimit);
    }
    Ok(())
}

pub(super) fn coverage(outline: &OutlinedGlyph, size: UVec2, origin: Vec2) -> Vec<f32> {
    let mut bitmap =
        vec![0.; usize::try_from(u64::from(size.x) * u64::from(size.y)).expect("bounded raster")];
    let bounds = outline.px_bounds();
    outline.draw(|x, y, c| {
        let column = f64::from(bounds.min.x - origin.x) + f64::from(x);
        let row = f64::from(bounds.min.y - origin.y) + f64::from(y);
        if column >= 0. && row >= 0. && column < f64::from(size.x) && row < f64::from(size.y) {
            let p = DVec2::new(column, row).as_uvec2();
            bitmap[usize::try_from(u64::from(p.y) * u64::from(size.x) + u64::from(p.x))
                .expect("bounded raster")] = c;
        }
    });
    bitmap
}

pub(super) fn to_byte(value: f64) -> u8 {
    u8::try_from(DVec2::splat(value.clamp(0., 255.).round()).as_uvec2().x).expect("byte range")
}

pub(super) fn distance_bytes(coverage: &[f32], size: UVec2, spread: u32) -> Vec<u8> {
    let scale = usize::try_from(SDF_SCALE).expect("small scale");
    let width = usize::try_from(size.x).expect("small raster") * scale;
    let height = usize::try_from(size.y).expect("small raster") * scale;
    let inside: Vec<bool> = coverage.iter().map(|&c| c >= 0.5).collect();
    let distance = super::sdf::signed_distance(&inside, width, height);
    let mut bytes = Vec::with_capacity(width * height / (scale * scale));
    for row in 0..height / scale {
        for column in 0..width / scale {
            let cell = (row * scale + scale / 2) * width + column * scale + scale / 2;
            let pixels = distance[cell] / f64::from(SDF_SCALE);
            bytes.push(to_byte(
                (0.5 + pixels / (2. * f64::from(spread.max(1)))) * 255.,
            ));
        }
    }
    bytes
}
