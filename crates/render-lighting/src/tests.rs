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
