use glam::Vec3;
use worldrunner_render_lighting::atmosphere::{HeightFog, SkyGradient};
use worldrunner_render_lighting::ibl::{brdf_integral, cube_direction, prefilter};

#[test]
fn horizon_zenith_and_linear_hdr() {
    let sky = SkyGradient::new(Vec3::splat(2.0), Vec3::splat(8.0), 1.0).unwrap();
    assert_eq!(sky.radiance(Vec3::X, Vec3::Z), Vec3::splat(2.0));
    assert_eq!(sky.radiance(Vec3::Z, Vec3::Z), Vec3::splat(8.0));
    assert_eq!(sky.radiance(-Vec3::Z, Vec3::Z), Vec3::splat(2.0));
    assert!(SkyGradient::new(Vec3::splat(-1.0), Vec3::ONE, 1.0).is_err());
}

#[test]
fn horizontal_fog_has_analytic_limit_and_altitude_continuity() {
    let fog = HeightFog::new(0.01, 0.2, 0.0, 0.0, Vec3::splat(4.0)).unwrap();
    let expected = (-1.0_f32).exp();
    assert!((fog.transmittance(Vec3::ZERO, Vec3::X * 100.0, Vec3::Z) - expected).abs() < 1e-6);
    for dz in [-0.0001, 0.0, 0.0001] {
        let t = fog.transmittance(Vec3::ZERO, Vec3::new(100.0, 0.0, dz), Vec3::Z);
        assert!((t - expected).abs() < 1e-5);
    }
    let low = fog.transmittance(Vec3::ZERO, Vec3::X * 100.0, Vec3::Z);
    let high = fog.transmittance(Vec3::Z * 10.0, Vec3::X * 100.0, Vec3::Z);
    assert!(high > low);
    assert_eq!(
        fog.transmittance(Vec3::ZERO, Vec3::ZERO, Vec3::Z).to_bits(),
        1.0f32.to_bits()
    );
}

#[test]
fn start_distance_excludes_the_initial_segment() {
    let fog = HeightFog::new(0.01, 0.2, 0.0, 10.0, Vec3::ONE).unwrap();
    assert_eq!(
        fog.transmittance(Vec3::ZERO, Vec3::Z * 5.0, Vec3::Z)
            .to_bits(),
        1.0f32.to_bits()
    );
    let a = fog.transmittance(Vec3::ZERO, Vec3::Z * 20.0, Vec3::Z);
    let b = HeightFog::new(0.01, 0.2, 0.0, 0.0, Vec3::ONE)
        .unwrap()
        .transmittance(Vec3::Z * 10.0, Vec3::Z * 10.0, Vec3::Z);
    assert!((a - b).abs() < 1e-6);
}

#[test]
fn brdf_mirror_limit_and_constant_cube_preserve_energy() {
    let lut = brdf_integral(1.0, 0.0, 1024);
    assert!((lut[0] - 1.0).abs() < 1e-4);
    assert!(lut[1].abs() < 1e-5);
    for roughness in [0.0, 0.3, 1.0] {
        let value = prefilter(Vec3::Z, roughness, 256, |_| Vec3::new(2.0, 4.0, 8.0));
        assert!((value - Vec3::new(2.0, 4.0, 8.0)).length() < 1e-4);
    }
    for face in 0..6 {
        assert!((cube_direction(face, [0.0, 0.0]).length() - 1.0).abs() < 1e-6);
    }
}
