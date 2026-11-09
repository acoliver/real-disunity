use super::{Case, Edge, SIZE, gpu, policy};

// Expand the reduction tree, assigning equal weight to each valid child at
// every node. Rejected raw receivers contribute neither radiance nor measure.
fn node(sizes: &[[u32; 2]], level: usize, pixel: [u32; 2], edge: Edge) -> [f64; 2] {
    if level == 0 {
        let centre = sizes[0].map(|n| n / 2);
        if !matches!(edge, Edge::None) && pixel[0] > centre[0] {
            return [0.0; 2];
        }
        return [f64::from(u8::from(pixel == centre)), 1.0];
    }
    let mut total = [0.0; 2];
    let mut count = 0.0;
    for y in pixel[1] * 2..(pixel[1] * 2 + 2).min(sizes[level - 1][1]) {
        for x in pixel[0] * 2..(pixel[0] * 2 + 2).min(sizes[level - 1][0]) {
            let child = node(sizes, level - 1, [x, y], edge);
            total[0] += child[0];
            total[1] += child[1];
            count += 1.0;
        }
    }
    total.map(|v| v / count)
}

fn axis(base: u32, count: u32, level: usize) -> [(u32, f64); 2] {
    let span = 1u32 << level;
    let lo = (base.saturating_sub(span) / (2 * span)).min(count - 1);
    let hi = (lo + 1).min(count - 1);
    let position = f64::from(base) * 0.5;
    let centre =
        |i: u32| ((f64::from(i) + 0.5) * f64::from(span)).min(f64::from(i * span + base) * 0.5);
    let fraction = ((position - centre(lo)) / (centre(hi) - centre(lo)).max(0.5)).clamp(0.0, 1.0);
    [(lo, 1.0 - fraction), (hi, fraction)]
}

fn level_coverage(sizes: &[[u32; 2]], level: usize, edge: Edge) -> f64 {
    let mut total = [0.0; 2];
    for (y, wy) in axis(sizes[0][1], sizes[level][1], level) {
        for (x, wx) in axis(sizes[0][0], sizes[level][0], level) {
            let value = node(sizes, level, [x, y], edge);
            total[0] += value[0] * wx * wy;
            total[1] += value[1] * wx * wy;
        }
    }
    total[0] / total[1]
}

pub(super) fn multiscale(case: Case, lane: u32, lod: f32, tent: f32) -> f32 {
    let sizes = policy(case).radiance_extents(SIZE);
    let lower = (0..sizes.len())
        .rfind(|&i| gpu::scalar(u32::try_from(i).unwrap()) <= lod)
        .unwrap();
    let upper = (lower + 1).min(sizes.len() - 1);
    let a = level_coverage(&sizes, lower, case.edge);
    let b = level_coverage(&sizes, upper, case.edge);
    let broad = (a + (b - a) * f64::from(lod.fract())) / f64::from(lane + 1);
    let t = f64::from((lod - 1.0).clamp(0.0, 1.0));
    let blend = t * t * (3.0 - 2.0 * t);
    // Convert only the final independent double-precision integration to f32.
    let value = f64::from(tent) * (1.0 - blend) + broad * blend;
    value.to_string().parse().unwrap()
}

#[test]
fn recursive_partial_bins_are_equal_child_not_area_weighted() {
    let sizes = [[5, 1], [3, 1], [2, 1], [1, 1]];
    let exact = |actual: [f64; 2], expected: [f64; 2]| {
        assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
    };
    exact(node(&sizes, 3, [0, 0], Edge::None), [0.125, 1.0]);
    exact(node(&sizes, 3, [0, 0], Edge::Normal), [0.125, 0.375]);
    let floor = [[5, 1], [2, 1], [1, 1]];
    exact(node(&floor, 2, [0, 0], Edge::None), [0.25, 1.0]);
    exact(node(&floor, 2, [0, 0], Edge::Normal), [0.25, 0.75]);
}

#[test]
fn odd_extent_bilinear_weights_normalize_each_classified_lod() {
    let sizes = [[17, 13], [9, 7], [5, 4], [3, 2]];
    // At image centre, level two has coarse weights 5/8 and 7/8 for
    // the hit bin. Level three has 9/16 and 8/13 at its partial Y bin.
    for (level, edge, expected) in [
        (2, Edge::None, 35.0 / 1024.0),
        (3, Edge::None, 9.0 / 1664.0),
        (2, Edge::Normal, 35.0 / 544.0),
        (3, Edge::Normal, 9.0 / 845.0),
    ] {
        assert!((level_coverage(&sizes, level, edge) - expected).abs() < 1e-15);
    }
}
