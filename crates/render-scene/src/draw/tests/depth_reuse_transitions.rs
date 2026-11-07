//! Relief frames keep independent metadata, then restore sharing on a later ordinary frame.
use super::*;

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
    sources
        .iter()
        .zip([0.0, 0.125, 0.25])
        .enumerate()
        .map(|(index, (source, time))| {
            let mut gpu =
                SceneGpu::upload(device, context.submission(), source, &pipelines).unwrap();
            stage_quad(context, &mut gpu, &pipelines);
            assert_eq!(gpu.has_relief(), index == 1);
            let mut shadowed = shadows.then(|| {
                crate::shadows::ShadowedScene::with_taa_velocity(
                    context,
                    source,
                    shadow_settings(),
                    TextureFormat::Rgba16Float,
                    1,
                )
                .unwrap()
            });
            let products = draw_frame(
                context,
                &mut renderer,
                (&mut gpu, &pipelines),
                shadowed.as_mut(),
                source,
                index,
                time,
            );
            assert_eq!(
                renderer.ssao.as_ref().unwrap().depth == *renderer.targets().depth().texture(),
                !independent && index != 1,
                "relief must keep separate metadata and ordinary frames must restore sharing"
            );
            products
        })
        .collect()
}

#[test]
fn gpu_relief_frame_preserves_products_and_restores_depth_reuse() {
    let context = context();
    for shadows in [false, true] {
        assert_eq!(
            sequence(&context, true, shadows),
            sequence(&context, false, shadows),
            "shadows={shadows}"
        );
    }
}
