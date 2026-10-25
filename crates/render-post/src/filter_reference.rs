//! Test-only scalar specification of the independently authored post filters.

#[derive(Clone)]
pub(super) struct Image {
    pub size: [u16; 2],
    pub pixels: Vec<[f32; 4]>,
}
impl Image {
    pub fn at(&self, x: i32, y: i32) -> [f32; 4] {
        let x = u16::try_from(x.clamp(0, i32::from(self.size[0]) - 1)).unwrap();
        let y = u16::try_from(y.clamp(0, i32::from(self.size[1]) - 1)).unwrap();
        self.pixels[usize::from(y) * usize::from(self.size[0]) + usize::from(x)]
    }
    pub fn sample(&self, uv: [f32; 2]) -> [f32; 4] {
        let axis = |i: usize| {
            let p = (uv[i] * f32::from(self.size[i]) - 0.5).clamp(0.0, f32::from(self.size[i] - 1));
            let base = (0..self.size[i])
                .rev()
                .find(|&v| f32::from(v) <= p)
                .unwrap();
            (i32::from(base), p - f32::from(base))
        };
        let (x, fx) = axis(0);
        let (y, fy) = axis(1);
        let a = blend(self.at(x, y), self.at(x + 1, y), fx);
        let b = blend(self.at(x, y + 1), self.at(x + 1, y + 1), fx);
        blend(a, b, fy)
    }
    pub fn render(size: [u16; 2], f: impl Fn([f32; 2]) -> [f32; 4]) -> Self {
        let mut pixels = Vec::new();
        for y in 0..size[1] {
            for x in 0..size[0] {
                pixels.push(f([
                    (f32::from(x) + 0.5) / f32::from(size[0]),
                    (f32::from(y) + 0.5) / f32::from(size[1]),
                ]));
            }
        }
        Self { size, pixels }
    }
}
fn blend(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|i| a[i] * (1.0 - t) + b[i] * t)
}

pub(super) fn prefilter(c: [f32; 4], threshold: f32, knee: f32) -> [f32; 4] {
    let rgb: [f32; 3] = std::array::from_fn(|i| if c[i].is_finite() { c[i].max(0.0) } else { 0.0 });
    let peak = rgb.into_iter().fold(0.0_f32, f32::max);
    let radius = threshold * knee;
    let mut excess = (peak - threshold).max(0.0);
    if radius > 0.0 {
        let ramp = (peak - threshold + radius).clamp(0.0, 2.0 * radius);
        excess = excess.max(ramp * ramp / (4.0 * radius));
    }
    let scale = if peak > 0.0 { excess / peak } else { 0.0 };
    [rgb[0] * scale, rgb[1] * scale, rgb[2] * scale, 1.0]
}

pub(super) fn tent(image: &Image, uv: [f32; 2]) -> [f32; 4] {
    let mut sum = [0.0; 4];
    for y in -1_i16..=1 {
        for x in -1_i16..=1 {
            let wx = if x == 0 { 2.0 } else { 1.0 };
            let wy = if y == 0 { 2.0 } else { 1.0 };
            let c = image.sample([
                uv[0] + f32::from(x) / f32::from(image.size[0]),
                uv[1] + f32::from(y) / f32::from(image.size[1]),
            ]);
            for i in 0..3 {
                sum[i] += c[i] * (wx * wy / 16.0);
            }
        }
    }
    sum[3] = 1.0;
    sum
}
pub(super) fn upsample(high: &Image, low: &Image, uv: [f32; 2]) -> [f32; 4] {
    let mut c = blend(high.sample(uv), tent(low, uv), 0.5);
    c[3] = 1.0;
    c
}
fn luma(c: [f32; 4]) -> f32 {
    (c[0] + 2.0 * c[1] + c[2]) * 0.25
}
pub(super) fn fxaa(image: &Image, uv: [f32; 2], amount: f32) -> [f32; 4] {
    let center = image.sample(uv);
    let mut minimum = luma(center);
    let mut maximum = minimum;
    let mut gradient = [0.0; 2];
    for y in -1_i16..=1 {
        for x in -1_i16..=1 {
            let value = luma(image.sample([
                uv[0] + f32::from(x) / f32::from(image.size[0]),
                uv[1] + f32::from(y) / f32::from(image.size[1]),
            ]));
            minimum = minimum.min(value);
            maximum = maximum.max(value);
            gradient[0] += value * f32::from(x) * if y == 0 { 2.0 } else { 1.0 };
            gradient[1] += value * f32::from(y) * if x == 0 { 2.0 } else { 1.0 };
        }
    }
    let norm = gradient[0].abs().max(gradient[1].abs());
    if maximum - minimum <= (1.0 / 64.0_f32).max(maximum / 10.0) || norm == 0.0 {
        return center;
    }
    let delta = [
        -gradient[1] / norm * 1.5 / f32::from(image.size[0]),
        gradient[0] / norm * 1.5 / f32::from(image.size[1]),
    ];
    let negative = image.sample([uv[0] - delta[0], uv[1] - delta[1]]);
    let positive = image.sample([uv[0] + delta[0], uv[1] + delta[1]]);
    let filtered = std::array::from_fn(|i| center[i] * 0.5 + (negative[i] + positive[i]) * 0.25);
    let mut result = blend(center, filtered, amount);
    result[3] = center[3];
    result
}

pub(super) fn half_bits(value: f32) -> u16 {
    assert!(value.is_finite() && (0.0..=65504.0).contains(&value));
    let decode = |bits: u16| crate::tone_map_gpu_tests::half(&bits.to_le_bytes());
    let mut low = 0_u16;
    let mut high = 0x7bff_u16;
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if decode(middle) <= f64::from(value) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    if low == 0x7bff {
        return low;
    }
    let below = f64::from(value) - decode(low);
    let above = decode(low + 1) - f64::from(value);
    if above < below || (above.total_cmp(&below).is_eq() && low % 2 == 1) {
        low + 1
    } else {
        low
    }
}

#[test]
fn reference_preserves_constants_and_prefilter_contract() {
    let image = Image {
        size: [4, 4],
        pixels: vec![[0.25, 0.5, 0.75, 1.0]; 16],
    };
    assert_eq!(
        tent(&image, [0.5, 0.5]).map(f32::to_bits),
        image.pixels[0].map(f32::to_bits)
    );
    assert_eq!(
        upsample(&image, &image, [0.5, 0.5]).map(f32::to_bits),
        image.pixels[0].map(f32::to_bits)
    );
    assert_eq!(
        fxaa(&image, [0.5, 0.5], 1.0).map(f32::to_bits),
        image.pixels[0].map(f32::to_bits)
    );
    for (v, expected) in [(0.25, 0.0), (0.75, 0.03125), (1.0, 0.125), (2.0, 1.0)] {
        assert_eq!(
            prefilter([v; 4], 1.0, 0.5).map(f32::to_bits),
            [expected, expected, expected, 1.0_f32].map(f32::to_bits)
        );
    }
    assert_eq!(
        prefilter([f32::NAN, -1.0, f32::INFINITY, 0.0], 0.0, 0.0).map(f32::to_bits),
        [0.0, 0.0, 0.0, 1.0_f32].map(f32::to_bits)
    );
}
