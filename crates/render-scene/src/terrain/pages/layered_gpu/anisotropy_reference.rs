//! Independent f64 oracle over the exact authored byte mip chain.
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};

pub(super) struct Reference {
    mips: Vec<(u16, u16, Vec<u8>)>,
}

impl Reference {
    pub(super) fn new() -> Self {
        let mips = (0..4)
            .map(|mip| {
                let width = (8_u16 >> mip).max(1);
                let height = (4_u16 >> mip).max(1);
                let bytes = (0..height)
                    .flat_map(|y| {
                        (0..width).map(move |x| {
                            u8::try_from(30 + (x * 47 + y * 71 + mip * 31) % 181).unwrap()
                        })
                    })
                    .collect();
                (width, height, bytes)
            })
            .collect();
        Self { mips }
    }

    pub(super) fn constant_texture(&self, value: [u8; 4]) -> SceneTexture {
        let mut texture = self.texture();
        let SceneTexture::Texels { bytes, .. } = &mut texture else {
            unreachable!();
        };
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.copy_from_slice(&value);
        }
        texture
    }

    pub(super) fn texture(&self) -> SceneTexture {
        SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 8,
            height: 4,
            mips: 4,
            bytes: self
                .mips
                .iter()
                .flat_map(|(_, _, bytes)| bytes.iter().flat_map(|v| [*v, *v, *v, 255]))
                .collect(),
        }
    }

    pub(super) fn sample(&self, uv: glam::DVec2, gradients: [glam::DVec2; 2], level: u16) -> f64 {
        let size = glam::DVec2::new(8.0, 4.0);
        let mut axes = gradients.map(|axis| ((axis * size).length(), axis));
        axes.sort_by(|a, b| a.0.total_cmp(&b.0));
        let [(minor, _), (major, direction)] = axes;
        let count = (major / minor.max(f64::MIN_POSITIVE))
            .ceil()
            .clamp(1.0, f64::from(level));
        let lod = (major / count).max(minor).max(1.0).log2().min(3.0);
        let mut sum = 0.0;
        let mut tap = 0.0;
        while tap < count {
            let position = uv + direction * ((tap + 0.5) / count - 0.5);
            sum += self.trilinear(position, lod);
            tap += 1.0;
        }
        sum / count
    }

    fn trilinear(&self, uv: glam::DVec2, lod: f64) -> f64 {
        self.mips
            .iter()
            .enumerate()
            .map(|(index, mip)| {
                let distance = (lod - f64::from(u32::try_from(index).unwrap())).abs();
                Self::bilinear(uv, mip) * (1.0 - distance).max(0.0)
            })
            .sum()
    }

    fn bilinear(uv: glam::DVec2, mip: &(u16, u16, Vec<u8>)) -> f64 {
        let (width, height, bytes) = mip;
        let coordinate =
            uv * glam::DVec2::new(f64::from(*width), f64::from(*height)) - glam::DVec2::splat(0.5);
        let extent = glam::DVec2::new(f64::from(*width), f64::from(*height));
        let wrapped = coordinate.rem_euclid(extent);
        let base = wrapped.floor().as_uvec2();
        let fraction = wrapped - wrapped.floor();
        let mut sum = 0.0;
        for y in 0..2_u32 {
            for x in 0..2_u32 {
                let column = (base.x + x) % u32::from(*width);
                let row = (base.y + y) % u32::from(*height);
                let index = usize::try_from(row * u32::from(*width) + column).unwrap();
                let wx = if x == 0 { 1.0 - fraction.x } else { fraction.x };
                let wy = if y == 0 { 1.0 - fraction.y } else { fraction.y };
                sum += f64::from(bytes[index]) * wx * wy;
            }
        }
        sum
    }
}
