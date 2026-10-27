#[test]
fn world_anchored_body_parallax_tracks_camera_translation() {
    use crate::view::{CameraPose, OrthographicLens, SceneView, Viewport};
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let lighting = SceneCelestialLighting::new(0, 0, 1_000_000).unwrap();
    let settings = effects(lighting, SceneTint::WHITE);
    let mut atmosphere = settings.atmosphere().unwrap().clone();
    atmosphere.meshes[0].anchor_millimeters = Some([1000, 2000, 3000]);
    let matrix = crate::sky::meshes::model(atmosphere.meshes[0]);
    assert!(
        (matrix.transform_point3(glam::Vec3::ZERO) - glam::Vec3::new(1.0, 12.0, 3.0)).length()
            < 1e-6
    );
    atmosphere.meshes[0].anchor_millimeters = Some([0; 3]);
    let anchored = scene_with_effects(settings.with_atmosphere(atmosphere).unwrap());
    let following = scene(lighting, SceneTint::WHITE);
    let origin = scene_tests::render(&context, &following, 1.0);
    assert_eq!(
        scene_tests::render(&context, &anchored, 1.0).color,
        origin.color
    );
    let view = SceneView::orthographic_with_up(
        CameraPose {
            eye_millimeters: [10_000, 0, 0],
            target_millimeters: [10_000, 1000, 0],
        },
        OrthographicLens {
            vertical_half_extent_meters: 2.0,
            near_meters: 0.1,
            far_meters: 100.0,
        },
        [0.0, 0.0, 1.0],
        Viewport {
            width: 9,
            height: 3,
        },
    )
    .unwrap();
    let moved = scene_tests::render_at(&context, &anchored, 1.0, &view);
    let follow = scene_tests::render_at(&context, &following, 1.0, &view);
    assert_eq!(
        follow.rgb(4).map(f32::to_bits),
        origin.rgb(4).map(f32::to_bits)
    );
    assert_eq!(
        moved.rgb(4).map(f32::to_bits),
        [0.0_f32, 0.0, 2.0].map(f32::to_bits)
    );
}

#[test]
fn gpu_hdr_body_tint_preserves_percentage_values_above_one() {
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let lighting = SceneCelestialLighting::new(0, 0, 1_000_000).unwrap();
    let tint = SceneTint::new([2_550_000, 1_500_000, 500_000]).unwrap();
    let image = scene_tests::render(&context, &scene(lighting, tint), 1.0);
    let specular = 0.04 / (4.0 * std::f64::consts::PI);
    for (actual, expected) in image
        .rgb(4)
        .into_iter()
        .zip([2.55 + specular, specular, specular])
    {
        assert!((f64::from(actual) - expected).abs() < expected * 2.0_f64.powi(-10) + 1e-6);
    }
    let mut bytes = crate::geometry::pack_model_instance(glam::Mat4::IDENTITY);
    let neutral = bytes.clone();
    crate::geometry::tint_instance(&mut bytes, SceneTint::WHITE);
    assert_eq!(bytes, neutral);
}

#[test]
fn coloured_sun_matches_linear_cpu_oracle_and_white_preserves_frame_bytes() {
    use crate::frame::FrameUniform;
    let lighting = SceneCelestialLighting::new(1_000_000, 0, 0).unwrap();
    let base = effects(lighting, SceneTint::WHITE);
    let view = scene_tests::view();
    let neutral = FrameUniform::new(&view, Some(&base)).unwrap();
    assert_eq!(&neutral.to_bytes()[308..320], &[0; 12]);
    let white = base.clone().with_sun_tint(SceneTint::WHITE);
    assert_eq!(
        neutral.to_bytes(),
        FrameUniform::new(&view, Some(&white)).unwrap().to_bytes()
    );
    let selected = base.with_sun_tint(SceneTint::new([500_000, 2_000_000, 0]).unwrap());
    let frame = FrameUniform::new(&view, Some(&selected)).unwrap();
    assert_eq!(
        frame.sun_irradiance.to_array().map(f32::to_bits),
        [0.5_f32, 2.0, 0.0].map(f32::to_bits)
    );
    assert_eq!(frame.sun_direction, neutral.sun_direction);
    assert_eq!(frame.diffuse_environment, neutral.diffuse_environment);
    let context = pollster::block_on(crate::device::GpuContext::request()).unwrap();
    let image = scene_tests::render(&context, &scene_with_effects(selected), 1.0);
    let specular = 0.04 / (4.0 * std::f64::consts::PI);
    let expected = [
        (0.96 / std::f64::consts::PI + specular) * 0.5,
        specular * 2.0,
        0.0,
    ];
    for (actual, expected) in image.rgb(4).into_iter().zip(expected) {
        assert!((f64::from(actual) - expected).abs() < expected * 2.0_f64.powi(-10) + 1e-6);
    }
}

use super::scene_tests;
use crate::device::GpuContext;
use worldrunner_render_plan::*;

#[test]
fn sun_tint_preserves_roughness_aware_ao_with_fixed_sh() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for intensity in [1_500_000, 3_000_000] {
        let scalar = f64::from(intensity) / 1e6;
        let ao = 64.0 / 255.0 + (1.0 - 64.0 / 255.0) * (scalar / 3.0);
        let specular_ao = ((1.0 + ao).powf(2.0_f64.powi(-17)) - 1.0 + ao).min(1.0);
        for ordinary in [false, true] {
            for tint in [[1_000_000; 3], [0, 1_000_000, 0], [500_000, 2_000_000, 0]] {
                let source = ao_scene(intensity, SceneTint::new(tint).unwrap(), ordinary);
                let image = scene_tests::render(&context, &source, 1.0);
                let direct_diffuse = if ordinary {
                    0.96 / std::f64::consts::PI * ao
                } else {
                    0.0
                };
                let specular = 0.04 / (4.0 * std::f64::consts::PI) * specular_ao;
                let expected = tint.map(|v| scalar * f64::from(v) / 1e6 * specular);
                let expected = [
                    expected[0] + scalar * f64::from(tint[0]) / 1e6 * direct_diffuse + 0.25 * ao,
                    expected[1],
                    expected[2],
                ];
                for (actual, expected) in image.rgb(4).into_iter().zip(expected) {
                    assert!(
                        (f64::from(actual) - expected).abs() < expected * 0.001 + 1e-5,
                        "intensity={intensity}, ordinary={ordinary}, tint={tint:?}: {actual} vs {expected}"
                    );
                }
            }
        }
    }
}

fn ao_scene(intensity: u32, tint: SceneTint, ordinary: bool) -> RenderScene {
    let lighting = SceneCelestialLighting::new(0, 1_000_000, 0).unwrap();
    let settings = effects_at(lighting, SceneTint::WHITE, u64::from(intensity))
        .with_sun_tint(tint)
        .with_specular_convention(SceneSpecularConvention {
            occlusion: SceneSpecularOcclusion::RoughnessAware,
            ..SceneSpecularConvention::default()
        });
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(1),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: Some(TextureHandle::from_raw(3)),
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: true,
    })
    .unwrap();
    let objects = if ordinary {
        vec![SceneObject::new(
            SceneTransform {
                position_millimeters: [0, 2000, 0],
                rotation: SceneRotation::IDENTITY,
                scale_millionths: 1_000_000,
            },
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        )]
    } else {
        vec![]
    };
    RenderScene::new(SceneParts {
        resources: SceneResources::new(scene_tests::textures().to_vec(), vec![material]).unwrap(),
        geometry: SceneGeometry::new(vec![scene_tests::quad()], objects).unwrap(),
        terrain: None,
        effects: Some(settings),
    })
    .unwrap()
}

fn effects(lighting: SceneCelestialLighting, tint: SceneTint) -> SceneEffects {
    effects_at(lighting, tint, 1_000_000)
}
fn effects_at(lighting: SceneCelestialLighting, tint: SceneTint, intensity: u64) -> SceneEffects {
    let base = scene_tests::base_effects(false);
    let mut atmosphere = scene_tests::atmosphere(0);
    atmosphere.fog = None;
    atmosphere.meshes[0].material = MaterialHandle::from_raw(0);
    atmosphere.meshes[0].lighting = lighting;
    atmosphere.meshes[0].tint = tint;
    let mut coefficients = [[0; 3]; 9];
    coefficients[0] = [250_000; 3];
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *base.sky(),
        sun: SceneSun {
            direction: SceneDirection::new([0, -1_000_000, 0]).unwrap(),
            irradiance_micro: intensity,
        },
        ambient: *base.ambient(),
        shadows: base.shadows().clone(),
        ssao: None,
        camera: *base.camera(),
        grading: *base.grading(),
        post: vec![],
        anti_alias: *base.anti_alias(),
    })
    .unwrap()
    .with_diffuse_environment(SceneDiffuseEnvironment::Sh9(
        SceneSh9::new(coefficients).unwrap(),
    ))
    .with_atmosphere(atmosphere)
    .unwrap()
}
fn scene(lighting: SceneCelestialLighting, tint: SceneTint) -> RenderScene {
    scene_with_effects(effects(lighting, tint))
}
fn scene_with_effects(effects: SceneEffects) -> RenderScene {
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            scene_tests::textures().to_vec(),
            vec![scene_tests::material(1)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![scene_tests::quad()], vec![]).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

#[test]
fn gpu_each_body_contribution_matches_an_independent_cpu_oracle() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for values in [
        [1_000_000; 3],
        [0; 3],
        [2_000_000, 0, 0],
        [0, 2_000_000, 0],
        [0, 0, 2_000_000],
        [500_000, 2_000_000, 3_000_000],
    ] {
        let lighting = SceneCelestialLighting::new(values[0], values[1], values[2]).unwrap();
        let image = scene_tests::render(&context, &scene(lighting, SceneTint::WHITE), 1.0);
        let [direct, environment, emission] = values.map(|v| f64::from(v) / 1e6);
        // At normal incidence with unit roughness, GGX D=1/pi, G/4=1/4, F=0.04.
        let specular = 0.04 / (4.0 * std::f64::consts::PI);
        let expected = [
            0.96 / std::f64::consts::PI * direct + specular + 0.25 * environment + emission,
            specular,
            specular,
        ];
        for (actual, expected) in image.rgb(4).into_iter().zip(expected) {
            let tolerance = expected.abs() * 2.0_f64.powi(-10) + 1e-6;
            assert!(
                (f64::from(actual) - expected).abs() < tolerance,
                "{values:?}: {actual} vs {expected}"
            );
        }
    }
}
