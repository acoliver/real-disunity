#[test]
fn fog_segments_preserve_existing_fog_without_dividing_by_transmittance() {
    use glam::Vec3;
    let background = Vec3::new(0.5, 0.25, 0.125);
    let fog = Vec3::splat(0.25);
    let scattering = Vec3::splat(0.125);
    let specular = Vec3::splat(0.0625);
    for air in [0.0, 0.25, 1.0] {
        let snapshot = fog.lerp(background, air * 0.5);
        for medium in [0.0, 0.5, 1.0] {
            let expected = fog.lerp(
                background * (0.5 * medium)
                    + fog * (0.5 * medium)
                    + scattering * (1.0 - medium)
                    + specular,
                air,
            );
            let actual = worldrunner_render_lighting::atmosphere::compose_water_fog(
                snapshot,
                Vec3::splat(medium),
                scattering,
                specular,
                air,
                fog,
            );
            assert_eq!(actual, expected);
        }
    }
}

#[path = "environment_tests.rs"]
pub(super) mod environment_tests;
use super::*;

#[test]
fn water_fog_composes_only_the_camera_segment_at_every_supported_sample_count() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in crate::msaa_tests::supported_samples(&context) {
        let mut renderer = regressions::renderer(&context, samples);
        let mut body = settings();
        body.absorption_micro = [10_000_000; 3];
        renderer.install_water(context.device(), body).unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
        frame.height_fog = Some(
            worldrunner_render_lighting::atmosphere::HeightFog::new(
                100.0,
                0.0,
                0.0,
                0.0,
                glam::Vec3::splat(0.25),
            )
            .unwrap(),
        );
        encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5)
            .finish()
            .submit()
            .unwrap();
        let output = read(&context, renderer.targets().texture());
        let center = (16 * 32 + 16) * 8;
        // Opaque absorption leaves the fog colour 0.25 (mix of f32 values stored as half).
        crate::tolerance_tests::assert_half_pixel(
            &output[center..center + 8],
            [0.25, 0.25, 0.25, 1.0],
            &format!("samples={samples} water fog"),
        );
        let (normal, material, specular) = renderer.water_receivers().unwrap();
        for view in [normal, material, specular] {
            assert_eq!(view.texture().sample_count(), samples);
            assert_eq!(view.texture().format(), wgpu::TextureFormat::Rgba16Float);
        }
        renderer.resize(context.device(), [16, 16]).unwrap();
        assert!(renderer.water_receivers().is_none());
    }
}

#[test]
fn water_shader_uses_shared_lighting_and_sample_receiver_outputs() {
    let source = include_str!("water.wgsl");
    for required in [
        "rd_toward_light",
        "scene_diffuse_environment",
        "scene_surface_transmission",
        "textureSampleLevel",
        "@builtin(sample_index)",
        "@location(3)",
    ] {
        assert!(source.contains(required), "missing {required}");
    }
}

#[test]
fn sh9_diffuse_reaches_water_scattering_at_every_supported_sample_count() {
    use worldrunner_render_plan::{SceneDiffuseEnvironment, SceneSh9};
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut coefficients = [[0; 3]; 9];
    coefficients[0] = [250_000, 500_000, -250_000];
    let counts = crate::msaa_tests::supported_samples(&context);
    for (samples, selected) in [false, true]
        .into_iter()
        .flat_map(|selected| counts.iter().map(move |&samples| (samples, selected)))
    {
        let mut renderer = regressions::renderer(&context, samples);
        let mut body = settings();
        body.color_micro = [500_000; 3];
        body.absorption_micro = [10_000_000; 3];
        renderer.install_water(context.device(), body).unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
        if selected {
            frame.unlit = false;
            frame.sun_direction = glam::Vec3::Z;
            frame.specular_convention = worldrunner_render_plan::SceneSpecularConvention {
                roughness: worldrunner_render_plan::SceneRoughnessRemap::Perceptual04,
                visibility: worldrunner_render_plan::SceneSpecularVisibility::SchlickSmith,
                occlusion: worldrunner_render_plan::SceneSpecularOcclusion::RoughnessAware,
                ..Default::default()
            };
        }
        frame.diffuse_environment =
            SceneDiffuseEnvironment::Sh9(SceneSh9::new(coefficients).unwrap());
        encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5)
            .finish()
            .submit()
            .unwrap();
        let output = read(&context, renderer.targets().texture());
        let center = (16 * 32 + 16) * 8;
        for (channel, expected) in [0.125, 0.25, 0.0].into_iter().enumerate() {
            let offset = center + channel * 2;
            let actual = numerical::positive_half(&output[offset..offset + 2]);
            assert!(
                (actual - expected).abs() < 0.0005,
                "{samples}x, {channel}: {actual}"
            );
        }
    }
}
