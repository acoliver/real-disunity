//! What the pyramid game's tests expect and how they read it back: the texels every mip holds
//! after each frame's passes, computed on the CPU, and the readback of one mip of a texture.
use super::fixture::Rig;
use worldrunner_render_scene::wgpu;

pub const PYRAMID: &str = "pyramid";
/// Four texels, one per mip, that the digest pass fills from the whole chain.
pub const DIGEST: &str = "pyramid digest";
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Float;
pub const MIPS: u32 = 4;
/// The layer of the two the game builds its pyramid in; the other is never written.
pub const LAYER: u32 = 1;
pub const EXTENT: [u32; 2] = [16, 16];
/// The texel of every mip the digest pass reads.
pub const PROBE: [usize; 2] = [1, 1];
/// Rows of a readback copy are padded to this many bytes.
const ROW: u32 = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reduce {
    Average,
    Minimum,
}

/// The texels of every mip as the pyramid's passes compute them, mip by mip in pass order.
pub struct Model(pub [Vec<f32>; MIPS as usize]);

/// The width of `mip` in texels.
fn side(mip: usize) -> usize {
    EXTENT[0] as usize >> mip
}

/// A texel coordinate as a float, exactly: every coordinate here is below 2^16.
fn exact(value: usize) -> f32 {
    f32::from(u16::try_from(value).expect("a small texel coordinate"))
}

/// Mip 0 as the seed pass writes it: the base plus the column and 16 times the row.
fn seeded(base: f32, width: usize) -> Vec<f32> {
    (0..width * width)
        .map(|at| base + exact(at % width) + 16.0 * exact(at / width))
        .collect()
}

/// A mip of `width` texels as a reduction pass writes it from the mip above.
fn reduced(source: &[f32], width: usize, reduce: Reduce) -> Vec<f32> {
    let wide = width * 2;
    (0..width * width)
        .map(|at| {
            let first = 2 * (at / width) * wide + 2 * (at % width);
            let block = [
                source[first],
                source[first + 1],
                source[first + wide],
                source[first + wide + 1],
            ];
            match reduce {
                Reduce::Average => block.iter().sum::<f32>() * 0.25,
                Reduce::Minimum => block.into_iter().fold(f32::INFINITY, f32::min),
            }
        })
        .collect()
}

impl Model {
    pub fn new() -> Self {
        Self(std::array::from_fn(|mip| vec![0.0; side(mip).pow(2)]))
    }

    /// Applies one frame the way its passes do, in order.
    pub fn write(&mut self, mips: &[u32], base: f32, reduce: Reduce) {
        for &mip in mips {
            let mip = mip as usize;
            self.0[mip] = if mip == 0 {
                seeded(base, side(0))
            } else {
                reduced(&self.0[mip - 1], side(mip), reduce)
            };
        }
    }

    /// What the digest pass reads: the probe texel of every mip.
    pub fn digest(&self) -> Vec<f32> {
        self.0
            .iter()
            .enumerate()
            .map(|(mip, texels)| texels[PROBE[1] * side(mip) + PROBE[0]])
            .collect()
    }
}

/// The texels of `mip` of `layer` of `texture` after every submitted frame completed.
pub fn read_mip(rig: &Rig, texture: &wgpu::Texture, (layer, mip): (u32, u32)) -> Vec<f32> {
    let device = rig.context.device();
    let size = [
        (texture.width() >> mip).max(1),
        (texture.height() >> mip).max(1),
    ];
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pyramid readback"),
        size: u64::from(ROW * size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: mip,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW),
                rows_per_image: Some(size[1]),
            },
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    rig.context
        .submission()
        .submit_commands([encoder.finish()])
        .unwrap();
    let bytes = rig.read_buffer(&staging);
    let bytes = &bytes;
    (0..size[1] as usize)
        .flat_map(|row| {
            (0..size[0] as usize).map(move |column| {
                let at = row * ROW as usize + column * 4;
                f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
            })
        })
        .collect()
}

/// Checks every mip of the pyramid against the model.
pub fn assert_pyramid(rig: &Rig, model: &Model, context: &str) {
    let texture = rig.renderer.game_targets().texture(PYRAMID).unwrap();
    assert_eq!(texture.mip_level_count(), MIPS, "{context}");
    for mip in 0..MIPS {
        assert_eq!(
            read_mip(rig, &texture, (LAYER, mip)),
            model.0[mip as usize],
            "{context}: mip {mip}"
        );
        let untouched = read_mip(rig, &texture, (0, mip));
        assert!(
            untouched.iter().all(|texel| *texel == 0.0),
            "{context}: mip {mip} of the other layer"
        );
    }
}

/// Checks the digest target against the probe texel of every mip of the model.
pub fn assert_digest(rig: &Rig, model: &Model, context: &str) {
    let texture = rig.renderer.game_targets().texture(DIGEST).unwrap();
    assert_eq!(read_mip(rig, &texture, (0, 0)), model.digest(), "{context}");
}
