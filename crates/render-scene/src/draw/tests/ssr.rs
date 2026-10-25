use super::*;
use crate::compositor::{FrameRenderer, encode_scene};
use worldrunner_render_plan::SceneSsr;
use worldrunner_render_post::{AttachmentView, FrameConfig, Readback, ResourceRef, ToneMap};

fn settings(enabled: bool) -> SceneSsr {
    SceneSsr {
        enabled,
        max_steps: 64,
        thickness_millimeters: 100,
        roughness_cutoff_micro: 600_000,
        resolution_scale_micro: 500_000,
    }
}
fn source(request: Option<SceneSsr>) -> RenderScene {
    let mut effects = SceneEffects::new(effect_parts()).unwrap();
    if let Some(request) = request {
        effects = effects.with_ssr(request).unwrap();
    }
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture(0xffff)],
            vec![material(SceneAlphaMode::Opaque, true)],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![quad(), card(None)],
            vec![
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                ),
                SceneObject::new(
                    identity(),
                    MeshHandle::from_raw(1),
                    MaterialHandle::from_raw(0),
                ),
            ],
        )
        .unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}
fn capture(context: &GpuContext, request: Option<SceneSsr>, samples: u32) -> Vec<u8> {
    let source = source(request);
    let view = SceneView::new(
        pose(),
        camera(),
        Viewport {
            width: 65,
            height: 49,
        },
    )
    .unwrap();
    let mut renderer = FrameRenderer::hdr(
        context,
        [65, 49],
        COLOR_FORMAT,
        samples,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let pipelines =
        ScenePipelines::with_samples(context.device(), wgpu::TextureFormat::Rgba16Float, samples)
            .unwrap();
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
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
    let mut encoder = context.submission().begin(context.device()).unwrap();
    encode_scene(
        &mut renderer,
        context.device(),
        &mut encoder,
        (&mut gpu, &pipelines),
        (&color, &depth),
        FrameUniform::new(&view, source.effects()).unwrap(),
    )
    .unwrap();
    renderer.targets().encode_resolve(&mut encoder);
    encoder.finish().submit().unwrap();
    Readback::new(
        context.device(),
        FrameConfig::new([65, 49], wgpu::TextureFormat::Rgba16Float, 1),
        1_000_000,
    )
    .unwrap()
    .read(
        context.device(),
        context.submission(),
        renderer.targets().texture(),
    )
    .unwrap()
}
#[test]
fn gpu_ssr_profile_nonempty_without_ssao_and_disabled_identity_at_1x_and_4x() {
    let context = context();
    for samples in [1, 4] {
        let baseline = capture(&context, None, samples);
        assert_eq!(baseline, capture(&context, Some(settings(false)), samples));
        let enabled = capture(&context, Some(settings(true)), samples);
        assert_eq!(baseline.len(), enabled.len());
        assert!(enabled.iter().any(|byte| *byte != 0));
    }
}
