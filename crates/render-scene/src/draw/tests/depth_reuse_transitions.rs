//! Relief frames keep independent metadata, then restore sharing on a later ordinary frame.
use super::*;
use crate::draw::PreparedScene;
use crate::shadows::ShadowedTarget;
use crate::test_support::upload_scene;

fn scenes() -> [RenderScene; 3] {
    let ordinary = source();
    let relief = impostor::relief_scene();
    let relief = relief
        .with_objects_and_effects(
            relief.geometry().objects().to_vec(),
            ordinary.effects().cloned(),
        )
        .unwrap();
    [ordinary.clone(), relief, ordinary]
}

fn sequence(context: &GpuContext, independent: bool, shadows: bool) -> Vec<Vec<Vec<u8>>> {
    let sources = scenes();
    let device = context.device();
    let pipelines =
        ScenePipelines::with_ssao_taa_velocity(device, TextureFormat::Rgba16Float, 1).unwrap();
    let mut renderer = FrameRenderer::hdr(
        context,
        SIZE,
        COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        sources[0].effects(),
    )
    .unwrap();
    if independent {
        renderer.independent_depth_for_test(device);
    }
    let depth = renderer.targets().depth().texture().clone();
    sources
        .iter()
        .zip([0.0, 0.125, 0.25])
        .enumerate()
        .map(|(index, (source, time))| {
            let mut gpu = upload_scene(device, context.submission(), source, &pipelines).unwrap();
            stage_quad(context, &mut gpu, &pipelines);
            assert_eq!(gpu.has_relief(), index == 1);
            let mut shadowed = shadows.then(|| {
                crate::shadows::ShadowedScene::new(
                    context,
                    PreparedScene::new(source).unwrap(),
                    shadow_settings(),
                    ShadowedTarget {
                        velocity: true,
                        ..ShadowedTarget::new(TextureFormat::Rgba16Float, 1)
                    },
                )
                .unwrap()
            });
            // Skip the helper's resize frames so reallocation cannot hide a missing restore.
            let products = draw_frame(
                context,
                &mut renderer,
                (&mut gpu, &pipelines),
                shadowed.as_mut(),
                source,
                index + 4,
                time,
            );
            assert_eq!(renderer.targets().depth().texture(), &depth);
            assert_eq!(
                renderer.ssao.as_ref().unwrap().depth == *renderer.targets().depth().texture(),
                !independent && (shadows || index != 1),
                "relief sharing requires a paired receiver payload; ordinary frames restore sharing"
            );
            products
        })
        .collect()
}

#[test]
fn gpu_relief_frame_preserves_products_and_restores_depth_reuse() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    for shadows in [false, true] {
        assert_eq!(
            sequence(&context, true, shadows),
            sequence(&context, false, shadows),
            "shadows={shadows}"
        );
    }
}

#[test]
fn gpu_relief_ordinary_relief_retains_depth_without_draining_or_rebudgeting() {
    use crate::compositor::{ConfiguredFrameRenderer, RendererConfiguration};
    use worldrunner_render_gpu::configuration::AllocationBudget;
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let device = context.device();
    let [ordinary, relief, _] = scenes();
    let pipelines =
        ScenePipelines::with_ssao_taa_velocity(device, TextureFormat::Rgba16Float, 1).unwrap();
    let mut settings = RendererConfiguration::new(SIZE, COLOR_FORMAT, ToneMap::Reinhard);
    settings.frame.format = TextureFormat::Rgba16Float;
    let effects = ordinary.effects().unwrap();
    settings.ssao = effects.ssao().copied();
    settings.ssr = effects.ssr().copied();
    settings.water = effects.water_bodies().to_vec();
    settings.post = crate::post::settings(effects).unwrap();
    let mut configured =
        ConfiguredFrameRenderer::new(&context, settings, AllocationBudget(64 * 1024 * 1024))
            .unwrap();
    let before = (
        configured.generation(),
        configured.budget(),
        configured.resident_bytes(&device.limits()).unwrap(),
    );
    let depth = configured
        .renderer_mut()
        .ssao
        .as_ref()
        .unwrap()
        .independent_depth
        .clone();
    let mut scenes = [&relief, &ordinary, &relief].map(|source| {
        (
            source,
            upload_scene(device, context.submission(), source, &pipelines).unwrap(),
        )
    });
    for (slot, (source, gpu)) in scenes.iter_mut().enumerate() {
        let renderer = configured.renderer_mut();
        renderer.select_frame_slot(slot);
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            renderer.targets().scene().texture(),
        )
        .unwrap();
        let scene_depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            renderer.targets().depth().texture(),
        )
        .unwrap();
        let view = SceneView::new(
            pose(),
            camera(),
            Viewport {
                width: SIZE[0],
                height: SIZE[1],
            },
        )
        .unwrap();
        let frame = FrameUniform::new(&view, source.effects()).unwrap();
        let mut encoder = context.submission().begin(device).unwrap();
        renderer
            .encoding()
            .encode_scene(
                device,
                &mut encoder,
                (gpu, &pipelines),
                (&color, &scene_depth),
                frame,
            )
            .unwrap();
        encoder.finish().submit().unwrap();
        let screen = renderer.ssao.as_ref().unwrap();
        assert_eq!(screen.independent_depth, depth);
        assert_eq!(screen.depth == depth, slot != 1);
        assert_eq!(
            (
                configured.generation(),
                configured.budget(),
                configured.resident_bytes(&device.limits()).unwrap()
            ),
            before
        );
    }
}
