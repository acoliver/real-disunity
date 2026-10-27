use super::super::{preparation, render_tests};
use super::*;

pub(crate) fn settings() -> SceneScattering {
    SceneScattering {
        compose_celestial: true,
        planet_radius_meters: 6_360_000,
        atmosphere_radius_meters: 6_420_000,
        molecular_scattering_pico_per_meter: [5_800_000, 13_500_000, 33_100_000],
        aerosol_scattering_pico_per_meter: [2_000_000; 3],
        molecular_height_meters: 8000,
        aerosol_height_meters: 1200,
        anisotropy_micro: 760_000,
        solar_radiance_micro: 20_000_000,
        sky_exposure_micro: 1_000_000,
        solar_diameter_microradians: 530_000,
        peak_radiance_micro: 100_000_000,
        model: SceneScatteringModel::ElevationColumn20,
        lookup_rows: 128,
        format: SceneScatteringFormat::Float16,
    }
}
fn rgb(bytes: &[u8]) -> Vec3 {
    Vec3::from_array(std::array::from_fn(|i| {
        half::f16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]).to_f32()
    }))
}
#[test]
fn half_storage_saturates_and_truncates() {
    assert_eq!(quantize(1e5), half::f16::MAX.to_bits());
    for value in [0.00001, 0.12345, 1.2345, 12345.0] {
        let stored = half::f16::from_bits(quantize(value)).to_f32();
        assert!(stored <= value);
        assert!(value - stored <= value * 0.000_977 + 5.961e-8);
    }
}
#[test]
#[expect(clippy::cast_precision_loss, reason = "bounded lookup indices")]
fn every_lookup_row_stores_the_defined_model_with_declared_quantization() {
    for format in [
        SceneScatteringFormat::Float16,
        SceneScatteringFormat::Float32,
    ] {
        let settings = SceneScattering {
            format,
            ..settings()
        };
        let bytes = pixels(settings, 0.5, 0.1);
        for row in 0..settings.lookup_rows {
            let elevation = (1.0 - (row as f32 + 0.5) / settings.lookup_rows as f32)
                * std::f32::consts::FRAC_PI_2;
            let direct = model(settings).integrate(elevation, 0.5, 0.1);
            for (layer, expected) in [direct.molecular, direct.aerosol, direct.transmission]
                .into_iter()
                .enumerate()
            {
                let offset = (layer * settings.lookup_rows as usize + row as usize) * 256;
                for (channel, value) in expected.to_array().into_iter().enumerate() {
                    if format == SceneScatteringFormat::Float16 {
                        let stored = half::f16::from_le_bytes(
                            bytes[offset + channel * 2..offset + channel * 2 + 2]
                                .try_into()
                                .unwrap(),
                        )
                        .to_f32();
                        let bounded = value.min(65_504.0);
                        assert!(
                            stored <= bounded && bounded - stored <= bounded * 0.000_977 + 5.961e-8
                        );
                    } else {
                        assert_eq!(
                            &bytes[offset + channel * 4..offset + channel * 4 + 4],
                            &value.to_le_bytes()
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn preparation_budget_and_cancellation_control_publication() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let device = context.device();
    let mut maps = None;
    let transaction = context
        .submission()
        .prepare(device, |encoder, _| {
            assert!(matches!(
                preparation::encode_atmosphere(
                    device,
                    encoder,
                    settings(),
                    (Vec3::Z, 0.1),
                    AllocationBudget(1)
                ),
                Err(PreparationError::Budget)
            ));
            maps = Some(preparation::encode_atmosphere(
                device,
                encoder,
                settings(),
                (Vec3::Z, 0.1),
                AllocationBudget(1_000_000),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    assert!(maps.as_ref().unwrap().view().is_err());
    transaction.cancel();
    assert_eq!(
        maps.as_ref().unwrap().status(),
        worldrunner_render_gpu::submission::TransactionStatus::Cancelled
    );
    assert!(maps.take().unwrap().view().is_err());
    let transaction = context
        .submission()
        .prepare(device, |encoder, _| {
            maps = Some(preparation::encode_atmosphere(
                device,
                encoder,
                settings(),
                (Vec3::Z, 0.1),
                AllocationBudget(1_000_000),
            )?);
            Ok::<_, PreparationError>(())
        })
        .unwrap();
    transaction.submit().unwrap();
    assert!(maps.unwrap().view().is_ok());
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .unwrap();
}
#[test]
fn gpu_lookup_formats_match_their_saturated_or_unquantized_reference() {
    let mut sky = render_tests::settings();
    let mut frame = render_tests::test_frame(1.0);
    frame.sun_direction = Vec3::Z;
    for format in [
        SceneScatteringFormat::Float16,
        SceneScatteringFormat::Float32,
    ] {
        let settings = SceneScattering {
            format,
            ..settings()
        };
        sky.scattering = Some(settings);
        let (actual, _) = render_tests::render_frame(&sky, &frame, 1.0);
        let physical = model(settings);
        let elevation = std::f32::consts::FRAC_PI_2 / 256.0;
        let mut direct = physical.integrate(elevation, std::f32::consts::FRAC_PI_2, 0.1);
        assert!(
            direct.molecular.max_element() > 65_504.0,
            "horizon exercises half saturation"
        );
        if format == SceneScatteringFormat::Float16 {
            direct.molecular = direct
                .molecular
                .map(|v| half::f16::from_bits(quantize(v)).to_f32());
            direct.aerosol = direct
                .aerosol
                .map(|v| half::f16::from_bits(quantize(v)).to_f32());
        }
        let expected = physical.radiance(direct, 0.0);
        let error = (rgb(&actual) - expected).abs();
        assert!(
            error.cmple(expected * 0.001 + Vec3::splat(1e-5)).all(),
            "{format:?}: {:?} vs {expected:?}",
            rgb(&actual)
        );
    }
}

#[test]
fn gpu_physical_background_and_discs_share_scattering_exposure_fog_and_depth() {
    let mut sky = render_tests::settings();
    sky.scattering = Some(settings());
    let mut frame = render_tests::test_frame(1.0);
    frame.sun_direction = Vec3::Z;
    let (background, _) = render_tests::render_frame(&sky, &frame, 1.0);
    assert!(rgb(&background).min_element() > 0.0);
    sky.discs.push(worldrunner_render_plan::SceneCelestialDisc {
        direction: worldrunner_render_plan::SceneDirection::new([0, 1_000_000, 0]).unwrap(),
        angular_radius_millidegrees: 30_000,
        rotation_millidegrees: 0,
        texture: None,
        radiance_micro: [1_000_000; 3],
    });
    let (body, _) = render_tests::render_frame(&sky, &frame, 1.0);
    let transmission = rgb(&body) - rgb(&background);
    assert!(transmission.min_element() >= 0.0 && transmission.max_element() < 1.0);
    frame.exposure = 2.0;
    let (twice, _) = render_tests::render_frame(&sky, &frame, 1.0);
    assert!((rgb(&twice) - rgb(&body) * 2.0).abs().max_element() < 0.003);
    let (occluded, depth) = render_tests::render_frame(&sky, &frame, 0.5);
    assert_eq!(
        rgb(&occluded).to_array().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    assert_eq!(depth.to_bits(), 0.5_f32.to_bits());
    frame.height_fog = Some(
        worldrunner_render_lighting::atmosphere::HeightFog::new(
            0.001,
            0.0,
            0.0,
            0.0,
            Vec3::splat(0.25),
        )
        .unwrap(),
    );
    let (fogged, _) = render_tests::render_frame(&sky, &frame, 1.0);
    let expected = Vec3::splat(0.5).lerp(rgb(&twice), (-1.0_f32).exp());
    assert!((rgb(&fogged) - expected).abs().max_element() < 0.003);
}

#[test]
fn gpu_elevation_columns_have_no_azimuthal_twilight_shadow() {
    let mut sky = render_tests::settings();
    sky.scattering = Some(SceneScattering {
        aerosol_scattering_pico_per_meter: [0; 3],
        solar_diameter_microradians: 1,
        format: SceneScatteringFormat::Float32,
        ..settings()
    });
    let mut frame = render_tests::test_frame(1.0);
    frame.sun_direction = Vec3::new(0.0, (-0.05_f32).cos(), (-0.05_f32).sin());
    let (sunward, _) = render_tests::render_frame(&sky, &frame, 1.0);
    frame.sun_direction.y = -frame.sun_direction.y;
    let (antisunward, _) = render_tests::render_frame(&sky, &frame, 1.0);
    assert!(rgb(&sunward).min_element() > 0.0);
    assert_eq!(rgb(&sunward), rgb(&antisunward));
}

#[test]
fn gpu_interior_elevation_interpolates_adjacent_quantized_columns() {
    let mut sky = render_tests::settings();
    let mut frame = render_tests::test_frame(1.0);
    frame.sun_direction = Vec3::Z;
    let elevation = (1.0 - 49.75 / 128.0) * std::f32::consts::FRAC_PI_2;
    frame.view_projection *= glam::Mat4::from_rotation_x(-elevation);
    for format in [
        SceneScatteringFormat::Float16,
        SceneScatteringFormat::Float32,
    ] {
        let settings = SceneScattering {
            format,
            ..settings()
        };
        sky.scattering = Some(settings);
        let (actual, _) = render_tests::render_frame(&sky, &frame, 1.0);
        let physical = model(settings);
        let row_elevations =
            [49.5_f32, 50.5].map(|row| (1.0 - row / 128.0) * std::f32::consts::FRAC_PI_2);
        let [mut first, mut second] =
            row_elevations.map(|angle| physical.integrate(angle, std::f32::consts::FRAC_PI_2, 0.1));
        if format == SceneScatteringFormat::Float16 {
            for sample in [&mut first, &mut second] {
                sample.molecular = sample
                    .molecular
                    .map(|v| half::f16::from_bits(quantize(v)).to_f32());
                sample.aerosol = sample
                    .aerosol
                    .map(|v| half::f16::from_bits(quantize(v)).to_f32());
            }
        }
        first.molecular = first.molecular.lerp(second.molecular, 0.25);
        first.aerosol = first.aerosol.lerp(second.aerosol, 0.25);
        let expected = physical.radiance(first, elevation.sin());
        assert!(
            (rgb(&actual) - expected)
                .abs()
                .cmple(expected * 0.002 + Vec3::splat(1e-5))
                .all(),
            "{format:?}: {:?} vs {expected:?}",
            rgb(&actual)
        );
    }
}
