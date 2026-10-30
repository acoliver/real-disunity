//! Scalar quality-path specification using separable second differences and pixel coordinates.
use super::filter_reference::Image;
use crate::{FxaaKernel, FxaaQuality};

fn brightness(color: [f32; 4], quality: FxaaQuality) -> f32 {
    super::filter_reference::luma(color, quality.luminance)
}

pub(super) fn scores(grid: [[f32; 3]; 3]) -> [f32; 2] {
    let weights = [1.0, 2.0, 1.0];
    let horizontal = (0..3)
        .map(|x| weights[x] * ((grid[0][x] + grid[2][x]) - 2.0 * grid[1][x]).abs())
        .sum();
    let vertical = (0..3)
        .map(|y| weights[y] * ((grid[y][0] + grid[y][2]) - 2.0 * grid[y][1]).abs())
        .sum();
    [horizontal, vertical]
}

pub(super) fn endpoint(
    image: &Image,
    origin: [f32; 2],
    step: [f32; 2],
    level: f32,
    gradient: f32,
    quality: FxaaQuality,
) -> (f32, f32) {
    endpoint_with_search::<4>(image, origin, step, level, gradient, quality)
}

pub(super) fn endpoint_with_search<const SAMPLES: usize>(
    image: &Image,
    origin: [f32; 2],
    step: [f32; 2],
    level: f32,
    gradient: f32,
    quality: FxaaQuality,
) -> (f32, f32) {
    let distances = [1.0, 2.5, 4.5, 8.5, 20.5];
    let mut difference = 0.0;
    for &distance in distances.iter().take(SAMPLES) {
        let uv =
            std::array::from_fn(|i| (origin[i] + step[i] * distance) / f32::from(image.size[i]));
        difference = brightness(image.sample(uv), quality) - level;
        if difference.abs() >= gradient / 4.0 {
            return (distance, difference);
        }
    }
    (distances[SAMPLES], difference)
}

pub(super) fn resolve(image: &Image, pixel: [u16; 2], quality: FxaaQuality) -> [f32; 4] {
    resolve_with_search::<4>(image, pixel, quality, false)
}

// Deliberate reference mutations establish that each fixture isolates its intended stage.
pub(super) fn resolve_with_search<const SAMPLES: usize>(
    image: &Image,
    pixel: [u16; 2],
    quality: FxaaQuality,
    nine_tap_range: bool,
) -> [f32; 4] {
    let [x, y] = pixel.map(i32::from);
    let center = image.at(x, y);
    let m = brightness(center, quality);
    let axial = [(x, y - 1), (x, y + 1), (x - 1, y), (x + 1, y)]
        .map(|(x, y)| brightness(image.at(x, y), quality));
    let grid: [[f32; 3]; 3] = std::array::from_fn(|row| {
        std::array::from_fn(|col| {
            brightness(
                image.at(
                    x + i32::try_from(col).unwrap() - 1,
                    y + i32::try_from(row).unwrap() - 1,
                ),
                quality,
            )
        })
    });
    let taps = if nine_tap_range {
        grid.into_iter().flatten().collect::<Vec<_>>()
    } else {
        axial.to_vec()
    };
    let minimum = taps.iter().copied().fold(m, f32::min);
    let maximum = taps.iter().copied().fold(m, f32::max);
    let range = maximum - minimum;
    if range == 0.0
        || range
            < quality
                .minimum_threshold
                .max(maximum * quality.edge_threshold)
    {
        return center;
    }
    filter::<SAMPLES>(image, pixel, quality, grid, range)
}

fn filter<const SAMPLES: usize>(
    image: &Image,
    pixel: [u16; 2],
    quality: FxaaQuality,
    grid: [[f32; 3]; 3],
    range: f32,
) -> [f32; 4] {
    let FxaaKernel::Quality { sample_scale, .. } = quality.kernel else {
        panic!("quality kernel required")
    };
    let m = grid[1][1];
    let axial = [grid[0][1], grid[2][1], grid[1][0], grid[1][2]];
    let [horizontal, vertical] = scores(grid);
    let axis = usize::from(horizontal >= vertical);
    let pair = if axis == 1 {
        [axial[0], axial[1]]
    } else {
        [axial[2], axial[3]]
    };
    let gradients = pair.map(|v| (v - m).abs());
    let side = usize::from(gradients[0] < gradients[1]);
    let sign = if side == 0 { -1.0 } else { 1.0 };
    let level = (m + pair[side]) * 0.5;
    let gradient = gradients[0].max(gradients[1]);
    let position = pixel.map(|v| f32::from(v) + 0.5);
    let mut origin = position;
    origin[axis] += sign * sample_scale[axis] / 2.0;
    let mut step = [0.0; 2];
    step[1 - axis] = sample_scale[1 - axis];
    let positive = endpoint_with_search::<SAMPLES>(image, origin, step, level, gradient, quality);
    let negative =
        endpoint_with_search::<SAMPLES>(image, origin, step.map(|v| -v), level, gradient, quality);
    let nearest = if negative.0 < positive.0 {
        negative
    } else {
        positive
    };
    let edge = if (nearest.1 < 0.0) == (m - level < 0.0) {
        0.0
    } else {
        0.5 - nearest.0 / (negative.0 + positive.0)
    };
    let subpixel = subpixel(grid, range) * quality.subpixel_blend;
    let mut uv = position;
    uv[axis] += edge.max(subpixel) * sign * sample_scale[axis];
    let mut result = image.sample(std::array::from_fn(|i| uv[i] / f32::from(image.size[i])));
    result[3] = image.at(i32::from(pixel[0]), i32::from(pixel[1]))[3];
    result
}

fn subpixel(grid: [[f32; 3]; 3], range: f32) -> f32 {
    let corners = grid[0][0] + grid[0][2] + grid[2][0] + grid[2][2];
    let axial = grid[0][1] + grid[2][1] + grid[1][0] + grid[1][2];
    let average = (axial * 2.0 + corners) / 12.0;
    let t = ((average - grid[1][1]).abs() / range).clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)).powi(2)
}
