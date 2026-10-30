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
    let context = pollster::block_on(GpuContext::request()).unwrap();
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
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let water = Water::new(
        context.device(),
        FrameConfig::new([32, 32], wgpu::TextureFormat::Rgba16Float, 1),
        vec![settings()],
    )
    .unwrap();
    let a = water.feature(context.device(), &prepared_frame(false), (0.0, 0), None);
    let b = water.feature(context.device(), &prepared_frame(true), (0.0, 0), None);
    let again = water.feature(context.device(), &prepared_frame(true), (0.0, 0), None);
    assert_ne!(a.water.pipeline, b.water.pipeline);
    assert_ne!(a.water.shadow_pipeline, b.water.shadow_pipeline);
    assert_eq!(b.water.pipeline, again.water.pipeline);
    assert_eq!(water.specializations.borrow().len(), 2);
    let mut frame = prepared_frame(false);
    frame.specular_convention = SceneSpecularConvention::default();
    let default = water.feature(context.device(), &frame, (0.0, 0), None);
    assert_eq!(default.water.pipeline, water.pipeline);
    assert_eq!(default.water.shadow_pipeline, water.shadow_pipeline);
    frame.specular_convention.surface_disk =
        Some(worldrunner_render_plan::SceneSurfaceDisk::new(2_000_000, 3_000_000).unwrap());
    let scene_disk = water.feature(context.device(), &frame, (0.0, 0), None);
    assert_eq!(scene_disk.water.pipeline, water.pipeline);
    assert_eq!(scene_disk.water.shadow_pipeline, water.shadow_pipeline);
}
