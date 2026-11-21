//! The scene's ambient probe against the lighting it describes, read the way the game's shaders
//! read the packed values.
//!
//! # What the game's shaders evaluate
//!
//! `Mohawk/Terrain/TerrainShader` (`resources.assets`, object 6707) lists seven four-float members
//! of its per-draw buffer: `unity_SHAr` at byte offset 432, `unity_SHAg` 448, `unity_SHAb` 464,
//! `unity_SHBr` 480, `unity_SHBg` 496, `unity_SHBb` 512 and `unity_SHC` 528. The fragment stage
//! of its first variant (`unity-import player-shader-modules`, the entry point of execution model
//! 4) evaluates, with `N` the normalised world normal and `U` the buffer:
//!
//! * `dot(U[432 + 16 * c], (N.x, N.y, N.z, 1.0))` for the channels `c` = 0, 1, 2,
//! * `dot(U[480 + 16 * c], (N.x * N.y, N.y * N.z, N.z * N.z, N.z * N.x))`,
//! * `U[528].xyz * (N.x * N.x - N.y * N.y)`.
//!
//! The three sums are the ambient light of a surface that faces `N`; [`shaded`] is that
//! expression. No scale, clamp or constant appears between the buffer and the sum. The fragment
//! stage of `Custom/Lava` (object 6728) has the same three terms at the same offsets.
//!
//! # What a stored probe holds
//!
//! `Old World`'s `level0` stores a 27-float ambient probe and the 512 square cube
//! `ReflectionProbe-1`, which is the scene's sky baked to six faces of `BC6H`. Projecting that cube
//! on the real spherical harmonics (`1`, `y`, `z`, `x`, `x * y`, `y * z`, `3 * z * z - 1`,
//! `x * z`, `x * x - y * y`, each with its textbook constant, the faces in the order +X, -X, +Y,
//! -Y, +Z, -Z with the usual axes of a cube face and the first row on top) and dividing the stored
//! coefficients by the projection gives the same ratio for red, green and blue to within 1.5%
//! (the `3 * z * z - 1` term, stored as 0.011, reads 0.79 to 0.87 and is left out of the table):
//!
//! | term                      | stored / projected radiance | stored / (constant * lobe) |
//! |---------------------------|-----------------------------|----------------------------|
//! | constant                  | 0.286                       | 1.01                       |
//! | `y`, `z`, `x`             | 0.295, 0.306, 0.307         | 0.91, 0.94, 0.94           |
//! | `x * y`, `y * z`, `x * z` | 0.217, 0.216, 0.219         | 0.80                       |
//! | `x * x - y * y`           | 0.111                       | 0.81                       |
//!
//! The basis constants are 0.282 for the constant term, 0.489 for band 1 and 1.093, 0.546 and
//! 0.315 for band 2; the cosine lobes are 1, 2/3 and 1/4. So the stored numbers are already the
//! coefficients of the polynomial above for the irradiance over pi: the basis constants and the
//! convolution with the cosine are in them, the stored order is constant, `y`, `z`, `x`, `x * y`,
//! `y * z`, `3 * z * z - 1`, `x * z`, `x * x - y * y`, and every sign is positive (the bright face
//! of the baked sky is -X and the stored `x` term is negative; the sky is bright above and the
//! stored `y` term is positive). The 0.8 to 0.94 left over is the baking's own smoothing and the
//! HDR peak of the sun; it is the same for all three channels, so it is not a convention.
//!
//! # What the tests check
//!
//! A radiance that is a sum of those polynomials `L = sum a_i * f_i` has, by the cosine lobes of
//! the bands, the stored probe `s_i = w_i * a_i` with `w` = 1, 2/3 and 1/4 for the bands. The
//! tests build that probe, pack it and read the packed vectors with [`shaded`], and compare with
//! the irradiance over pi that a numeric integral of `L * max(0, N . w)` over the sphere gives. The
//! integral shares nothing with the packing.

use std::f64::consts::PI;

use super::environment_values::{DIRECTIONS, environment, layer_of, shaded, skybox_probe, vector};
use crate::offscreen::{SH_AB, SH_AG, SH_AR, SH_BB, SH_BG, SH_BR, SH_C};

/// The polynomials of the nine real spherical harmonics, in the order a probe stores them.
const POLYNOMIALS: [fn([f64; 3]) -> f64; 9] = [
    |_| 1.0,
    |[_, y, _]| y,
    |[_, _, z]| z,
    |[x, _, _]| x,
    |[x, y, _]| x * y,
    |[_, y, z]| y * z,
    |[_, _, z]| 3.0 * z * z - 1.0,
    |[x, _, z]| x * z,
    |[x, y, _]| x * x - y * y,
];

/// The weight of the cosine lobe of the band of each term: 1, 2/3 and 1/4.
const LOBES: [f32; 9] = [
    1.0,
    2.0 / 3.0,
    2.0 / 3.0,
    2.0 / 3.0,
    0.25,
    0.25,
    0.25,
    0.25,
    0.25,
];

/// The rings and the steps of a ring of the numeric integral over the sphere.
const RINGS: u16 = 400;
const STEPS: u16 = 800;

/// The integral of `integrand` over the unit sphere, by the midpoint rule in the height and the
/// angle around the up axis, which weighs every cell the same.
fn over_sphere(integrand: &dyn Fn([f64; 3]) -> f64) -> f64 {
    let mut sum = 0.0;
    for ring in 0..RINGS {
        let height = -1.0 + (f64::from(ring) + 0.5) * 2.0 / f64::from(RINGS);
        let radius = (1.0 - height * height).sqrt();
        for step in 0..STEPS {
            let angle = (f64::from(step) + 0.5) * 2.0 * PI / f64::from(STEPS);
            sum += integrand([radius * angle.cos(), radius * angle.sin(), height]);
        }
    }
    sum * (2.0 / f64::from(RINGS)) * (2.0 * PI / f64::from(STEPS))
}

/// The radiance of the world along `direction`: `sum a_i * f_i`.
fn radiance(a: &[f32], direction: [f64; 3]) -> f64 {
    POLYNOMIALS
        .iter()
        .zip(a)
        .map(|(polynomial, coefficient)| f64::from(*coefficient) * polynomial(direction))
        .sum()
}

/// The irradiance over pi of a surface that faces `normal` under the radiance `a`.
fn lit(a: &[f32], normal: [f32; 3]) -> f64 {
    let [nx, ny, nz] = normal.map(f64::from);
    over_sphere(&|w| radiance(a, w) * (nx * w[0] + ny * w[1] + nz * w[2]).max(0.0)) / PI
}

/// The probe of the three channels of radiance `radiances`, as the scene stores it.
fn stored(radiances: &[[f32; 9]; 3]) -> [f32; 27] {
    std::array::from_fn(|index| LOBES[index % 9] * radiances[index / 9][index % 9])
}

/// Packs `radiances` and reads the packed vectors back at each of the directions, against the
/// numeric integral.
fn assert_lit(radiances: &[[f32; 9]; 3]) {
    let ambient = skybox_probe(stored(radiances), 1.0);
    let layer = layer_of(&environment(ambient));
    let a = [SH_AR, SH_AG, SH_AB].map(|name| vector(&layer, name));
    let b = [SH_BR, SH_BG, SH_BB].map(|name| vector(&layer, name));
    let c = vector(&layer, SH_C);
    for n in DIRECTIONS {
        for channel in 0..3 {
            let found = f64::from(shaded(a[channel], b[channel], c[channel], n));
            let expected = lit(&radiances[channel], n);
            assert!(
                (found - expected).abs() < 2.0e-3,
                "channel {channel} along {n:?}: the shader reads {found}, the light is {expected}"
            );
        }
    }
}

#[test]
fn a_uniform_ambient_colour_lights_every_direction_with_that_colour() {
    let mut radiances = [[0.0; 9]; 3];
    for (channel, colour) in [0.55, 0.3, 0.8].into_iter().enumerate() {
        radiances[channel][0] = colour;
    }
    assert_lit(&radiances);
}

#[test]
fn a_directional_lobe_lights_the_side_it_comes_from() {
    // The radiance is a colour plus a lobe along `d`: the polynomials of band 1 are `y`, `z`
    // and `x`, so `d . w` has the coefficients `(d.y, d.z, d.x)` in the stored order.
    let lobe = |colour: f32, strength: f32, d: [f32; 3]| {
        let mut a = [0.0; 9];
        a[0] = colour;
        a[1] = strength * d[1];
        a[2] = strength * d[2];
        a[3] = strength * d[0];
        a
    };
    assert_lit(&[
        lobe(0.5, 0.4, [0.0, 1.0, 0.0]),
        lobe(0.5, 0.4, [-0.6, 0.0, 0.8]),
        lobe(0.6, 0.5, [0.48, -0.6, -0.64]),
    ]);
}

#[test]
fn each_term_of_the_second_band_lights_by_its_own_polynomial() {
    let mut radiances = [[0.0; 9]; 3];
    for (index, channel) in (4..9).zip([0, 1, 2, 0, 1]) {
        radiances[channel][index] = 0.2 + 0.05 * f32::from(u8::try_from(index).unwrap());
    }
    assert_lit(&radiances);
}

#[test]
fn a_probe_of_every_term_lights_as_the_integral_of_its_radiance_does() {
    let radiances = std::array::from_fn(|channel| {
        std::array::from_fn(|term| {
            let step = f32::from(u8::try_from(channel * 9 + term).unwrap());
            0.3 * (step * 1.3).sin() + 0.02 * step
        })
    });
    assert_lit(&radiances);
}
