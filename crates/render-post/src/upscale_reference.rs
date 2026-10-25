//! Independent scalar f64 reference for the FSR 1 equations, with exact reciprocals.
#[derive(Clone)]
pub(crate) struct Image {
    pub size: [i32; 2],
    pub pixels: Vec<[f64; 4]>,
}
impl Image {
    pub fn load(&self, x: i32, y: i32) -> [f64; 4] {
        let x = x.clamp(0, self.size[0] - 1);
        let y = y.clamp(0, self.size[1] - 1);
        self.pixels[usize::try_from(y * self.size[0] + x).unwrap()]
    }
    pub fn reconstruct(&self, output: [i32; 2], easu: bool) -> Self {
        let mut pixels = Vec::new();
        for y in 0..output[1] {
            for x in 0..output[0] {
                let n = [
                    (2 * x + 1) * self.size[0] - output[0],
                    (2 * y + 1) * self.size[1] - output[1],
                ];
                let d = output.map(|v| 2 * v);
                let q = [n[0].div_euclid(d[0]), n[1].div_euclid(d[1])];
                let f = [
                    f64::from(n[0].rem_euclid(d[0])) / f64::from(d[0]),
                    f64::from(n[1].rem_euclid(d[1])) / f64::from(d[1]),
                ];
                pixels.push(if easu {
                    self.easu(q, f)
                } else {
                    self.bilinear(q, f)
                });
            }
        }
        Self {
            size: output,
            pixels,
        }
    }
    fn bilinear(&self, q: [i32; 2], f: [f64; 2]) -> [f64; 4] {
        std::array::from_fn(|c| {
            (self.load(q[0], q[1])[c] * (1.0 - f[0]) + self.load(q[0] + 1, q[1])[c] * f[0])
                * (1.0 - f[1])
                + (self.load(q[0], q[1] + 1)[c] * (1.0 - f[0])
                    + self.load(q[0] + 1, q[1] + 1)[c] * f[0])
                    * f[1]
        })
    }
    fn gradient(&self, x: i32, y: i32) -> [f64; 3] {
        let luma = |x, y| {
            let c = self.load(x, y);
            c[2] * 0.5 + c[0] * 0.5 + c[1]
        };
        let center = luma(x, y);
        let top = luma(x, y - 1);
        let left = luma(x - 1, y);
        let right = luma(x + 1, y);
        let bottom = luma(x, y + 1);
        let dx = right - left;
        let dy = bottom - top;
        let lx = (dx.abs() / (right - center).abs().max((center - left).abs()).max(1e-20)).min(1.0);
        let ly = (dy.abs() / (bottom - center).abs().max((center - top).abs()).max(1e-20)).min(1.0);
        [dx, dy, lx * lx + ly * ly]
    }
    fn kernel(&self, q: [i32; 2], f: [f64; 2]) -> ([f64; 2], [f64; 2], f64) {
        let mut gradient = [0.0; 3];
        for (dx, dy, w) in [
            (0, 0, (1.0 - f[0]) * (1.0 - f[1])),
            (1, 0, f[0] * (1.0 - f[1])),
            (0, 1, (1.0 - f[0]) * f[1]),
            (1, 1, f[0] * f[1]),
        ] {
            let g = self.gradient(q[0] + dx, q[1] + dy);
            for c in 0..3 {
                gradient[c] += g[c] * w;
            }
        }
        let magnitude = gradient[0] * gradient[0] + gradient[1] * gradient[1];
        let dir = if magnitude < 1.0 / 32768.0 {
            [1.0, 0.0]
        } else {
            [
                gradient[0] / magnitude.sqrt(),
                gradient[1] / magnitude.sqrt(),
            ]
        };
        let len = (gradient[2] * 0.5).powi(2);
        let stretch = (dir[0] * dir[0] + dir[1] * dir[1]) / dir[0].abs().max(dir[1].abs());
        let axes = [1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len];
        let lobe = 0.5 - 0.29 * len;
        (dir, axes, lobe)
    }
    fn easu(&self, q: [i32; 2], f: [f64; 2]) -> [f64; 4] {
        let (dir, axes, lobe) = self.kernel(q, f);
        let mut sum = [0.0; 3];
        let mut weights = 0.0;
        for (x, y) in [
            (0, -1),
            (1, -1),
            (-1, 1),
            (0, 1),
            (0, 0),
            (-1, 0),
            (1, 1),
            (2, 1),
            (2, 0),
            (1, 0),
            (1, 2),
            (0, 2),
        ] {
            let off = [f64::from(x) - f[0], f64::from(y) - f[1]];
            let vx = (off[0] * dir[0] + off[1] * dir[1]) * axes[0];
            let vy = (-off[0] * dir[1] + off[1] * dir[0]) * axes[1];
            let distance = (vx * vx + vy * vy).min(1.0 / lobe);
            let w = (1.5625 * (0.4 * distance - 1.0).powi(2) - 0.5625)
                * (lobe * distance - 1.0).powi(2);
            let color = self.load(q[0] + x, q[1] + y);
            for c in 0..3 {
                sum[c] += color[c] * w;
            }
            weights += w;
        }
        let corners = [
            self.load(q[0], q[1]),
            self.load(q[0] + 1, q[1]),
            self.load(q[0], q[1] + 1),
            self.load(q[0] + 1, q[1] + 1),
        ];
        let mut result = self.bilinear(q, f);
        for c in 0..3 {
            let min = corners.iter().map(|v| v[c]).fold(f64::INFINITY, f64::min);
            let max = corners
                .iter()
                .map(|v| v[c])
                .fold(f64::NEG_INFINITY, f64::max);
            result[c] = (sum[c] / weights).clamp(min, max);
        }
        result
    }
    pub fn sharpen(&self, stops: f64) -> Self {
        let mut pixels = Vec::new();
        for y in 0..self.size[1] {
            for x in 0..self.size[0] {
                pixels.push(self.rcas(x, y, stops));
            }
        }
        Self {
            size: self.size,
            pixels,
        }
    }
    fn rcas(&self, x: i32, y: i32, stops: f64) -> [f64; 4] {
        let center = self.load(x, y);
        let ring = [
            self.load(x, y - 1),
            self.load(x - 1, y),
            self.load(x + 1, y),
            self.load(x, y + 1),
        ];
        let mut lobe: f64 = -0.1875;
        for c in 0..3 {
            let min = ring.iter().map(|p| p[c]).fold(f64::INFINITY, f64::min);
            let max = ring.iter().map(|p| p[c]).fold(f64::NEG_INFINITY, f64::max);
            let lower = -min.min(center[c]) / (4.0 * max).max(1e-20);
            let upper = (1.0 - max.max(center[c])) / (4.0 * min - 4.0).min(-1e-20);
            lobe = lobe.max(lower.max(upper).min(0.0));
        }
        lobe *= 2.0_f64.powf(-stops);
        let mut result = center;
        for c in 0..3 {
            result[c] =
                (center[c] + lobe * ring.iter().map(|p| p[c]).sum::<f64>()) / (1.0 + 4.0 * lobe);
        }
        result
    }
}
