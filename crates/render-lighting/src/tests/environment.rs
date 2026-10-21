use super::*;

fn config() -> EnvironmentConfig {
    EnvironmentConfig {
        frame: LightingFrame {
            up: Vec3::Z,
            coefficient_from_world: Mat3::IDENTITY,
            direction_semantics: DirectionSemantics::TowardLight,
        },
        direct: DirectionalLight {
            direction: Vec3::Z * 3.0,
            radiance: Vec3::splat(2.0),
        },
        shadow_direction: Some(Vec3::Y),
        ambient: AmbientLight::Hemisphere {
            sky: Vec3::splat(0.8),
            ground: Vec3::splat(0.2),
        },
        exposure: ExposureOwnership::SceneLinear(0.25),
    }
}

#[test]
fn rejects_nonfinite_singular_and_nonorthonormal_frames() {
    for basis in [
        Mat3::ZERO,
        Mat3::from_diagonal(Vec3::new(1.0, 2.0, 1.0)),
        Mat3::from_cols(Vec3::X, Vec3::X, Vec3::Z),
        Mat3::from_diagonal(Vec3::splat(f32::NAN)),
    ] {
        let mut c = config();
        c.frame.coefficient_from_world = basis;
        assert!(matches!(c.prepare(), Err(EnvironmentError::InvalidFrame)));
    }
    let mut c = config();
    c.frame.up = Vec3::ZERO;
    assert!(matches!(c.prepare(), Err(EnvironmentError::InvalidFrame)));
}

#[test]
fn rejects_invalid_direct_and_independent_shadow_directions() {
    for direction in [
        Vec3::ZERO,
        Vec3::splat(f32::INFINITY),
        Vec3::splat(f32::NAN),
    ] {
        let mut c = config();
        c.direct.direction = direction;
        assert!(matches!(
            c.prepare(),
            Err(EnvironmentError::InvalidDirectLight)
        ));
        let mut c = config();
        c.shadow_direction = Some(direction);
        assert!(matches!(
            c.prepare(),
            Err(EnvironmentError::InvalidShadowDirection)
        ));
    }
}

#[test]
fn validates_radiance_coefficients_and_exposure_without_clamping() {
    let mut c = config();
    c.direct.radiance.x = -1.0;
    assert!(c.prepare().is_err());
    for exposure in [
        ExposureOwnership::SceneLinear(f32::NAN),
        ExposureOwnership::SceneLinear(-1.0),
        ExposureOwnership::PostProcess {
            authored_stops: f32::INFINITY,
        },
    ] {
        c = config();
        c.exposure = exposure;
        assert!(matches!(
            c.prepare(),
            Err(EnvironmentError::InvalidExposure)
        ));
    }
    c = config();
    c.ambient = AmbientLight::Polynomial([Vec3::splat(f32::NAN); 9]);
    assert!(matches!(c.prepare(), Err(EnvironmentError::InvalidAmbient)));
}

#[test]
fn prepares_basis_distinct_directions_and_exposure_ownership() {
    let mut c = config();
    c.frame.direction_semantics = DirectionSemantics::LightTravel;
    c.frame.coefficient_from_world = Mat3::from_cols(Vec3::X, Vec3::Z, Vec3::NEG_Y);
    let mut coefficients = [Vec3::ZERO; 9];
    coefficients[3] = Vec3::splat(3.0);
    c.ambient = AmbientLight::Polynomial(coefficients);
    c.exposure = ExposureOwnership::PostProcess {
        authored_stops: 0.36,
    };
    let prepared = c.prepare().unwrap();
    assert!((prepared.toward_light + Vec3::Z).length() < 1e-6);
    assert!((prepared.toward_shadow_light.unwrap() + Vec3::Y).length() < 1e-6);
    assert!((prepared.ambient_radiance(Vec3::Y) - Vec3::splat(3.0)).length() < 1e-6);
    assert!((prepared.scene_exposure - 1.0).abs() < 1e-6);
    let prepared = config().prepare().unwrap();
    assert!((prepared.ambient_radiance(Vec3::Z) - Vec3::splat(0.8)).length() < 1e-6);
    assert!((prepared.scene_exposure - 0.25).abs() < 1e-6);
}
