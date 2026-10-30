use super::tests::{context, read};
use crate::{compositor::FrameRenderer, draw::SceneGpu, renderer::ScenePipelines};
use worldrunner_render_plan::*;
use worldrunner_render_post::{AttachmentView, FrameError, ResourceRef, ToneMap};

fn scene(reflections: Option<bool>, velocity: bool) -> RenderScene {
    let mut parts = crate::draw::tests::effect_parts();
    parts.anti_alias = if velocity {
        SceneAntiAlias::taa(900_000)
    } else {
        SceneAntiAlias::Disabled
    };
    let mut effects = SceneEffects::new(parts)
        .unwrap()
        .with_atmosphere(crate::sky::render_tests::settings())
        .unwrap();
    if let Some(enabled) = reflections {
        effects = effects
            .with_ssr(SceneSsr {
                enabled,
                ..Default::default()
            })
            .unwrap();
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(vec![], vec![]).unwrap(),
        geometry: SceneGeometry::new(vec![], vec![]).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

fn render(reflections: Option<bool>, velocity: bool, capture: bool) -> Vec<u8> {
    let context = context();
    let device = context.device();
    let scene = scene(reflections, velocity);
    let mut renderer = FrameRenderer::hdr(
        &context,
        [1, 1],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        ToneMap::Reinhard,
        scene.effects(),
    )
    .unwrap();
    let pipelines = if velocity {
        ScenePipelines::with_taa_velocity(device, wgpu::TextureFormat::Rgba16Float, 1).unwrap()
    } else {
        ScenePipelines::new(device, wgpu::TextureFormat::Rgba16Float)
    };
    let mut gpu = SceneGpu::upload(device, context.submission(), &scene, &pipelines).unwrap();
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
    let frame = crate::sky::render_tests::test_frame(1.0);
    context
        .submission()
        .prepare(device, |encoder, _| {
            crate::compositor::encode_scene(
                &mut renderer,
                device,
                encoder,
                (&mut gpu, &pipelines),
                (&color, &depth),
                frame,
            )?;
            encode_output(device, &renderer, encoder, capture);
            Ok::<_, FrameError>(())
        })
        .unwrap()
        .submit()
        .unwrap();
    if reflections == Some(true) {
        assert!(renderer.ssr.is_some(), "enabled SSR resources");
    }
    assert_metadata(&context, &renderer, velocity);
    read(&context, color.texture())
}

fn encode_output(
    device: &wgpu::Device,
    renderer: &FrameRenderer,
    encoder: &mut wgpu::CommandEncoder,
    capture: bool,
) {
    if capture {
        let output = worldrunner_render_post::color_texture(
            device,
            "sky capture",
            [1, 1],
            wgpu::TextureFormat::Rgba8UnormSrgb,
        );
        renderer
            .encode_output(
                encoder,
                &output.create_view(&wgpu::TextureViewDescriptor::default()),
            )
            .unwrap();
    }
}

fn assert_metadata(context: &crate::device::GpuContext, renderer: &FrameRenderer, velocity: bool) {
    if let Some(ssr) = &renderer.ssr {
        for texture in &ssr.forward_products {
            assert_eq!(
                read(context, texture),
                vec![0, 0, 0, 0, 0, 0, 0, 0x3c],
                "sky SSR metadata"
            );
        }
        assert!(
            read(context, ssr.confidence_target())
                .iter()
                .all(|b| *b == 0)
        );
    }
    if velocity {
        assert_eq!(
            read(context, renderer.targets().velocity_texture().unwrap()),
            vec![0; 4]
        );
    }
}

#[test]
fn sky_ssr_combined_frame() {
    for velocity in [false, true] {
        for capture in [false, true] {
            let baseline = render(None, velocity, capture);
            assert_eq!(render(Some(false), velocity, capture), baseline);
            assert_eq!(render(Some(true), velocity, capture), baseline);
            assert_eq!(baseline, [0, 0x40, 0, 0x40, 0, 0x40, 0, 0x3c]);
        }
    }
}
