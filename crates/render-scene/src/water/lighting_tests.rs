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
fn water_fog_composes_only_the_camera_segment_at_one_and_four_samples() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    for samples in [1, 4] {
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
        encode_fixture(&context, &mut renderer, (&color, &depth), frame, 0.5)
            .finish()
            .submit()
            .unwrap();
        let output = read(&context, renderer.targets().texture());
        let center = (16 * 32 + 16) * 8;
        assert_eq!(
            &output[center..center + 8],
            &[0, 0x34, 0, 0x34, 0, 0x34, 0, 0x3c]
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
        "rd_hemisphere",
        "rd_height_fog_transmittance",
        "textureSampleLevel",
        "@builtin(sample_index)",
        "@location(3)",
    ] {
        assert!(source.contains(required), "missing {required}");
    }
}
