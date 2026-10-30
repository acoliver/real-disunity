use super::scene_tests::{base_effects, quad, scene, view};
use crate::{
    compositor::FrameRenderer, device::GpuContext, draw::SceneGpu, renderer::ScenePipelines,
};
use worldrunner_render_plan::*;
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef, ToneMap};

fn source(reflections: bool, velocity: bool) -> RenderScene {
    let fixture = scene(0, false, false);
    let mut parts = crate::draw::tests::effect_parts();
    parts.sun.irradiance_micro = 0;
    parts.ambient.sky_irradiance_micro = 0;
    parts.ambient.ground_irradiance_micro = 0;
    parts.camera = *base_effects(false).camera();
    parts.anti_alias = if velocity {
        SceneAntiAlias::taa(900_000)
    } else {
        SceneAntiAlias::Disabled
    };
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_atmosphere(fixture.effects().unwrap().atmosphere().unwrap().clone())
        .unwrap()
        .with_ssr(SceneSsr {
            enabled: reflections,
            ..Default::default()
        })
        .unwrap();
    RenderScene::new(SceneParts {
        resources: fixture.resources().clone(),
        geometry: SceneGeometry::new(vec![quad()], vec![]).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

fn read(context: &GpuContext, texture: &wgpu::Texture) -> Vec<u8> {
    Readback::new(
        context.device(),
        FrameConfig::new([9, 3], texture.format(), 1),
        1_000_000,
    )
    .unwrap()
    .read(context.device(), context.submission(), texture)
    .unwrap()
}

fn render(reflections: bool, velocity: bool) -> Vec<u8> {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let device = context.device();
    let source = source(reflections, velocity);
    let mut renderer = FrameRenderer::hdr(
        &context,
        [9, 3],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let upload = ScenePipelines::new(device, wgpu::TextureFormat::Rgba16Float);
    let selected = if velocity {
        ScenePipelines::with_taa_velocity(device, wgpu::TextureFormat::Rgba16Float, 1).unwrap()
    } else {
        upload.clone()
    };
    let mut gpu = SceneGpu::upload(device, context.submission(), &source, &upload).unwrap();
    let color = AttachmentView::new(
        ResourceRef::new("celestial SSR color"),
        renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("celestial SSR depth"),
        renderer.targets().depth().texture(),
    )
    .unwrap();
    let frame = crate::frame::FrameUniform::new(&view(), source.effects()).unwrap();
    context
        .submission()
        .prepare(device, |encoder, _| {
            crate::compositor::encode_scene(
                &mut renderer,
                device,
                encoder,
                (&mut gpu, &selected),
                (&color, &depth),
                frame,
            )
        })
        .unwrap()
        .submit()
        .unwrap();
    assert_metadata(&context, &renderer, reflections, velocity);
    read(&context, color.texture())
}

fn assert_metadata(
    context: &GpuContext,
    renderer: &FrameRenderer,
    reflections: bool,
    velocity: bool,
) {
    if reflections {
        let ssr = renderer.ssr.as_ref().expect("enabled SSR resources");
        assert!(
            read(context, ssr.confidence_target())
                .iter()
                .all(|b| *b == 0)
        );
    }
    if velocity {
        assert!(
            read(context, renderer.targets().velocity_texture().unwrap())
                .iter()
                .all(|b| *b == 0)
        );
    }
}

fn combined(velocity: bool) {
    let baseline = render(false, velocity);
    assert_eq!(render(true, velocity), baseline);
    let center = &baseline[(9 + 4) * 8..(9 + 5) * 8];
    assert_eq!(
        center,
        [0, 0, 0, 0x3c, 0, 0, 0, 0x3c],
        "green celestial mesh"
    );
    assert_eq!(&baseline[..8], [0, 0, 0, 0, 0, 0x40, 0, 0x3c], "blue sky");
}

#[test]
fn celestial_sky_ssr_without_velocity() {
    combined(false);
}

#[test]
fn celestial_sky_ssr_with_velocity() {
    combined(true);
}
