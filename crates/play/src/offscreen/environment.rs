//! The scene's environment lighting as the values and the texture a draw reads.
//!
//! The world holds the environment as the scene or a script wrote it (see
//! [`Environment`]); this module turns it into the names a shader reads, in the layer of the
//! globals a frame starts from. A section nobody wrote adds nothing, so the host's own default
//! ambient (see [`add_ambient`](super::builtins::add_ambient)) and the grey cube a draw falls back
//! to stay in place for a world without an environment.
//!
//! # Names
//!
//! * `unity_SpecCube0` is the scene's reflection cube: the custom cube when the reflection mode
//!   is custom and one is set, otherwise the cube baked from the skybox, otherwise no texture
//!   (see [`Reflection::source`]). A cube the frame cannot read is left to the grey default, and
//!   the layer says why.
//! * `unity_SpecCube0_HDR` is `(intensity, 1, 0, 0)`: the reflection intensity scales the cube and
//!   the second term is neutral. The cube is uploaded as the host decodes it, so no further
//!   decode is applied. The repository has no measurement of how the player decodes a probe, which
//!   is the choice the default `(1, 1, 0, 0)` already made.
//! * `unity_SHAr`, `unity_SHAg`, `unity_SHAb`, `unity_SHBr`, `unity_SHBg`, `unity_SHBb` and
//!   `unity_SHC` are the ambient light as [`spherical_harmonics`] packs it. A scene's probe is
//!   the coefficients of the polynomial the shader evaluates, so it reaches the names with no
//!   further constant.
//!
//! A name the layer already holds keeps its value: the globals the world holds and the values the
//! host supplies come first, and a global a script sets later replaces what this adds, as it does
//! for the other built-ins.
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::environment::{
    Ambient, AmbientMode, Environment, PROBE_COEFFICIENTS, Reflection,
};
use worldrunner_world_simulation::graphics::{PropertyValue as WorldValue, TextureSlot};

use super::builtins::{SH_AB, SH_AG, SH_AR, SH_BB, SH_BG, SH_BR, SH_C, SPEC_CUBE0_HDR};
use super::properties::{Textures, set_value};

/// The reflection cube a draw samples.
pub const SPEC_CUBE0: &str = "unity_SpecCube0";

/// The vectors of `unity_SHAr`, `unity_SHAg`, `unity_SHAb`, `unity_SHBr`, `unity_SHBg`,
/// `unity_SHBb` and `unity_SHC`, in that order.
pub type PackedHarmonics = [[f32; 4]; 7];

/// What a draw reads as `unity_SpecCube0` once the scene's reflection was added to the globals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReflectionCube {
    /// The scene wrote no reflection, so a draw reads the grey default cube.
    NotWritten,
    /// The world's globals already name a cube, which the scene's reflection leaves alone.
    Held,
    /// The reflection names no cube, so a draw reads the grey default cube.
    NoCube,
    /// The reflection's cube is uploaded and bound.
    Bound,
    /// The reflection names a cube the frame cannot read, so a draw reads the grey default cube.
    Unreadable,
}

impl ReflectionCube {
    /// The census line of the outcome, which names what the draws of the frame read.
    #[must_use]
    pub const fn census(self) -> &'static str {
        match self {
            Self::NotWritten => "unity_SpecCube0: the scene wrote no reflection, grey default cube",
            Self::Held => "unity_SpecCube0: a cube the world's globals name",
            Self::NoCube => {
                "unity_SpecCube0: the scene's reflection names no cube, grey default cube"
            }
            Self::Bound => "unity_SpecCube0: the scene's reflection cube",
            Self::Unreadable => {
                "unity_SpecCube0: the scene's reflection cube is not readable, grey default cube"
            }
        }
    }
}

/// Adds the scene's ambient light and reflection to `layer`, for each name the layer holds no
/// value for. The reflection's cube is resolved through `textures`. The result says which cube a
/// draw reads as `unity_SpecCube0`.
pub fn add_environment(
    layer: &mut PropertyLayer,
    environment: &Environment,
    textures: &mut Textures<'_>,
) -> ReflectionCube {
    if let Some(ambient) = &environment.ambient {
        add_ambient_light(layer, ambient);
    }
    environment
        .reflection
        .as_ref()
        .map_or(ReflectionCube::NotWritten, |reflection| {
            add_reflection(layer, reflection, textures)
        })
}

fn add_ambient_light(layer: &mut PropertyLayer, ambient: &Ambient) {
    let packed = spherical_harmonics(ambient);
    let names = [SH_AR, SH_AG, SH_AB, SH_BR, SH_BG, SH_BB, SH_C];
    for (name, value) in names.into_iter().zip(packed) {
        if layer.values.get(name).is_none() {
            layer.values.set(name, PropertyValue::Vector(value));
        }
    }
}

fn add_reflection(
    layer: &mut PropertyLayer,
    reflection: &Reflection,
    textures: &mut Textures<'_>,
) -> ReflectionCube {
    if layer.values.get(SPEC_CUBE0_HDR).is_none() {
        layer.values.set(
            SPEC_CUBE0_HDR,
            PropertyValue::Vector([reflection.intensity, 1.0, 0.0, 0.0]),
        );
    }
    let held = layer.textures.contains_key(SPEC_CUBE0) || layer.stand_ins.contains_key(SPEC_CUBE0);
    let source = reflection.source();
    if held {
        return ReflectionCube::Held;
    }
    if source.is_unbound() {
        return ReflectionCube::NoCube;
    }
    let slot = TextureSlot {
        texture: source.clone(),
        ..TextureSlot::default()
    };
    set_value(
        layer,
        &WorldValue::Texture(SPEC_CUBE0.to_owned(), slot),
        textures,
    );
    if layer.textures.contains_key(SPEC_CUBE0) {
        ReflectionCube::Bound
    } else {
        ReflectionCube::Unreadable
    }
}

/// The first coefficient of a channel a uniform radiance of 1 has: the basis function of band 0.
fn band_zero() -> f32 {
    0.5 / std::f32::consts::PI.sqrt()
}

/// The basis function of band 1 along an axis.
fn band_one() -> f32 {
    (3.0 / (4.0 * std::f32::consts::PI)).sqrt()
}

/// The radiance coefficients of the flat and trilight modes, nine for each of red, green and
/// blue, with the intensity applied.
///
/// The flat mode is a uniform radiance of its colour. The trilight mode is the sky above, the
/// ground below and the equator between: the radiance runs linearly from the equator colour to
/// the sky colour over the upper hemisphere and to the ground colour over the lower one, and its
/// band 0 and the band 1 term along the up axis are what is kept; the bands above are left out.
/// A probe is not radiance and has no coefficients here (see [`coefficients`]).
fn radiance(ambient: &Ambient) -> [f32; PROBE_COEFFICIENTS] {
    let mut coefficients = [0.0; PROBE_COEFFICIENTS];
    let tau = 2.0 * std::f32::consts::PI;
    for channel in 0..3 {
        let own = &mut coefficients[channel * 9..channel * 9 + 9];
        let [sky, equator, ground, flat] =
            [ambient.sky, ambient.equator, ambient.ground, ambient.flat].map(|rgb| rgb[channel]);
        match ambient.mode {
            AmbientMode::Flat => own[0] = flat / band_zero(),
            AmbientMode::Trilight => {
                own[0] = tau * band_zero() * (equator + f32::midpoint(sky, ground));
                own[1] = tau * band_one() * (sky - ground) / 3.0;
            }
            AmbientMode::Skybox | AmbientMode::Custom => {}
        }
    }
    coefficients
}

/// The coefficients of the nine terms of one channel that radiance coefficients `l` give: each
/// radiance coefficient times the constant of its basis function and the weight of the cosine
/// lobe of its band (1, 2/3 and 1/4), the form in which a scene stores its probe.
///
/// The constants are the textbook ones of the real spherical harmonics: `y`, `z` and `x` of band
/// 1 share one, `x * y`, `y * z` and `x * z` share another, and `3 * z * z - 1` and `x * x - y *
/// y` have their own.
fn irradiance(l: &[f32]) -> [f32; 9] {
    let pi = std::f32::consts::PI;
    let (lobe_one, lobe_two) = (2.0 / 3.0, 0.25);
    let cross = 0.5 * (15.0 / pi).sqrt();
    let axial = 0.25 * (5.0 / pi).sqrt();
    let difference = 0.25 * (15.0 / pi).sqrt();
    let weights = [
        band_zero(),
        lobe_one * band_one(),
        lobe_one * band_one(),
        lobe_one * band_one(),
        lobe_two * cross,
        lobe_two * cross,
        lobe_two * axial,
        lobe_two * cross,
        lobe_two * difference,
    ];
    std::array::from_fn(|term| weights[term] * l[term])
}

/// The nine coefficients of each of red, green and blue of the polynomial the shader evaluates
/// for the ambient light of `ambient`, in the order `1`, `y`, `z`, `x`, `x * y`, `y * z`,
/// `3 * z * z - 1`, `x * z`, `x * x - y * y`, with the intensity applied.
///
/// A probe (the skybox and custom modes) is the scene's own: it is stored in this form already.
/// The flat and trilight modes are radiance, which [`irradiance`] brings to it.
fn coefficients(ambient: &Ambient) -> [f32; PROBE_COEFFICIENTS] {
    let mut coefficients = match ambient.mode {
        AmbientMode::Skybox | AmbientMode::Custom => ambient.probe,
        AmbientMode::Flat | AmbientMode::Trilight => {
            let radiance = radiance(ambient);
            let mut lit = [0.0; PROBE_COEFFICIENTS];
            for channel in 0..3 {
                let own = irradiance(&radiance[channel * 9..channel * 9 + 9]);
                lit[channel * 9..channel * 9 + 9].copy_from_slice(&own);
            }
            lit
        }
    };
    for coefficient in &mut coefficients {
        *coefficient *= ambient.intensity;
    }
    coefficients
}

/// The ambient light of `ambient` as the seven vectors a shader reads, in the packing the
/// default ambient uses: the fourth component of `unity_SHA*` is the constant term.
///
/// A surface that faces `n = (x, y, z)` reads `dot(unity_SHA*, (x, y, z, 1))` plus
/// `dot(unity_SHB*, (x * y, y * z, z * z, z * x))` plus `unity_SHC[channel] * (x * x - y * y)`
/// (the evaluation of the player's own shaders, with no scale between the values and the sum).
/// With the nine coefficients `s` of a channel (see [`coefficients`]) that reads
/// `s0 + s1 * y + s2 * z + s3 * x + s4 * x * y + s5 * y * z + s6 * (3 * z * z - 1) + s7 * x * z +
/// s8 * (x * x - y * y)`, and collecting the monomials gives:
///
/// * `A.x, A.y, A.z = s3, s1, s2`
/// * `A.w = s0 - s6`
/// * `B = (s4, s5, 3 * s6, s7)`
/// * `C = s8`
///
/// The stored probe of the player's scenes is in that form: its coefficients are those of the
/// real spherical harmonics of the irradiance over pi, ordered `1, y, z, x, ...` with every sign
/// positive, so `y`, `z` and `x` of the scene are the same axes the shader reads. See the tests
/// of `environment_probe` for the measurement.
#[must_use]
pub fn spherical_harmonics(ambient: &Ambient) -> PackedHarmonics {
    let coefficients = coefficients(ambient);
    let mut packed = [[0.0; 4]; 7];
    for channel in 0..3 {
        let s = &coefficients[channel * 9..channel * 9 + 9];
        packed[channel] = [s[3], s[1], s[2], s[0] - s[6]];
        packed[3 + channel] = [s[4], s[5], 3.0 * s[6], s[7]];
        packed[6][channel] = s[8];
    }
    packed
}
