//! Deterministic reference integrals for split-sum GGX image-based lighting.
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

#[expect(
    clippy::cast_precision_loss,
    reason = "bounded sample counts and cube coordinates"
)]
fn hammersley(index: u32, count: u32) -> Vec2 {
    Vec2::new(
        index as f32 / count as f32,
        index.reverse_bits() as f32 * 2.328_306_4e-10,
    )
}
fn half_vector(xi: Vec2, roughness: f32) -> Vec3 {
    let a2 = roughness.powi(4);
    let cosine = ((1.0 - xi.y) / (1.0 + (a2 - 1.0) * xi.y)).sqrt();
    let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
    Vec3::new((TAU * xi.x).cos() * sine, (TAU * xi.x).sin() * sine, cosine)
}
/// Returns coefficients A and B for `F0 * A + B`, not exposed radiance.
#[expect(clippy::cast_precision_loss, reason = "bounded sample count")]
pub fn brdf_integral(no_v: f32, roughness: f32, samples: u32) -> [f32; 2] {
    assert!(samples > 0 && no_v > 0.0 && no_v <= 1.0 && (0.0..=1.0).contains(&roughness));
    let v = Vec3::new((1.0 - no_v * no_v).sqrt(), 0.0, no_v);
    let mut sum = Vec2::ZERO;
    for i in 0..samples {
        let h = half_vector(hammersley(i, samples), roughness);
        let vo_h = v.dot(h).max(0.0);
        let l = 2.0 * vo_h * h - v;
        if l.z > 0.0 {
            let visibility = crate::visibility(
                no_v,
                l.z,
                roughness,
                crate::VisibilityAlgorithm::SmithCorrelated,
            );
            let weight = 4.0 * visibility * l.z * vo_h / h.z.max(1e-7);
            let fc = (1.0 - vo_h).powi(5);
            sum += Vec2::new(1.0 - fc, fc) * weight;
        }
    }
    (sum / samples as f32).to_array()
}
/// Radiance convolution with normalized `NoL` weighting. The sampler receives unit directions.
pub fn prefilter(
    normal: Vec3,
    roughness: f32,
    samples: u32,
    sample: impl Fn(Vec3) -> Vec3,
) -> Vec3 {
    assert!(samples > 0 && (0.0..=1.0).contains(&roughness));
    let seed = if normal.z.abs() < 0.999 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let tangent = seed.cross(normal).normalize();
    let bitangent = normal.cross(tangent);
    let mut sum = Vec3::ZERO;
    let mut weight = 0.0;
    for i in 0..samples {
        let local = half_vector(hammersley(i, samples), roughness);
        let h = tangent * local.x + bitangent * local.y + normal * local.z;
        let l = 2.0 * normal.dot(h) * h - normal;
        let no_l = normal.dot(l).max(0.0);
        if no_l > 0.0 {
            sum += sample(l) * no_l;
            weight += no_l;
        }
    }
    sum / weight
}
/// WebGPU cube-face convention, +X, -X, +Y, -Y, +Z, -Z; coordinates in [-1,1].
pub fn cube_direction(face: u32, uv: [f32; 2]) -> Vec3 {
    let [u, v] = uv;
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        5 => Vec3::new(-u, -v, -1.0),
        _ => panic!("cube face must be in 0..6"),
    }
    .normalize()
}

/// GGX solid-angle PDF, useful for choosing source mip levels during convolution.
pub fn sample_pdf(no_h: f32, vo_h: f32, roughness: f32) -> f32 {
    crate::distribution_ggx(no_h, roughness, 1e-7) * no_h / (4.0 * vo_h).max(1e-7)
}

pub const HEMISPHERE_SOLID_ANGLE: f32 = 2.0 * PI;
