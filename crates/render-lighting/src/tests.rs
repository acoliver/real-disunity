use super::*;

#[test]
fn explicit_light_direction_and_polynomial_frame_preserve_conventions() {
    let travel = Vec3::new(2.0, -3.0, 4.0);
    let toward = DirectionSemantics::LightTravel
        .toward_light(travel)
        .unwrap();
    assert!((toward + travel.normalize()).length() < 1e-6);
    assert!(
        DirectionSemantics::TowardLight
            .toward_light(Vec3::ZERO)
            .is_none()
    );
    for slot in 0..9 {
        let mut coefficients = [Vec3::ZERO; 9];
        coefficients[slot] = Vec3::new(1.0, 2.0, 3.0);
        let direction = Vec3::new(2.0, 3.0, 5.0);
        let basis = [1.0, 2.0, 3.0, 5.0, 6.0, 10.0, 15.0, -5.0, 25.0];
        assert!(
            (irradiance_polynomial(direction, &coefficients) - coefficients[slot] * basis[slot])
                .length()
                < 1e-6
        );
    }
}

#[test]
fn reflectance_and_exposure_have_known_endpoints() {
    assert!((exposure_from_stops(-2.0) - 0.25).abs() < 1e-6);
    let f0 = Vec3::new(0.04, 0.1, 0.2);
    assert!((fresnel_schlick(1.0, f0) - f0).length() < 1e-6);
    assert!((fresnel_schlick(0.0, f0) - Vec3::ONE).length() < 1e-6);
    assert!((distribution_ggx(1.0, 1.0, 1e-8) - std::f32::consts::FRAC_1_PI).abs() < 1e-6);
    for algorithm in [
        VisibilityAlgorithm::SmithCorrelated,
        VisibilityAlgorithm::SchlickSmithHotness,
    ] {
        assert!((visibility(1.0, 1.0, 1.0, algorithm) - 0.25).abs() < 1e-6);
        assert!(visibility(0.0, 0.0, 0.045, algorithm).is_finite());
    }
}

#[test]
fn composed_lighting_shader_is_valid() {
    let module = naga::front::wgsl::parse_str(WGSL).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
}

mod environment;
mod reference;

#[test]
fn specular_visibility_remap_and_occlusion_reference_values() {
    assert!((remap_roughness(0.25, true) - 0.5).abs() < 1e-7);
    assert!((remap_roughness(0.25, false) - 0.25).abs() < 1e-7);
    assert!(
        (visibility(0.5, 0.5, 0.5, VisibilityAlgorithm::SchlickSmithHotness) - 0.609_161_2).abs()
            < 1e-6
    );
    assert!(
        (visibility(0.5, 0.5, 0.5, VisibilityAlgorithm::SchlickSmithIbl) - 0.790_123_46).abs()
            < 1e-6
    );
    assert!((specular_occlusion(0.5, 0.5, 0.5) - 0.5).abs() < 1e-6);
    assert!(specular_occlusion(0.0, 0.0, 0.0).abs() < 1e-7);
    assert!((specular_occlusion(1.0, 1.0, 1.0) - 1.0).abs() < 1e-7);
}
#[test]
fn split_sum_energy_matches_independent_hemisphere_quadrature() {
    for algorithm in [
        VisibilityAlgorithm::SmithCorrelated,
        VisibilityAlgorithm::SchlickSmithIbl,
    ] {
        for (no_v, roughness) in [
            (1.0_f32, 0.5_f32),
            (0.5, 0.5),
            (0.2, 1.0),
            (0.8, 0.045),
            (0.1, 0.045),
        ] {
            let [a, b] = ibl::brdf_integral_with_visibility(no_v, roughness, 4096, algorithm);
            let energy = a * 0.04 + b;
            let independent = reference::integral(
                no_v,
                roughness,
                65536,
                algorithm == VisibilityAlgorithm::SchlickSmithIbl,
            );
            let reference = independent[0] * 0.04 + independent[1];
            if roughness >= 0.5 {
                assert!(
                    (reference - f64::from(quadrature(no_v, roughness, algorithm))).abs() < 0.003
                );
            }
            assert!(
                (f64::from(energy) - reference).abs() < 0.003,
                "{algorithm:?}: {energy} != {reference}"
            );
            assert!((0.0..=1.001).contains(&(a + b)));
        }
    }
}
fn quadrature(no_v: f32, roughness: f32, algorithm: VisibilityAlgorithm) -> f32 {
    let view = Vec3::new((1.0 - no_v * no_v).sqrt(), 0.0, no_v);
    let mut sum = 0.0;
    for row in 0..128u16 {
        let no_l = (f32::from(row) + 0.5) / 128.0;
        for column in 0..256u16 {
            let phi = (f32::from(column) + 0.5) * std::f32::consts::TAU / 256.0;
            let sine = (1.0 - no_l * no_l).sqrt();
            let light = Vec3::new(sine * phi.cos(), sine * phi.sin(), no_l);
            let half = (view + light).normalize();
            let fresnel = 0.04 + 0.96 * (1.0 - view.dot(half)).powi(5);
            let a2 = roughness.powi(4);
            let d = a2 / (std::f32::consts::PI * (half.z * half.z * (a2 - 1.0) + 1.0).powi(2));
            let vis = if algorithm == VisibilityAlgorithm::SchlickSmithIbl {
                let k = (roughness * roughness / 2.0).max(1e-5);
                0.25 / ((no_v * (1.0 - k) + k) * (no_l * (1.0 - k) + k))
            } else {
                let lambda = |n: f32| ((1.0 + a2 * (1.0 / (n * n) - 1.0)).sqrt() - 1.0) / 2.0;
                1.0 / (4.0 * no_v * no_l * (1.0 + lambda(no_v) + lambda(no_l)))
            };
            sum += d * vis * fresnel * no_l;
        }
    }
    sum * std::f32::consts::TAU / (128.0 * 256.0)
}
