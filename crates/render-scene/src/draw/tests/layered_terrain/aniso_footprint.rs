//! The anisotropic layered draw computes its mip level in the shader (`footprint.wgsl`) and hands
//! it to `textureSampleLevel`, so the sampler sees a fractional LOD. A sampler may keep only some
//! fractional bits and truncate, so an LOD a hair below an integer level selects that level's
//! predecessor by 2^-6 or more on such hardware. This test recomputes the LOD at the centre pixel
//! on the CPU, in f64, as the shader does, and requires it to sit just above the asserted level.
//!
//! Upper edge: with linear mip filtering a fraction `t` of the next level moves a channel by at
//! most `t * (230 - 40) / 255 = 0.745 t`. The half float target adds at most 2^-11 = 0.0005, and
//! the assertion bound is 0.005. For `t <= 2^-8` the sum is 0.0029 + 0.0005 = 0.0034, which leaves
//! 0.0016 inside the bound. Any sampler with at most eight fractional bits has 2^-8 on its grid,
//! so rounding an LOD of at most 2^-8 above the level never exceeds `t = 2^-8`.
//!
//! Lower edge: 2^-10 above the level covers the shader's f32 evaluation of the same expression
//! (about 1e-5) and keeps an exact, rounding or truncating sampler on the asserted level.
//! Projection jitter and the camera tilt only enlarge the footprint, so they cannot lower it.
use super::super::mip_rig::RENDER;
use super::super::terrain_mip_bias::frame_at;
use super::mip_bias::aniso_height_for;
use glam::{DMat4, DVec2, DVec3};

const TEXTURE_SIZE: f64 = 64.0;
const REPEAT_METERS: f64 = 10.0;
/// The `anisotropy` argument of the anisotropic fixture, `4`, allows up to four taps.
const TAP_LIMIT: f64 = 4.0;
const ABOVE_MIN: f64 = 1.0 / 1024.0;
const ABOVE_MAX: f64 = 1.0 / 256.0;

/// The terrain is the plane `z = 0`: the world position under the centre of `pixel`.
fn ground_point(inverse: DMat4, pixel: [u32; 2]) -> DVec2 {
    let ndc = |index: u32| (f64::from(index) + 0.5) / f64::from(RENDER) * 2.0 - 1.0;
    let (x, y) = (ndc(pixel[0]), -ndc(pixel[1]));
    let near = inverse.project_point3(DVec3::new(x, y, 0.0));
    let far = inverse.project_point3(DVec3::new(x, y, 1.0));
    let along = near.z / (near.z - far.z);
    (near + (far - near) * along).truncate()
}

/// The finite differences a 2x2 quad gives the top-left centre pixel, in layer uv units.
fn uv_derivatives(height: i64, gradient_scale: f64) -> [DVec2; 2] {
    let inverse = frame_at(height).view_projection.as_dmat4().inverse();
    let centre = [RENDER / 2; 2];
    let origin = ground_point(inverse, centre);
    let scale = gradient_scale / REPEAT_METERS;
    [
        (ground_point(inverse, [centre[0] + 1, centre[1]]) - origin) * scale,
        (ground_point(inverse, [centre[0], centre[1] + 1]) - origin) * scale,
    ]
}

/// `layered_principal_axes` and `layered_axis_footprint` without their range scaling: the
/// singular values of the Jacobian in texels, then the tap count and the LOD.
fn shader_footprint(dx: DVec2, dy: DVec2) -> (f64, f64) {
    let east = dx * TEXTURE_SIZE;
    let north = dy * TEXTURE_SIZE;
    let a = east.x * east.x + north.x * north.x;
    let b = east.x * east.y + north.x * north.y;
    let c = east.y * east.y + north.y * north.y;
    let eigenvalue = 0.5 * (a + c + (a - c).hypot(2.0 * b));
    let major = eigenvalue.sqrt();
    let minor = (east.x * north.y - east.y * north.x).abs() / major;
    if major <= 1.0 {
        return (1.0, 0.0);
    }
    let count = (major / minor.max(1e-20)).ceil().clamp(1.0, TAP_LIMIT);
    let lod = (major / count).max(minor).max(1e-20).log2();
    (count, lod.max(0.0))
}

/// Plain renderer at levels 2 and 3, then the biased renderer (gradients times 2^-2) which must
/// land two levels finer.
#[test]
fn the_anisotropic_fixture_lod_sits_just_above_each_asserted_level() {
    for level in [2_u32, 3] {
        for (gradient_scale, finer) in [(1.0, 0), (0.25, 2)] {
            let [dx, dy] = uv_derivatives(aniso_height_for(level), gradient_scale);
            let (count, lod) = shader_footprint(dx, dy);
            let asserted = f64::from(level - finer);
            let above = lod - asserted;
            assert!(
                (count - 2.0).abs() < 0.5,
                "level {level} scale {gradient_scale}: {count} taps"
            );
            assert!(
                (ABOVE_MIN..=ABOVE_MAX).contains(&above),
                "level {level} scale {gradient_scale}: lod {lod} is {above:+e} from level {asserted}, \
                 wanted {ABOVE_MIN:e} to {ABOVE_MAX:e}"
            );
        }
    }
}
