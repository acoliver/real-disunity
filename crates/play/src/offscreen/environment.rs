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
//!   `unity_SHC` are the ambient light as [`spherical_harmonics`] packs it.
//!
//! A name the layer already holds keeps its value: the globals the world holds and the values the
//! host supplies come first, and a global a script sets later replaces what this adds, as it does
//! for the other built-ins.
use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::environment::{
    Ambient, AmbientMode, Environment, PROBE_COEFFICIENTS, Reflection,
};
use worldrunner_world_simulation::graphics::{
    PropertyValue as WorldValue, TextureRef, TextureSlot,
};

use super::builtins::{SH_AB, SH_AG, SH_AR, SH_BB, SH_BG, SH_BR, SH_C, SPEC_CUBE0_HDR};
use super::properties::{Textures, set_value};

/// The reflection cube a draw samples.
pub const SPEC_CUBE0: &str = "unity_SpecCube0";

/// The vectors of `unity_SHAr`, `unity_SHAg`, `unity_SHAb`, `unity_SHBr`, `unity_SHBg`,
/// `unity_SHBb` and `unity_SHC`, in that order.
pub type PackedHarmonics = [[f32; 4]; 7];

/// Adds the scene's ambient light and reflection to `layer`, for each name the layer holds no
/// value for. The reflection's cube is resolved through `textures`.
pub fn add_environment(
    layer: &mut PropertyLayer,
    environment: &Environment,
    textures: &mut Textures<'_>,
) {
    if let Some(ambient) = &environment.ambient {
        add_ambient_light(layer, ambient);
    }
    if let Some(reflection) = &environment.reflection {
        add_reflection(layer, reflection, textures);
    }
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

fn add_reflection(layer: &mut PropertyLayer, reflection: &Reflection, textures: &mut Textures<'_>) {
    if layer.values.get(SPEC_CUBE0_HDR).is_none() {
        layer.values.set(
            SPEC_CUBE0_HDR,
            PropertyValue::Vector([reflection.intensity, 1.0, 0.0, 0.0]),
        );
    }
    let held = layer.textures.contains_key(SPEC_CUBE0) || layer.stand_ins.contains_key(SPEC_CUBE0);
    let source = reflection.source();
    if held || *source == TextureRef::None {
        return;
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
}

/// The first coefficient of a channel a uniform radiance of 1 has: the basis function of band 0.
fn band_zero() -> f32 {
    0.5 / std::f32::consts::PI.sqrt()
}

/// The basis function of band 1 along an axis.
fn band_one() -> f32 {
    (3.0 / (4.0 * std::f32::consts::PI)).sqrt()
}

/// The radiance coefficients of the ambient light of `ambient`, nine for each of red, green and
/// blue, with the intensity applied.
///
/// A probe (the skybox and custom modes) is used as it is. The flat mode is a uniform radiance of
/// its colour. The trilight mode is the sky above, the ground below and the equator between: the
/// radiance runs linearly from the equator colour to the sky colour over the upper hemisphere and
/// to the ground colour over the lower one, and its band 0 and the band 1 term along the up axis
/// are what is kept; the bands above are left out.
fn radiance(ambient: &Ambient) -> [f32; PROBE_COEFFICIENTS] {
    let mut coefficients = [0.0; PROBE_COEFFICIENTS];
    let tau = 2.0 * std::f32::consts::PI;
    for channel in 0..3 {
        let own = &mut coefficients[channel * 9..channel * 9 + 9];
        let [sky, equator, ground, flat] =
            [ambient.sky, ambient.equator, ambient.ground, ambient.flat].map(|rgb| rgb[channel]);
        match ambient.mode {
            AmbientMode::Skybox | AmbientMode::Custom => {
                own.copy_from_slice(&ambient.probe[channel * 9..channel * 9 + 9]);
            }
            AmbientMode::Flat => own[0] = flat / band_zero(),
            AmbientMode::Trilight => {
                own[0] = tau * band_zero() * (equator + f32::midpoint(sky, ground));
                own[1] = tau * band_one() * (sky - ground) / 3.0;
            }
        }
    }
    coefficients.map(|coefficient| coefficient * ambient.intensity)
}

/// The ambient light of `ambient` as the seven vectors a shader reads, in the packing the
/// default ambient uses: the fourth component of `unity_SHA*` is the constant term.
///
/// For a surface that faces `n = (x, y, z)` a shader adds `dot(unity_SHA*.xyz, n) +
/// unity_SHA*.w`, then `dot(unity_SHB*, (x * y, y * z, z * z, z * x))`, then
/// `unity_SHC[channel] * (x * x - y * y)`. With radiance coefficients `L` in the real spherical
/// harmonics `(Y00, Y1-1, Y10, Y11, Y2-2, Y2-1, Y20, Y21, Y22)`, ordered `y, z, x` in band 1
/// and with no sign on `x`, the irradiance over pi of that surface is the sum of every band's
/// cosine lobe (1, 2/3, 1/4) times its terms. Expanding the sum in the monomials gives:
///
/// * `A.x, A.y, A.z = 2/3 * Y1 * (L11, L1-1, L10)`
/// * `A.w = Y00 * L00 - 1/4 * Y20 * L20`
/// * `B = 1/4 * (Y2xy * L2-2, Y2xy * L2-1, 3 * Y20 * L20, Y2xy * L21)`
/// * `C = 1/4 * Y22 * L22`
///
/// where `Y2xy` is the basis constant of the `x * y`, `y * z` and `z * x` terms. The repository
/// has no measurement of the sign or order convention of the player's own probes; this is the
/// conventional real basis, so a probe with the opposite sign on band 1 is lit from the other
/// side.
#[must_use]
pub fn spherical_harmonics(ambient: &Ambient) -> PackedHarmonics {
    let pi = std::f32::consts::PI;
    let (band_one_lobe, band_two_lobe) = (2.0 / 3.0, 0.25);
    let cross = 0.5 * (15.0 / pi).sqrt();
    let axial = 0.25 * (5.0 / pi).sqrt();
    let difference = 0.25 * (15.0 / pi).sqrt();
    let radiance = radiance(ambient);
    let mut packed = [[0.0; 4]; 7];
    for channel in 0..3 {
        let l = &radiance[channel * 9..channel * 9 + 9];
        let linear = band_one_lobe * band_one();
        packed[channel] = [
            linear * l[3],
            linear * l[1],
            linear * l[2],
            band_zero() * l[0] - band_two_lobe * axial * l[6],
        ];
        packed[3 + channel] = [
            band_two_lobe * cross * l[4],
            band_two_lobe * cross * l[5],
            3.0 * band_two_lobe * axial * l[6],
            band_two_lobe * cross * l[7],
        ];
        packed[6][channel] = band_two_lobe * difference * l[8];
    }
    packed
}
