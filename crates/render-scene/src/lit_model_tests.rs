//! Independent f64 model of the lighting of an upward-facing, fully rough dielectric.
//!
//! GPU tests that compare two renders through the same `lighting.wgsl` cannot see an error
//! inside it, because the error reaches both renders. These functions give the ideal radiance
//! from the published terms (Lambert, GGX with `alpha = roughness^2`, height-correlated Smith,
//! Schlick, hemisphere ambient), so a test can check the rendered display code against it.
//! At roughness 1 the GGX distribution is the constant `1 / pi` and the Smith term reduces to
//! `0.5 / (n.l + n.v)`, which keeps the model short enough to audit by eye.

use worldrunner_render_gpu::tolerance_test_support::{assert_code_within, encoded_steps};

pub(crate) const PI: f64 = std::f64::consts::PI;

/// Dielectric normal-incidence reflectance.
const F0: f64 = 0.04;

/// Radiance of an up-facing surface of `albedo`, lit by a sun straight overhead of `sun` and a
/// hemisphere whose upward radiance is `sky`, seen along the unit vector `view` (towards the eye).
pub(crate) fn overhead_rough_radiance(albedo: f64, sun: f64, sky: f64, view: [f64; 3]) -> f64 {
    let no_v = view[2].clamp(1e-4, 1.0);
    let no_l = 1.0;
    // Half vector of view and an overhead light.
    let sum = [view[0], view[1], view[2] + 1.0];
    let length = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
    let view_dot_half =
        ((view[0] * sum[0] + view[1] * sum[1] + view[2] * sum[2]) / length).clamp(0.0, 1.0);
    let fresnel = F0 + (1.0 - F0) * (1.0 - view_dot_half).powi(5);
    let specular = fresnel * (1.0 / PI) * (0.5 / (no_l + no_v));
    let diffuse = (1.0 - fresnel) * albedo / PI;
    (diffuse + specular) * sun * no_l + sky * albedo
}

/// Unit vector from the point where the pixel's ray meets the plane z = 0 back to the eye, for a
/// pinhole camera at `eye` looking at `target` with Z up, a square viewport of `size` pixels and
/// a vertical field of view of `fov_degrees`. Pixel (x, y) counts from the top left.
pub(crate) fn pixel_view_towards_eye(
    eye: [f64; 3],
    target: [f64; 3],
    fov_degrees: f64,
    size: u32,
    pixel: (u32, u32),
) -> [f64; 3] {
    let unit = |v: [f64; 3]| {
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / n, v[1] / n, v[2] / n]
    };
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let forward = unit([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let right = unit(cross(forward, [0.0, 0.0, 1.0]));
    let up = cross(right, forward);
    let scale = (fov_degrees.to_radians() / 2.0).tan();
    let extent = f64::from(size);
    let ndc_x = (f64::from(pixel.0) + 0.5) / extent * 2.0 - 1.0;
    let ndc_y = 1.0 - (f64::from(pixel.1) + 0.5) / extent * 2.0;
    let ray: [f64; 3] =
        std::array::from_fn(|i| forward[i] + ndc_x * scale * right[i] + ndc_y * scale * up[i]);
    let travel = -eye[2] / ray[2];
    let point: [f64; 3] = std::array::from_fn(|i| eye[i] + travel * ray[i]);
    unit(std::array::from_fn(|i| eye[i] - point[i]))
}

/// Asserts a rendered display code is the sRGB code of `radiance * exposure` within the shared
/// output bound. The radiance must stay below 1 so the code is not clamped.
pub(crate) fn assert_display_matches_radiance(code: u8, radiance: f64, context: &str) {
    assert!(
        radiance < 1.0,
        "{context}: model radiance {radiance} saturates"
    );
    assert_code_within(code, encoded_steps(radiance, true), 0.0, context);
}

/// Linear value of an 8-bit sRGB code.
pub(crate) fn srgb_decode(code: u8) -> f64 {
    let value = f64::from(code) / 255.0;
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
