//! The values the host derives from the scene's environment, checked without a device: the
//! spherical harmonics of the ambient light, the decode of the reflection cube and which names a
//! layer that already holds them keeps.
//!
//! The harmonics are checked against an evaluation that shares no code with the packing: the
//! irradiance of a surface facing `n`, divided by pi, from the radiance coefficients of the probe
//! and the cosine lobe of each band, which is what the shader's three terms (a linear one in
//! `unity_SHA*`, a quadratic one in `unity_SHB*` and the `x * x - y * y` one in `unity_SHC`)
//! have to add up to.

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_render_program::layers::PropertyLayer;
use worldrunner_render_program::properties::PropertyValue;
use worldrunner_world_simulation::graphics::environment::{
    Ambient, AmbientMode, Environment, PROBE_COEFFICIENTS, Reflection, ReflectionMode,
};
use worldrunner_world_simulation::graphics::{RenderTextureId, TextureRef};

use super::super::properties::Textures;
use crate::offscreen::{
    SH_AB, SH_AG, SH_AR, SH_BB, SH_BG, SH_BR, SH_C, SPEC_CUBE0, SPEC_CUBE0_HDR, add_environment,
};

const BAND_ONE_WEIGHT: f32 = 2.0 / 3.0;
const BAND_TWO_WEIGHT: f32 = 0.25;

/// The directions the packing is read along, none of them along an axis.
const DIRECTIONS: [[f32; 3]; 6] = [
    [0.6, 0.0, 0.8],
    [-0.36, 0.48, 0.8],
    [0.0, -1.0, 0.0],
    [1.0, 0.0, 0.0],
    [0.48, -0.6, -0.64],
    [-0.8, 0.36, 0.48],
];

/// A probe whose 27 coefficients are all different and exercise each of the nine terms of every
/// channel.
fn probe() -> [f32; PROBE_COEFFICIENTS] {
    std::array::from_fn(|index| {
        let step = f32::from(u8::try_from(index).unwrap());
        0.35 * (step * 1.7).sin() + 0.1 * step / 27.0
    })
}

fn environment(ambient: Ambient) -> Environment {
    Environment {
        ambient: Some(ambient),
        reflection: None,
    }
}

fn vector(layer: &PropertyLayer, name: &str) -> [f32; 4] {
    match layer.values.get(name) {
        Some(PropertyValue::Vector(value)) => *value,
        other => panic!("{name} is {other:?}"),
    }
}

/// The layer a frame with `environment` starts from, over no stored or render textures.
fn layer_of(environment: &Environment) -> PropertyLayer {
    let mut layer = PropertyLayer::default();
    fill(&mut layer, environment);
    layer
}

fn fill(layer: &mut PropertyLayer, environment: &Environment) {
    let render = BTreeMap::new();
    let stored = BTreeSet::from([1_u64 << 40]);
    let mut textures = Textures::new(&render, &stored);
    add_environment(layer, environment, &mut textures);
}

/// The irradiance over pi that the radiance coefficients `l` (the nine of one channel) give a
/// surface that faces `n`, by the real spherical harmonics and the cosine lobe of each band.
fn irradiance(l: &[f32], n: [f32; 3]) -> f32 {
    let [x, y, z] = n;
    let pi = std::f32::consts::PI;
    let band_zero = 0.5 / pi.sqrt();
    let band_one = (3.0 / (4.0 * pi)).sqrt();
    let cross = 0.5 * (15.0 / pi).sqrt();
    let axial = 0.25 * (5.0 / pi).sqrt();
    let difference = 0.25 * (15.0 / pi).sqrt();
    band_zero * l[0]
        + BAND_ONE_WEIGHT * band_one * (l[1] * y + l[2] * z + l[3] * x)
        + BAND_TWO_WEIGHT
            * (cross * (l[4] * x * y + l[5] * y * z + l[7] * x * z)
                + axial * l[6] * (3.0 * z * z - 1.0)
                + difference * l[8] * (x * x - y * y))
}

/// What the three terms of the shader add up to for a channel with the vectors `a` (`unity_SHA*`),
/// `b` (`unity_SHB*`) and the `c` component of `unity_SHC`, for a surface that faces `n`.
fn shaded(a: [f32; 4], b: [f32; 4], c: f32, n: [f32; 3]) -> f32 {
    let [x, y, z] = n;
    a[0] * x
        + a[1] * y
        + a[2] * z
        + a[3]
        + b[0] * x * y
        + b[1] * y * z
        + b[2] * z * z
        + b[3] * z * x
        + c * (x * x - y * y)
}

fn assert_near(found: f32, expected: f32, what: &str) {
    assert!(
        (found - expected).abs() <= 1.0e-5,
        "{what}: {found} against {expected}"
    );
}

fn skybox_probe(probe: [f32; PROBE_COEFFICIENTS], intensity: f32) -> Ambient {
    Ambient {
        mode: AmbientMode::Skybox,
        probe,
        intensity,
        ..Ambient::default()
    }
}

/// Checks every channel of the layer against the evaluation of `radiance`.
fn assert_packs(layer: &PropertyLayer, radiance: &[f32; PROBE_COEFFICIENTS]) {
    let a = [SH_AR, SH_AG, SH_AB].map(|name| vector(layer, name));
    let b = [SH_BR, SH_BG, SH_BB].map(|name| vector(layer, name));
    let c = vector(layer, SH_C);
    for n in DIRECTIONS {
        for channel in 0..3 {
            let coefficients = &radiance[channel * 9..channel * 9 + 9];
            assert_near(
                shaded(a[channel], b[channel], c[channel], n),
                irradiance(coefficients, n),
                &format!("channel {channel} along {n:?}"),
            );
        }
    }
}

#[test]
fn a_probe_packs_into_the_vectors_that_add_up_to_its_irradiance() {
    let probe = probe();
    let layer = layer_of(&environment(skybox_probe(probe, 1.0)));
    assert_packs(&layer, &probe);
}

#[test]
fn a_custom_probe_packs_as_a_skybox_probe_does() {
    let probe = probe();
    let custom = Ambient {
        mode: AmbientMode::Custom,
        ..skybox_probe(probe, 1.0)
    };
    assert_packs(&layer_of(&environment(custom)), &probe);
}

#[test]
fn the_ambient_intensity_scales_every_coefficient() {
    let probe = probe();
    let layer = layer_of(&environment(skybox_probe(probe, 2.5)));
    assert_packs(&layer, &probe.map(|coefficient| coefficient * 2.5));
}

#[test]
fn the_fourth_component_of_the_constant_term_is_the_uniform_radiance_of_a_flat_probe() {
    // A uniform radiance of 0.5 in red is a coefficient of 0.5 / 0.282095 in the first term.
    let mut probe = [0.0; PROBE_COEFFICIENTS];
    probe[0] = 0.5 / 0.282_094_8;
    let layer = layer_of(&environment(skybox_probe(probe, 1.0)));
    let red = vector(&layer, SH_AR);
    assert_near(red[3], 0.5, "constant term");
    assert_eq!(red[..3], [0.0; 3]);
    assert_eq!(vector(&layer, SH_BR), [0.0; 4]);
    assert_eq!(vector(&layer, SH_C), [0.0; 4]);
    assert_eq!(vector(&layer, SH_AG)[3], 0.0);
}

#[test]
fn a_flat_ambient_is_its_colour_in_every_direction() {
    let flat = Ambient {
        mode: AmbientMode::Flat,
        flat: [0.2, 0.4, 0.6, 1.0],
        intensity: 1.5,
        probe: probe(),
        ..Ambient::default()
    };
    let layer = layer_of(&environment(flat));
    for (name, colour) in [(SH_AR, 0.3), (SH_AG, 0.6), (SH_AB, 0.9)] {
        let value = vector(&layer, name);
        assert_near(value[3], colour, name);
        assert_eq!(value[..3], [0.0; 3]);
    }
    for n in DIRECTIONS {
        let red = shaded(
            vector(&layer, SH_AR),
            vector(&layer, SH_BR),
            vector(&layer, SH_C)[0],
            n,
        );
        assert_near(red, 0.3, "red");
    }
}

#[test]
fn a_trilight_ambient_is_the_sky_above_the_ground_below_and_the_equator_between() {
    let trilight = Ambient {
        mode: AmbientMode::Trilight,
        sky: [0.9, 0.9, 0.9, 1.0],
        equator: [0.5, 0.5, 0.5, 1.0],
        ground: [0.1, 0.1, 0.1, 1.0],
        intensity: 1.0,
        ..Ambient::default()
    };
    let layer = layer_of(&environment(trilight));
    let red = |n: [f32; 3]| {
        shaded(
            vector(&layer, SH_AR),
            vector(&layer, SH_BR),
            vector(&layer, SH_C)[0],
            n,
        )
    };
    let (up, side, down) = (
        red([0.0, 1.0, 0.0]),
        red([1.0, 0.0, 0.0]),
        red([0.0, -1.0, 0.0]),
    );
    assert!(up > side && side > down, "{up} {side} {down}");
    // The two bands are symmetric about the equator, so the sides read the same.
    assert_near(side, red([0.0, 0.0, -1.0]), "side");
    assert_near(up - side, side - down, "gradient");
    // The mean of a gradient that runs from 0.1 to 0.9 through 0.5 is 0.5.
    assert_near(vector(&layer, SH_AR)[3], 0.5, "mean");
}

#[test]
fn a_name_the_layer_already_holds_keeps_its_value() {
    let mut layer = PropertyLayer::default();
    layer
        .values
        .set(SH_AR, PropertyValue::Vector([9.0, 8.0, 7.0, 6.0]));
    layer
        .values
        .set(SPEC_CUBE0_HDR, PropertyValue::Vector([5.0; 4]));
    fill(&mut layer, &environment(skybox_probe(probe(), 1.0)));
    assert_eq!(vector(&layer, SH_AR), [9.0, 8.0, 7.0, 6.0]);
    assert_ne!(vector(&layer, SH_AG), [0.0; 4]);
    let reflecting = Environment {
        ambient: None,
        reflection: Some(Reflection::default()),
    };
    fill(&mut layer, &reflecting);
    assert_eq!(vector(&layer, SPEC_CUBE0_HDR), [5.0; 4]);
}

#[test]
fn the_decode_of_the_reflection_cube_scales_by_the_reflection_intensity() {
    for intensity in [0.0, 0.25, 1.0] {
        let reflecting = Environment {
            ambient: None,
            reflection: Some(Reflection {
                intensity,
                ..Reflection::default()
            }),
        };
        let layer = layer_of(&reflecting);
        assert_eq!(
            vector(&layer, SPEC_CUBE0_HDR),
            [intensity, 1.0, 0.0, 0.0],
            "{intensity}"
        );
    }
}

#[test]
fn a_cube_the_frame_can_read_is_bound_and_one_it_cannot_is_left_to_the_grey_default() {
    let reading = |identity: u64| Environment {
        ambient: None,
        reflection: Some(Reflection {
            mode: ReflectionMode::Custom,
            custom: TextureRef::Asset(identity),
            ..Reflection::default()
        }),
    };
    assert!(
        layer_of(&reading(1 << 40))
            .textures
            .contains_key(SPEC_CUBE0)
    );
    let missing = layer_of(&reading((1 << 40) + 7));
    assert!(!missing.textures.contains_key(SPEC_CUBE0));
    assert!(missing.stand_ins.contains_key(SPEC_CUBE0));
}

#[test]
fn a_render_texture_of_the_world_that_is_gone_binds_nothing() {
    let gone = TextureRef::Render(RenderTextureId::from_bits(77));
    let reflecting = Environment {
        ambient: None,
        reflection: Some(Reflection {
            mode: ReflectionMode::Custom,
            custom: gone,
            ..Reflection::default()
        }),
    };
    assert!(!layer_of(&reflecting).textures.contains_key(SPEC_CUBE0));
}

#[test]
fn an_environment_nobody_wrote_adds_nothing() {
    assert_eq!(layer_of(&Environment::default()), PropertyLayer::default());
}
