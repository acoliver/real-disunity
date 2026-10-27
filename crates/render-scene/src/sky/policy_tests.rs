use super::{render_tests, uniform};
use crate::{
    frame::FrameUniform,
    view::{CameraPose, SceneView, Viewport},
};
use glam::{Mat4, Vec3, Vec4};
use worldrunner_render_plan::{
    SceneBackgroundDepthPolicy, SceneCamera, SceneFogDensityPolicy, SceneHeightFog,
};

fn frame(shift: Vec3) -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, 0, 20_000],
            target_millimeters: [0, 1000, 20_000],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: 1,
            height: 1,
        },
    )
    .unwrap();
    let mut frame = FrameUniform::unlit(&view);
    frame.background_far_plane_meters = view.clip_distances_meters()[1];
    frame.view_projection = Mat4::from_translation(shift) * frame.view_projection;
    frame
}

fn oracle(offset: Vec3) -> f64 {
    let mut average = 0.0;
    for sample in 0..4096 {
        let altitude = 20.0 + f64::from(offset.z) * (f64::from(sample) + 0.5) / 4096.0;
        average += 0.02 * (-0.03 * altitude).exp() / 4096.0;
    }
    2.0 * (-average * (f64::from(offset.length()) - 25.0)).exp()
}

#[test]
fn background_fog_raster_matches_full_ray_oracle_at_far_plane() {
    let mut settings = render_tests::settings();
    settings.background_depth_policy = SceneBackgroundDepthPolicy::FarPlane;
    settings.background_distance_millimeters = 500_000;
    settings.fog = Some(SceneHeightFog {
        density_policy: SceneFogDensityPolicy::FullRay,
        density: worldrunner_render_plan::SceneFogDensity::new(0.02).unwrap(),
        falloff_micro_per_meter: 30_000,
        base_height_millimeters: 0,
        start_distance_millimeters: 25_000,
        radiance_micro: [0; 3],
    });
    for shift in [
        Vec3::new(0.0, -0.8, 0.0),
        Vec3::new(0.0, 0.8, 0.0),
        Vec3::new(0.6, -0.4, 0.0),
    ] {
        let mut frame = frame(shift);
        frame.height_fog = settings.fog.map(super::height_fog);
        let point = frame.view_projection.inverse() * Vec4::new(0.0, 0.0, 1.0, 1.0);
        let offset = point.truncate() / point.w - frame.camera_position;
        let expected = oracle(offset);
        let (bytes, _) = render_tests::render_frame(&settings, &frame, 1.0);
        let actual = half::f16::from_le_bytes([bytes[0], bytes[1]]).to_f64();
        // Binary16 output rounding plus far/near-conditioned projection inversion.
        assert!(
            (actual - expected).abs() < 0.002,
            "{shift}: {actual} != {expected}"
        );
    }
}

#[test]
fn default_background_policy_keeps_reserved_uniform_bytes_zero() {
    let settings = render_tests::settings();
    assert_eq!(
        settings.background_depth_policy,
        SceneBackgroundDepthPolicy::default()
    );
    assert_eq!(&uniform(&settings, None, false)[32..64], &[0; 32]);
}

#[test]
fn separate_surface_keeps_opacity_surface_fog_and_two_exposures_independent() {
    use super::scene_tests;
    use worldrunner_render_plan::*;
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let mut sky = scene_tests::atmosphere(100_000);
    sky.celestial_composition = SceneCelestialComposition::SeparateSurface;
    sky.celestial_exposure_micro = Some(500_000);
    let mut textures = scene_tests::textures().to_vec();
    textures[2] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0, 255, 0, 128].into(),
    };
    let source = RenderScene::new(SceneParts {
        resources: SceneResources::new(
            textures,
            vec![scene_tests::material(1), scene_tests::material(2)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![scene_tests::quad()], vec![]).unwrap(),
        terrain: None,
        effects: Some(
            scene_tests::base_effects(false)
                .with_atmosphere(sky)
                .unwrap(),
        ),
    })
    .unwrap();
    let image = scene_tests::render(&context, &source, 2.0);
    let actual = image.rgb(4);
    let object = 128.0 / 255.0 * (-1.0_f64).exp() * 0.5;
    let background = (2.0 * (-2.0_f64).exp() + 1.0 - (-2.0_f64).exp()) * 2.0;
    assert!((f64::from(actual[1]) - object).abs() < 0.001);
    assert!((f64::from(actual[2]) - background).abs() < 0.003);
}

#[test]
fn independent_object_exposure_does_not_change_inherited_surface_fog_distance() {
    use super::scene_tests;
    use worldrunner_render_plan::*;
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let source = scene_tests::scene(100_000, false, false);
    let mut sky = source.effects().unwrap().atmosphere().unwrap().clone();
    sky.celestial_exposure_micro = Some(500_000);
    let selected = RenderScene::new(SceneParts {
        resources: source.resources().clone(),
        geometry: source.geometry().clone(),
        terrain: None,
        effects: Some(
            source
                .effects()
                .unwrap()
                .clone()
                .with_atmosphere(sky)
                .unwrap(),
        ),
    })
    .unwrap();
    let actual = scene_tests::render(&context, &selected, 2.0).rgb(4);
    let transmission = (-1.0_f64).exp();
    assert!((f64::from(actual[1]) - transmission * 0.5).abs() < 0.001);
    assert!((f64::from(actual[2]) - (1.0 - transmission) * 0.5).abs() < 0.001);
}
