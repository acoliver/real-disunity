//! Real SH irradiance in a right-handed Z-up frame.
//!
//! Ramamoorthi and Hanrahan, 2001, An Efficient Representation for Irradiance
//! Environment Maps, <https://graphics.stanford.edu/papers/envmap/>.
use glam::Vec3;
use std::f32::consts::PI;

/// Orthonormal real basis ordered [00, 1-1, 10, 11, 2-2, 2-1, 20, 21, 22].
/// Positive first-order lobes point along +Y, +Z and +X respectively.
pub fn basis(direction: Vec3) -> [f32; 9] {
    let Vec3 { x, y, z } = direction;
    [
        0.282_094_8,
        0.488_602_52 * y,
        0.488_602_52 * z,
        0.488_602_52 * x,
        1.092_548_5 * x * y,
        1.092_548_5 * y * z,
        0.315_391_57 * (3.0 * z * z - 1.0),
        1.092_548_5 * x * z,
        0.546_274_24 * (x * x - y * y),
    ]
}

/// Projects radiance samples with supplied solid-angle weights, in steradians.
/// Convolution applies the cosine-kernel band factors [pi, 2pi/3, pi/4].
/// Samples must have finite nonnegative radiance and unit directions.
pub fn project(samples: impl IntoIterator<Item = (Vec3, Vec3, f32)>) -> [Vec3; 9] {
    let mut coefficients = [Vec3::ZERO; 9];
    for (direction, radiance, solid_angle) in samples {
        assert!(direction.is_finite() && (direction.length_squared() - 1.0).abs() < 1e-4);
        assert!(radiance.is_finite() && radiance.min_element() >= 0.0);
        assert!(solid_angle.is_finite() && solid_angle >= 0.0);
        for (coefficient, value) in coefficients.iter_mut().zip(basis(direction)) {
            *coefficient += radiance * (value * solid_angle);
        }
    }
    for (index, coefficient) in coefficients.iter_mut().enumerate() {
        *coefficient *= match index {
            0 => PI,
            1..=3 => 2.0 * PI / 3.0,
            _ => PI / 4.0,
        };
    }
    coefficients
}

/// Converts normalized irradiance SH to the existing polynomial kernel's basis.
/// The result represents E/pi, so multiplying albedo yields Lambertian radiance.
pub fn diffuse_polynomial(c: &[Vec3; 9]) -> [Vec3; 9] {
    assert!(c.iter().all(|value| value.is_finite()));
    [
        c[0] * 0.282_094_8 - c[6] * 0.315_391_57,
        c[3] * 0.488_602_52,
        c[1] * 0.488_602_52,
        c[2] * 0.488_602_52,
        c[4] * 1.092_548_5,
        c[7] * 1.092_548_5,
        c[5] * 1.092_548_5,
        c[8] * 0.546_274_24,
        c[6] * 0.946_174_7,
    ]
    .map(|value| value / PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(radiance: impl Fn(Vec3) -> Vec3) -> Vec<(Vec3, Vec3, f32)> {
        let mut result = Vec::new();
        for latitude in 0..128_u16 {
            let z = 1.0 - (f32::from(latitude) + 0.5) / 64.0;
            for longitude in 0..256_u16 {
                let phi = (f32::from(longitude) + 0.5) * PI / 128.0;
                let r = (1.0 - z * z).sqrt();
                let direction = Vec3::new(r * phi.cos(), r * phi.sin(), z);
                result.push((direction, radiance(direction), 4.0 * PI / 32768.0));
            }
        }
        result
    }

    #[test]
    fn constant_and_linear_environment_have_analytic_irradiance() {
        for slope in [0.0, 0.5] {
            let c = project(samples(|n| Vec3::splat(1.0 + slope * n.z)));
            let polynomial = diffuse_polynomial(&c);
            for n in [Vec3::Z, Vec3::NEG_Z, Vec3::X, Vec3::Y] {
                let actual = crate::irradiance_polynomial(n, &polynomial);
                let expected = Vec3::splat(1.0 + slope * n.z * 2.0 / 3.0);
                assert!((actual - expected).abs().max_element() < 0.0002);
            }
        }
    }

    #[test]
    fn single_lobe_and_each_basis_slot_preserve_z_up_orientation() {
        let c = project([(Vec3::Z, Vec3::ONE, 1.0)]);
        let polynomial = diffuse_polynomial(&c);
        let up = crate::irradiance_polynomial(Vec3::Z, &polynomial).x;
        assert!((up - 1.0625 / PI).abs() < 1e-6);
        for n in [
            Vec3::X,
            Vec3::Y,
            Vec3::Z,
            Vec3::new(1.0, 2.0, 3.0).normalize(),
        ] {
            for slot in 0..9 {
                let mut c = [Vec3::ZERO; 9];
                c[slot] = Vec3::ONE;
                let value = crate::irradiance_polynomial(n, &diffuse_polynomial(&c));
                assert!((value.x - basis(n)[slot] / PI).abs() < 1e-6);
            }
        }
    }
}
