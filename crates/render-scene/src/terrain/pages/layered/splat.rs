use super::LayeredReference;
use worldrunner_render_plan::TerrainSplatFilter;

const OFFSETS: [[f32; 2]; 14] = [
    [0.0, 0.0],
    [0.023_591, 0.067_420],
    [-0.111_690, 0.089_070],
    [-0.181_441, -0.114_006],
    [0.063_576, -0.278_551],
    [0.354_897, -0.039_989],
    [0.185_952, 0.386_129],
    [-0.353_551, 0.353_556],
    [-0.514_841, -0.247_930],
    [0.071_972, -0.638_816],
    [0.696_376, -0.158_950],
    [0.418_032, 0.665_279],
    [-0.534_412, 0.670_147],
    [-0.876_467, -0.306_677],
];
const WEIGHTS: [f32; 14] = [
    1.0,
    0.928_571_7,
    0.857_143,
    0.785_714_6,
    0.714_285_8,
    0.642_857_1,
    0.571_428_2,
    0.499_999_8,
    0.428_571_4,
    0.357_142_4,
    0.285_713_4,
    0.214_285_1,
    0.142_857_7,
    0.071_428_8,
];

impl LayeredReference<'_> {
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "matches shader f32 arithmetic and saturating unsigned world-position hash conversion"
    )]
    pub(super) fn coverage(&self, layer: u8, world: [f32; 2], radius_mm: u32) -> f32 {
        let grid = [
            (world[0] - self.source.type_grid.origin_millimeters()[0] as f32 * 0.001)
                / (self.source.type_grid.spacing_millimeters() as f32 * 0.001),
            (world[1] - self.source.type_grid.origin_millimeters()[1] as f32 * 0.001)
                / (self.source.type_grid.spacing_millimeters() as f32 * 0.001),
        ];
        if self.source.filter == TerrainSplatFilter::Bilinear {
            return self.membership(layer, grid);
        }
        let hash = world.map(|x| (x * 1_000_000.0) as u32);
        let hash = (hash[0].wrapping_mul(1_597_334_677) ^ hash[1].wrapping_mul(3_812_015_801))
            .wrapping_mul(1_597_334_677);
        let noise = hash as f32 / u32::MAX as f32;
        let sector = (noise * 100_000.0) as u32 % 18;
        let angle = sector as f32 * std::f32::consts::PI / 9.0;
        // Quantize the rotation table to the authored six decimal places.
        let sine = (angle.sin() * 1_000_000.0).round() / 1_000_000.0;
        let cosine = (angle.cos() * 1_000_000.0).round() / 1_000_000.0;
        let radius = radius_mm as f32 / self.source.type_grid.spacing_millimeters() as f32
            * (1.0 + noise * 0.125);
        OFFSETS
            .into_iter()
            .zip(WEIGHTS)
            .map(|(offset, weight)| {
                self.membership(
                    layer,
                    [
                        grid[0] + (offset[0] * sine - offset[1] * cosine) * radius,
                        grid[1] + (offset[0] * cosine + offset[1] * sine) * radius,
                    ],
                ) * weight
            })
            .sum::<f32>()
            / 7.499_999
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "bounded floored f32 grid coordinates become integer texel positions"
    )]
    pub(super) fn membership(&self, layer: u8, grid: [f32; 2]) -> f32 {
        let dimensions = [
            self.source.type_grid.dimensions()[0],
            self.source.type_grid.dimensions()[1],
        ];
        let position: [f32; 2] =
            std::array::from_fn(|axis| (grid[axis] - 0.5).clamp(-1.0, dimensions[axis] as f32));
        let base = position.map(|x| x.floor() as i64);
        let fraction = position.map(|x| x - x.floor());
        let read = |x: i64, y: i64| {
            let x =
                usize::try_from(x.clamp(0, i64::from(self.source.type_grid.dimensions()[0]) - 1))
                    .expect("validated grid");
            let y =
                usize::try_from(y.clamp(0, i64::from(self.source.type_grid.dimensions()[1]) - 1))
                    .expect("validated grid");
            f32::from(u8::from(
                self.source.type_grid.indices()
                    [y * self.source.type_grid.dimensions()[0] as usize + x]
                    == layer,
            ))
        };
        let a = read(base[0], base[1]);
        let b = read(base[0] + 1, base[1]);
        let c = read(base[0], base[1] + 1);
        let d = read(base[0] + 1, base[1] + 1);
        let lower = a + (b - a) * fraction[0];
        let upper = c + (d - c) * fraction[0];
        lower + (upper - lower) * fraction[1]
    }
}
