//! CPU mirror of `shader/gbuffer.wgsl`, for tests only. The GPU tests check it against
//! the shader's stored codes, and the CPU tests use it to bound the quantisation.
use glam::{Mat3, Vec2, Vec3};

/// One logical surface, as the WGSL `SceneGbufferSurface`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Surface {
    pub shading_normal: Vec3,
    pub geometric_normal: Vec3,
    pub base_colour: Vec3,
    pub roughness: f32,
    pub metal: f32,
    pub emission: f32,
    pub ao: f32,
    pub flags: u32,
    pub lighting_record: u8,
}

/// Stored codes of one pixel, as the four targets hold them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Codes {
    /// Ten-bit G0 red, green, blue and two-bit alpha.
    pub g0: [u16; 4],
    /// G1 sRGB colour bytes and the lighting-record byte.
    pub g1: [u8; 4],
    /// G2 bytes.
    pub g2: [u8; 4],
    pub g3: u16,
}

impl Codes {
    /// Reads one pixel from the native readback bytes of each target.
    pub(crate) fn from_targets(g0: &[u8], g1: &[u8], g2: &[u8], g3: &[u8], pixel: usize) -> Self {
        let word = u32::from_le_bytes(g0[pixel * 4..pixel * 4 + 4].try_into().unwrap());
        let field = |shift: u32| u16::try_from((word >> shift) & 1023).expect("ten bits");
        Self {
            g0: [field(0), field(10), field(20), field(30)],
            g1: g1[pixel * 4..pixel * 4 + 4].try_into().unwrap(),
            g2: g2[pixel * 4..pixel * 4 + 4].try_into().unwrap(),
            g3: u16::from_le_bytes(g3[pixel * 2..pixel * 2 + 2].try_into().unwrap()),
        }
    }
}

const OCT_MAX: f32 = 127.0;

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0..=max, at most 1023, before it is cast"
)]
fn unorm(value: f32, max: f32) -> u16 {
    (value.clamp(0.0, 1.0) * max + 0.5).floor() as u16
}

fn srgb_encode(linear: f32) -> f32 {
    let linear = linear.clamp(0.0, 1.0);
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn srgb_decode(encoded: f32) -> f32 {
    if encoded <= 0.040_45 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

pub(crate) fn spheremap_encode(normal: Vec3) -> Vec2 {
    let planar = normal.truncate().length();
    let direction = if planar > 0.0 {
        normal.truncate() / planar
    } else {
        Vec2::X
    };
    let radius = ((1.0 - normal.z).max(0.0) * 0.125).sqrt();
    direction * radius + Vec2::splat(0.5)
}

pub(crate) fn spheremap_decode(encoded: Vec2) -> Vec3 {
    let planar = encoded * 4.0 - Vec2::splat(2.0);
    let f = planar.dot(planar).min(4.0);
    let scale = (1.0 - f * 0.25).sqrt();
    (planar * scale).extend(1.0 - f * 0.5)
}

fn sign(value: Vec2) -> Vec2 {
    Vec2::new(
        if value.x >= 0.0 { 1.0 } else { -1.0 },
        if value.y >= 0.0 { 1.0 } else { -1.0 },
    )
}

pub(crate) fn oct_encode(normal: Vec3) -> Vec2 {
    let projected = normal.truncate() / (normal.x.abs() + normal.y.abs() + normal.z.abs());
    if normal.z < 0.0 {
        (Vec2::ONE - Vec2::new(projected.y.abs(), projected.x.abs())) * sign(projected)
    } else {
        projected
    }
}

pub(crate) fn oct_decode(encoded: Vec2) -> Vec3 {
    let z = 1.0 - encoded.x.abs() - encoded.y.abs();
    let fold = (-z).clamp(0.0, 1.0);
    let xy = encoded - fold * sign(encoded);
    xy.extend(z).normalize()
}

pub(crate) fn g3_encode(normal: Vec3, flags: u32) -> u16 {
    let unit = (oct_encode(normal) * 0.5 + Vec2::splat(0.5)).clamp(Vec2::ZERO, Vec2::ONE);
    let mut code = [unorm(unit.x, OCT_MAX), unorm(unit.y, OCT_MAX)];
    let bits = flags & 3;
    let bits = u16::try_from(bits).expect("two bits");
    if code == [0, 0] && bits == 0 {
        code = [127, 127];
    }
    (code[0] << 9) | (code[1] << 2) | bits
}

pub(crate) fn g3_normal(g3: u16) -> Vec3 {
    let code = Vec2::new(f32::from((g3 >> 9) & 127), f32::from((g3 >> 2) & 127));
    oct_decode(code / OCT_MAX * 2.0 - Vec2::ONE)
}

pub(crate) fn encode(surface: &Surface, view_from_scene: Mat3) -> Codes {
    let g0 = spheremap_encode(view_from_scene * surface.shading_normal);
    let byte = |value: f32| u8::try_from(unorm(value, 255.0)).expect("byte");
    let colour = surface.base_colour.to_array().map(srgb_encode);
    Codes {
        g0: [unorm(g0.x, 1023.0), unorm(g0.y, 1023.0), 0, 0],
        g1: [
            byte(colour[0]),
            byte(colour[1]),
            byte(colour[2]),
            surface.lighting_record,
        ],
        g2: [
            byte(surface.roughness),
            byte(surface.metal),
            byte(surface.emission.max(0.0).sqrt()),
            byte(surface.ao),
        ],
        g3: g3_encode(surface.geometric_normal, surface.flags),
    }
}

pub(crate) fn decode(codes: &Codes, scene_from_view: Mat3) -> Surface {
    let g0 = Vec2::new(f32::from(codes.g0[0]), f32::from(codes.g0[1])) / 1023.0;
    let unit = |byte: u8| f32::from(byte) / 255.0;
    let emission = unit(codes.g2[2]);
    Surface {
        shading_normal: (scene_from_view * spheremap_decode(g0)).normalize(),
        geometric_normal: g3_normal(codes.g3),
        base_colour: Vec3::new(
            srgb_decode(unit(codes.g1[0])),
            srgb_decode(unit(codes.g1[1])),
            srgb_decode(unit(codes.g1[2])),
        ),
        roughness: unit(codes.g2[0]),
        metal: unit(codes.g2[1]),
        emission: emission * emission,
        ao: unit(codes.g2[3]),
        flags: u32::from(codes.g3) & 3,
        lighting_record: codes.g1[3],
    }
}

/// Angle between two vectors in degrees.
pub(crate) fn angle_degrees(a: Vec3, b: Vec3) -> f32 {
    a.normalize()
        .dot(b.normalize())
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

/// `count` directions spread evenly over the unit sphere (a Fibonacci lattice).
pub(crate) fn sphere(count: u16) -> Vec<Vec3> {
    let golden = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
    (0..count)
        .map(|index| {
            let z = 1.0 - 2.0 * (f32::from(index) + 0.5) / f32::from(count);
            let radius = (1.0 - z * z).max(0.0).sqrt();
            let angle = golden * f32::from(index);
            Vec3::new(radius * angle.cos(), radius * angle.sin(), z)
        })
        .collect()
}
