use super::{height_fog, render_tests, scattering, uniform};
use glam::Vec3;
use worldrunner_render_lighting::atmosphere::scattering::ScatteringSample;
use worldrunner_render_plan::{SceneFogDensity, SceneHeightFog};

#[test]
fn neutral_sky_parameters_preserve_every_byte() {
    let mut sky = render_tests::settings();
    sky.scattering = Some(scattering::tests::settings());
    let expected: [[f32; 4]; 10] = [
        [2.0, 2.0, 2.0, 1.0],
        [2.0, 2.0, 2.0, 0.0],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0; 4],
        [0.0, 0.0, 1.0, 1000.0],
        [
            5_800_000.0 * 1e-12,
            13_500_000.0 * 1e-12,
            33_100_000.0 * 1e-12,
            0.76,
        ],
        [
            2_000_000.0 * 1e-12,
            2_000_000.0 * 1e-12,
            2_000_000.0 * 1e-12,
            20.0,
        ],
        [128.0, 100.0, 1.0, 0.0],
    ];
    let expected: Vec<_> = expected
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect();
    assert_eq!(uniform(&sky, None, false), expected);
}

fn radiance_oracle(exposure: f64, peak: f64) -> [f64; 3] {
    let mu = 0.25_f64;
    let g = 0.76_f64;
    let molecular = 3.0 * (1.0 + mu * mu) / (16.0 * std::f64::consts::PI);
    let aerosol = 3.0 * (1.0 - g * g) * (1.0 + mu * mu)
        / (8.0 * std::f64::consts::PI * (2.0 + g * g) * (1.0 + g * g - 2.0 * g * mu).powf(1.5));
    let color = [5.8e-6, 13.5e-6, 33.1e-6].map(|coefficient| {
        20.0 * exposure * (1000.0 * coefficient * molecular + 100.0 * 2e-6 * aerosol)
    });
    let luminance = color[0] * 0.2126 + color[1] * 0.7152 + color[2] * 0.0722;
    color.map(|channel| channel * luminance.clamp(0.0, peak) / luminance.max(0.0001))
}

#[test]
fn sky_exposure_scales_radiance_before_limiting_without_changing_columns() {
    let neutral = scattering::tests::settings();
    for exposure in [0_u32, 500_000, 1_000_000, 4_000_000, 64_000_000] {
        let mut settings = neutral;
        settings.sky_exposure_micro = exposure;
        settings.peak_radiance_micro = 10_000;
        settings.validate().unwrap();
        let model = scattering::model(settings);
        let sample = ScatteringSample {
            molecular: Vec3::splat(1000.0),
            aerosol: Vec3::splat(100.0),
            transmission: Vec3::ONE,
        };
        let actual = model.radiance(sample, 0.25).to_array();
        let expected = radiance_oracle(f64::from(exposure) / 1e6, 0.01);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!((f64::from(actual) - expected).abs() <= expected.abs() * 2e-6 + 1e-9);
        }
        assert_eq!(
            scattering::pixels(settings, 0.5, 0.1),
            scattering::pixels(neutral, 0.5, 0.1)
        );
    }
    let mut invalid = neutral;
    invalid.sky_exposure_micro = 64_000_001;
    assert!(invalid.validate().is_err());
}

fn fog(density: f32) -> SceneHeightFog {
    SceneHeightFog {
        density: SceneFogDensity::new(density).unwrap(),
        falloff_micro_per_meter: 0,
        base_height_millimeters: 0,
        start_distance_millimeters: 0,
        radiance_micro: [4_000_000; 3],
    }
}

#[test]
fn sub_micro_fog_matches_independent_beer_lambert_integral() {
    for density in [0.0, f32::EPSILON, 0.000_000_5, 0.001, 1.0] {
        let physical = height_fog(fog(density));
        assert_eq!(physical.density.to_bits(), density.to_bits());
        let actual = physical.transmittance(Vec3::ZERO, Vec3::Y * 1000.0, Vec3::Z);
        let expected = (-f64::from(density) * 1000.0).exp();
        assert!((f64::from(actual) - expected).abs() < 2e-7);
        if density == 0.0 {
            assert_eq!(actual.to_bits(), 1.0_f32.to_bits());
        }
    }
}

#[test]
fn gpu_sub_micro_fog_and_zero_match_hdr_oracle() {
    let mut sky = render_tests::settings();
    sky.background_distance_millimeters = 4_000_000_000;
    let frame = render_tests::test_frame(1.0);
    let (clear, _) = render_tests::render_frame(&sky, &frame, 1.0);
    for density in [0.0, f32::EPSILON, 0.000_000_5] {
        sky.fog = Some(fog(density));
        let mut fog_frame = frame;
        fog_frame.height_fog = sky.fog.map(height_fog);
        let (actual, _) = render_tests::render_frame(&sky, &fog_frame, 1.0);
        let expected = 4.0 - 2.0 * (-f64::from(density) * 4_000_000.0).exp();
        for channel in actual[..6].chunks_exact(2) {
            let channel = half::f16::from_le_bytes(channel.try_into().unwrap()).to_f32();
            // Binary16 storage contributes at most 2^-10 relative error.
            assert!((f64::from(channel) - expected).abs() < expected * 2.0_f64.powi(-10));
        }
        if density == 0.0 {
            assert_eq!(actual, clear);
        }
    }
}

#[test]
fn gpu_sky_exposure_leaves_frame_and_direct_sun_unchanged() {
    let mut sky = render_tests::settings();
    let mut physical = scattering::tests::settings();
    sky.scattering = Some(physical);
    let mut frame = render_tests::test_frame(1.0);
    frame.sun_direction = Vec3::Z;
    let (neutral, _) = render_tests::render_frame(&sky, &frame, 1.0);
    physical.sky_exposure_micro = 2_000_000;
    sky.scattering = Some(physical);
    let (twice, _) = render_tests::render_frame(&sky, &frame, 1.0);
    for (a, b) in neutral[..6].chunks_exact(2).zip(twice[..6].chunks_exact(2)) {
        let a = half::f16::from_le_bytes(a.try_into().unwrap()).to_f32();
        let b = half::f16::from_le_bytes(b.try_into().unwrap()).to_f32();
        assert!((b - 2.0 * a).abs() < 0.003);
    }
    sky.sun_angular_radius_millidegrees = 45_000;
    physical.compose_celestial = false;
    sky.scattering = Some(physical);
    frame.sun_direction = Vec3::Y;
    let (sun, _) = render_tests::render_frame(&sky, &frame, 1.0);
    assert_eq!(sun, [0, 0x48, 0, 0x48, 0, 0x48, 0, 0x3c]);
    assert_eq!(frame.exposure.to_bits(), 1.0_f32.to_bits());
}
