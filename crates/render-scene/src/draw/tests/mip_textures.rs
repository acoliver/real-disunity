//! Fixture textures for the end to end mip bias measurement: a worst case stripe pattern and a
//! smooth multi-frequency pattern, both 128 texels square with a box-filtered 8 level chain.
use super::*;
use worldrunner_render_plan::TexelFormat;

const SIZE: usize = 128;
const LEVELS: usize = 8;

fn texture(bytes: Vec<u8>) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 128,
        height: 128,
        mips: 8,
        bytes: bytes.into(),
    }
}

/// Vertical stripes of 8 texels (4 dark, 4 light). The energy sits at one frequency that is
/// right at the render Nyquist for the reduced scales, so this is the worst case for shimmer.
pub(super) fn stripes() -> SceneTexture {
    let dark = 13_u32;
    let light = 242_u32;
    let mut bytes = Vec::new();
    for level in 0..LEVELS {
        let size = SIZE >> level;
        for _ in 0..size {
            for x in 0..size {
                let columns = 1_usize << level;
                let lit = (x * columns..(x + 1) * columns)
                    .filter(|column| (column / 4) % 2 == 1)
                    .count();
                let lit = u32::try_from(lit).unwrap();
                let width = u32::try_from(columns).unwrap();
                let value = u8::try_from((light * lit + dark * (width - lit)) / width).unwrap();
                bytes.extend_from_slice(&[value, value, value, 255]);
            }
        }
    }
    texture(bytes)
}

/// Five sinusoids with periods of 64, 32, 21, 16 and 11 texels along different directions and
/// amplitudes falling with frequency (40, 30, 20, 15, 8 of 255). The energy is spread over
/// several octaves and not concentrated at the render Nyquist.
fn smooth_level_zero(x: usize, y: usize) -> u8 {
    let (x, y) = (
        f64::from(u16::try_from(x).unwrap()),
        f64::from(u16::try_from(y).unwrap()),
    );
    let tau = std::f64::consts::TAU;
    let waves = [
        (40.0, 64.0, 1.0, 0.0, 0.3),
        (30.0, 32.0, 1.0, 2.0, 1.1),
        (20.0, 21.0, 0.0, 1.0, 2.0),
        (15.0, 16.0, 1.0, -1.0, 0.7),
        (8.0, 11.0, 2.0, 1.0, 4.0),
    ];
    let value = waves
        .iter()
        .fold(128.0, |sum, &(amplitude, period, dx, dy, phase)| {
            sum + amplitude * (tau * (dx * x + dy * y) / period + phase).sin()
        });
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the sum of amplitudes is 113, so the rounded value is within 15..=241"
    )]
    let byte = value.round() as u8;
    byte
}

/// The smooth pattern with each coarser level the rounded 2x2 box average of the level above.
pub(super) fn smooth() -> SceneTexture {
    let mut level: Vec<u8> = (0..SIZE * SIZE)
        .map(|index| smooth_level_zero(index % SIZE, index / SIZE))
        .collect();
    let mut bytes = Vec::new();
    for mip in 0..LEVELS {
        let size = SIZE >> mip;
        for value in &level {
            bytes.extend_from_slice(&[*value, *value, *value, 255]);
        }
        let half = size / 2;
        level = (0..half * half)
            .map(|index| {
                let (x, y) = (2 * (index % half), 2 * (index / half));
                let sum: u32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .map(|(dx, dy)| u32::from(level[(y + dy) * size + x + dx]))
                    .sum();
                u8::try_from((sum + 2) / 4).unwrap()
            })
            .collect();
    }
    texture(bytes)
}
