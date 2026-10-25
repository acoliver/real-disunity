//! Small-image reference uses discrete taps and exact integer resampling coordinates.
use crate::{Bloom, GaussianLevel};

#[derive(Clone)]
pub(super) struct Image {
    pub size: [u16; 2],
    pub pixels: Vec<[f64; 3]>,
}
impl Image {
    pub fn new(size: [u16; 2], pixel: impl Fn(i32, i32) -> [f64; 3]) -> Self {
        let pixels = (0..size[1])
            .flat_map(|y| (0..size[0]).map(move |x| (x, y)))
            .map(|(x, y)| pixel(i32::from(x), i32::from(y)))
            .collect();
        Self { size, pixels }
    }
    pub fn load(&self, x: i32, y: i32) -> [f64; 3] {
        let x = usize::try_from(x.clamp(0, i32::from(self.size[0]) - 1)).unwrap();
        let y = usize::try_from(y.clamp(0, i32::from(self.size[1]) - 1)).unwrap();
        self.pixels[y * usize::from(self.size[0]) + x]
    }
    pub fn scaled(&self, x: i32, y: i32, size: [u16; 2]) -> [f64; 3] {
        let axis = |p: i32, source: u16, target: u16| {
            let numerator = (2 * p + 1) * i32::from(source) - i32::from(target);
            let denominator = 2 * i32::from(target);
            (
                numerator.div_euclid(denominator),
                f64::from(numerator.rem_euclid(denominator)) / f64::from(denominator),
            )
        };
        let (qx, fx) = axis(x, self.size[0], size[0]);
        let (qy, fy) = axis(y, self.size[1], size[1]);
        let samples = [
            self.load(qx, qy),
            self.load(qx + 1, qy),
            self.load(qx, qy + 1),
            self.load(qx + 1, qy + 1),
        ];
        std::array::from_fn(|c| {
            (samples[0][c] * (1.0 - fx) + samples[1][c] * fx) * (1.0 - fy)
                + (samples[2][c] * (1.0 - fx) + samples[3][c] * fx) * fy
        })
    }
    fn downsample(&self) -> Self {
        Self::new(self.size.map(|v| v.div_ceil(2)), |x, y| {
            let samples = [
                self.load(x * 2, y * 2),
                self.load(x * 2 + 1, y * 2),
                self.load(x * 2, y * 2 + 1),
                self.load(x * 2 + 1, y * 2 + 1),
            ];
            std::array::from_fn(|c| samples.iter().map(|p| p[c]).sum::<f64>() * 0.25)
        })
    }
    fn blur(&self, radius: u32, vertical: bool) -> Self {
        let weights = crate::gaussian::discrete_weights(radius).unwrap();
        Self::new(self.size, |x, y| {
            std::array::from_fn(|c| {
                let mut value = self.load(x, y)[c] * f64::from(weights[0]);
                for (index, weight) in weights.iter().enumerate().skip(1) {
                    let offset = i32::try_from(index).unwrap();
                    let (dx, dy) = if vertical { (0, offset) } else { (offset, 0) };
                    value += (self.load(x - dx, y - dy)[c] + self.load(x + dx, y + dy)[c])
                        * f64::from(*weight);
                }
                value
            })
        })
    }
}

pub(super) fn pyramid(input: &Image, bloom: Bloom, levels: &[GaussianLevel; 8]) -> Vec<Image> {
    let bright = Image::new(input.size, |x, y| {
        let color = input
            .load(x, y)
            .map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 });
        let brightness = color.into_iter().fold(0.0_f64, f64::max);
        let threshold = f64::from(bloom.threshold);
        let knee = threshold * f64::from(bloom.knee);
        let soft = (brightness - threshold + knee)
            .clamp(0.0, 2.0 * knee)
            .powi(2)
            / (4.0 * knee).max(0.00001);
        let weight = (brightness - threshold).max(soft) / brightness.max(0.00001);
        color.map(|v| v * weight)
    });
    let mut images = vec![bright.downsample()];
    for _ in 1..bloom.levels {
        images.push(images.last().unwrap().downsample());
    }
    for i in (0..images.len()).rev() {
        let blurred = images[i]
            .blur(levels[i].radius, false)
            .blur(levels[i].radius, true);
        images[i] = Image::new(blurred.size, |x, y| {
            let local = blurred.load(x, y);
            let lower = images
                .get(i + 1)
                .map_or([0.0; 3], |lower| lower.scaled(x, y, blurred.size));
            std::array::from_fn(|c| {
                local[c] * f64::from(levels[i].tint[c]) * f64::from(levels[i].weight) + lower[c]
            })
        });
    }
    images
}
