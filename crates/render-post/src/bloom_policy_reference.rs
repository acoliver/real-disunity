use super::reference::Image;
use crate::{
    Bloom, BloomReduction, BloomTerminal, BrightCurve, BrightExtraction, GaussianKernelRule,
    GaussianLevel,
};

pub(super) fn fixture(size: [u16; 2]) -> Image {
    Image::new(size, |x, y| {
        let v = 2.0 + f64::from((x * 7 + y * 3) % 13) * 0.5;
        [v, v * 0.25, 0.5]
    })
}

fn curve(x: f64, kind: BrightCurve) -> f64 {
    match kind {
        BrightCurve::AcesFilmic => x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14),
        BrightCurve::Reinhard => x / (1.0 + x),
    }
}

fn extract(rgb: [f64; 3], bloom: Bloom, exposure: f64) -> [f64; 3] {
    let threshold = f64::from(bloom.threshold);
    match bloom.policies.extraction {
        BrightExtraction::SoftKnee => {
            let brightness = rgb.into_iter().fold(0.0, f64::max);
            let knee = threshold * f64::from(bloom.knee);
            let soft = (brightness - threshold + knee)
                .clamp(0.0, 2.0 * knee)
                .powi(2)
                / (4.0 * knee).max(0.00001);
            rgb.map(|v| v * (brightness - threshold).max(soft) / brightness.max(0.00001))
        }
        BrightExtraction::ToneCurve(p) => {
            let rgb = rgb.map(|v| v.min(f64::from(p.maximum)));
            let lum: f64 = rgb
                .iter()
                .zip(p.luminance)
                .map(|(v, w)| v * f64::from(w))
                .sum();
            let gate = (curve(exposure * f64::from(p.exposure_scale) * lum, p.curve)
                - curve(threshold, p.curve))
            .max(0.0)
                * f64::from(p.contrast);
            let maximum = rgb.into_iter().fold(f64::from(p.epsilon), f64::max);
            rgb.map(|v| v * gate / maximum)
        }
    }
}

fn quarter(source: &Image, x: i32, y: i32, size: [u16; 2]) -> [f64; 3] {
    let mut sum = [0.0; 3];
    for dy in [-1, 1] {
        for dx in [-1, 1] {
            let axis = |p: i32, src: u16, dst: u16, offset: i32| {
                let denominator = 4 * i32::from(dst);
                let numerator =
                    (4 * p + 2) * i32::from(src) - 2 * i32::from(dst) + offset * i32::from(dst);
                (
                    numerator.div_euclid(denominator),
                    f64::from(numerator.rem_euclid(denominator)) / f64::from(denominator),
                )
            };
            let (qx, fx) = axis(x, source.size[0], size[0], dx);
            let (qy, fy) = axis(y, source.size[1], size[1], dy);
            for (c, value) in sum.iter_mut().enumerate() {
                *value += ((source.load(qx, qy)[c] * (1.0 - fx) + source.load(qx + 1, qy)[c] * fx)
                    * (1.0 - fy)
                    + (source.load(qx, qy + 1)[c] * (1.0 - fx)
                        + source.load(qx + 1, qy + 1)[c] * fx)
                        * fy)
                    * 0.25;
            }
        }
    }
    sum
}

fn reduce(source: &Image, bloom: Bloom, exposure: f64, first: bool) -> Image {
    let size = source.size.map(|v| v.div_ceil(2));
    Image::new(size, |x, y| {
        if bloom.policies.reduction == BloomReduction::QuarterTexelAfterAverage {
            let average = quarter(source, x, y, size);
            return if first {
                extract(average, bloom, exposure)
            } else {
                average
            };
        }
        let mut sum = [0.0; 3];
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let rgb = source.load(x * 2 + dx, y * 2 + dy);
            let rgb = if first {
                extract(rgb, bloom, exposure)
            } else {
                rgb
            };
            for c in 0..3 {
                sum[c] += rgb[c] * 0.25;
            }
        }
        sum
    })
}

fn blur(source: &Image, level: GaussianLevel, rule: GaussianKernelRule, vertical: bool) -> Image {
    let extent = u32::from(source.size[usize::from(vertical)]);
    let radius = match rule {
        GaussianKernelRule::RadiusThird => level.radius,
        GaussianKernelRule::AxisClampedPairs => 2 * level.radius.min(extent.max(1)).div_ceil(2),
    };
    let sigma = match rule {
        GaussianKernelRule::RadiusThird => f64::from(radius) / 3.0,
        GaussianKernelRule::AxisClampedPairs => (f64::from(radius) + 0.5) / 3.0,
    };
    let weights: Vec<_> = (0..=radius)
        .map(|i| {
            if radius == 0 {
                1.0
            } else {
                (-0.5 * (f64::from(i) / sigma).powi(2)).exp()
            }
        })
        .collect();
    let norm = weights[0] + 2.0 * weights[1..].iter().sum::<f64>();
    Image::new(source.size, |x, y| {
        std::array::from_fn(|c| {
            let mut value = source.load(x, y)[c] * weights[0];
            for (i, w) in weights.iter().enumerate().skip(1) {
                let d = i32::try_from(i).unwrap();
                let (dx, dy) = if vertical { (0, d) } else { (d, 0) };
                value += w * (source.load(x + dx, y + dy)[c] + source.load(x - dx, y - dy)[c]);
            }
            value / norm
        })
    })
}

pub(super) fn pyramid(
    input: &Image,
    bloom: Bloom,
    levels: &[GaussianLevel; 8],
    exposure: f64,
) -> Vec<Image> {
    let mut images = vec![reduce(input, bloom, exposure, true)];
    for _ in 1..bloom.levels {
        assert!(
            bloom.policies.terminal == BloomTerminal::Repeat
                || images.last().unwrap().size != [1, 1],
            "reference requires Repeat for additional terminal levels"
        );
        images.push(reduce(images.last().unwrap(), bloom, exposure, false));
    }
    for i in (usize::try_from(bloom.policies.selection.first_blurred).unwrap()..images.len()).rev()
    {
        let blurred = blur(
            &blur(&images[i], levels[i], bloom.policies.kernel, false),
            levels[i],
            bloom.policies.kernel,
            true,
        );
        images[i] = Image::new(blurred.size, |x, y| {
            let local = blurred.load(x, y);
            let lower = images
                .get(i + 1)
                .map_or([0.0; 3], |image| image.scaled(x, y, blurred.size));
            std::array::from_fn(|c| {
                local[c] * f64::from(levels[i].tint[c]) * f64::from(levels[i].weight) + lower[c]
            })
        });
    }
    images
}
