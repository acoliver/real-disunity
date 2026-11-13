use super::*;
use worldrunner_render_plan::{
    SceneDirectDiffuse, SceneRoughnessRemap, SceneSpecularConvention, SceneWaterSunDisk,
};

fn prepared_frame(selected: bool) -> FrameUniform {
    let mut frame = FrameUniform::new(&fixture_view(), None).unwrap();
    frame.unlit = false;
    frame.sun_direction = glam::Vec3::Z;
    frame.sun_irradiance = glam::Vec3::new(0.5, 1.0, 2.0);
    frame.ambient_sky = glam::Vec3::ZERO;
    frame.ambient_ground = glam::Vec3::ZERO;
    frame.specular_convention = SceneSpecularConvention {
        roughness: SceneRoughnessRemap::Perceptual04,
        direct_diffuse: if selected {
            SceneDirectDiffuse::LambertianPrepared
        } else {
            SceneDirectDiffuse::EnergyConserving
        },
        ..Default::default()
    };
    frame
}

#[test]
fn prepared_sun_water_diffuse_matches_independent_cpu() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let mut renderer = regressions::renderer(&context, 1);
    let mut body = settings();
    body.color_micro = [250_000, 500_000, 750_000];
    body.material.direct_surface = true;
    body.material.lighting_normal_micro = Some(0);
    body.material.metallic_micro = Some(100_000);
    body.material.sun_disk = Some(SceneWaterSunDisk {
        amplitude_micro: 0,
        ..Default::default()
    });
    renderer.install_water(&context, body).unwrap();
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
    for roughness in [
        SceneRoughnessRemap::Perceptual04,
        SceneRoughnessRemap::Perceptual045,
    ] {
        let mut frame = prepared_frame(true);
        frame.specular_convention.roughness = roughness;
        encode_fixture(&context, &mut renderer, (&color, &depth), &frame, 0.5)
            .finish()
            .submit()
            .unwrap();
        let bytes = read(&context, renderer.targets().texture());
        for (channel, (albedo, sun)) in [(0.25f64, 0.5), (0.5, 1.0), (0.75, 2.0)]
            .into_iter()
            .enumerate()
        {
            let expected = albedo * 0.9 * sun;
            let offset = (16 * 32 + 16) * 8 + channel * 2;
            let actual = f64::from(numerical::positive_half(&bytes[offset..offset + 2]));
            // One float16 store step plus 32 rounded float32 operations.
            let bound = expected * (1.0 / 1024.0 + 32.0 * f64::from(f32::EPSILON));
            assert!(
                (actual - expected).abs() <= bound,
                "{actual} vs {expected}, bound {bound}"
            );
        }
    }
}

#[test]
fn water_diffuse_conventions_have_distinct_reused_pipelines() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = GpuContext::shared_for_test().unwrap();
    let water = Water::new(
        context.device(),
        context.submission(),
        FrameConfig::new([32, 32], wgpu::TextureFormat::Rgba16Float, 1),
        vec![settings()],
    )
    .unwrap();
    // The fixture clears its shadow map in a submission of its own, before the frame opens.
    let shadow = crate::dynamic::capture_tests::shadow_fixture::groups(&context);
    let encoder = context.submission().begin(context.device()).unwrap();
    let gpu = (context.device(), &*encoder);
    let built = || water.programs.borrow().built();
    let a = water.feature(gpu, &prepared_frame(false), (0.0, 0), (None, None));
    let b = water.feature(gpu, &prepared_frame(true), (0.0, 0), (None, None));
    assert_eq!(
        built(),
        2,
        "the two diffuse conventions draw with distinct programs"
    );
    let again = water.feature(gpu, &prepared_frame(true), (0.0, 0), (None, None));
    assert_eq!(built(), 2, "a convention drawn again reuses its program");
    let mut frame = prepared_frame(false);
    frame.specular_convention = SceneSpecularConvention::default();
    let default = water.feature(gpu, &frame, (0.0, 0), (None, None));
    assert_eq!(
        built(),
        3,
        "the default convention has a program of its own"
    );
    frame.specular_convention.surface_disk =
        Some(worldrunner_render_plan::SceneSurfaceDisk::new(2_000_000, 3_000_000).unwrap());
    let scene_disk = water.feature(gpu, &frame, (0.0, 0), (None, None));
    assert_eq!(
        built(),
        3,
        "a scene disk shares the default convention's program"
    );
    let shadowed = Some(&shadow[0]);
    let a_shadowed = water.feature(gpu, &prepared_frame(false), (0.0, 0), (shadowed, None));
    assert_eq!(
        built(),
        4,
        "a shadowed convention draws with a program of its own"
    );
    let a_again = water.feature(gpu, &prepared_frame(false), (0.0, 0), (shadowed, None));
    assert_eq!(
        built(),
        4,
        "a shadowed convention drawn again reuses its program"
    );
    let b_shadowed = water.feature(gpu, &prepared_frame(true), (0.0, 0), (shadowed, None));
    assert_eq!(
        built(),
        5,
        "the two diffuse conventions draw with distinct shadowed programs"
    );
    drop((a, b, again, default, scene_disk));
    drop((a_shadowed, a_again, b_shadowed));
}
