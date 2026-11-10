use super::*;
use crate::compositor::FrameRenderer;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};
use worldrunner_render_post::ToneMap;

const SIZE: [u32; 2] = [1512, 807];

fn source() -> RenderScene {
    let base = scene(&[[0, 0, 1000]], 600_000, None);
    let mut parts = fixture::effects_parts(600_000);
    parts.ssao = Some(crate::ssr::neutral_ao());
    parts.water = Some(SceneWaterBody {
        east_min_millimeters: -3000,
        north_min_millimeters: -3000,
        east_extent_millimeters: 3000,
        north_extent_millimeters: 6000,
        roughness_micro: 100_000,
        height_millimeters: 100,
        ..super::super::unsupported_water_body()
    });
    let effects = SceneEffects::new(parts)
        .unwrap()
        .with_ssr(SceneSsr {
            enabled: true,
            traversal: SceneSsrTraversal::Grid,
            grid_domain: SceneSsrGridDomain::ReflectionOutput,
            grid_level_count: Some(5),
            depth_relative_ao: true,
            roughness_cutoff_disabled: true,
            roughness_cutoff_micro: 1_000_000,
            resolution_scale_micro: 500_000,
            radiance_rounding: SceneDownsampleRounding::Floor,
            radiance_pyramid: SceneSsrPyramid::Full,
            application_lod: SceneSsrLodMapping::RoughnessDownsample,
            composition: SceneSsrComposition::ReplaceEnvironment,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(effects.ssr().unwrap().radiance_extents(SIZE).len(), 10);
    RenderScene::new(SceneParts {
        resources: base.resources().clone(),
        geometry: base.geometry().clone(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

fn full_view(source: &RenderScene) -> SceneView {
    SceneView::new(
        crate::CameraPose {
            eye_millimeters: [0, -4000, 6500],
            target_millimeters: [0; 3],
        },
        *source.effects().unwrap().camera(),
        crate::Viewport {
            width: SIZE[0],
            height: SIZE[1],
        },
    )
    .unwrap()
}

fn renderers(context: &GpuContext, source: &RenderScene) -> (FrameRenderer, ShadowedScene) {
    let frame = FrameRenderer::hdr(
        context,
        SIZE,
        OffscreenTarget::COLOR_FORMAT,
        1,
        ToneMap::Reinhard,
        source.effects(),
    )
    .unwrap();
    let mut renderer = ShadowedScene::with_target(
        context,
        source,
        settings(),
        wgpu::TextureFormat::Rgba16Float,
        1,
    )
    .unwrap();
    renderer
        .set_depth_fitting(
            context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::Interactive {
                    latency_frames: 2,
                    contraction_frames: 0,
                },
                range: policy(),
            }),
        )
        .unwrap();
    (frame, renderer)
}

#[test]
fn gpu_sdsm_full_radiance_water_ssr_preserves_reductions_across_frames() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let source = source();
    let view = full_view(&source);
    let (mut frame, mut renderer) = renderers(&context, &source);
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        frame.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        frame.targets().depth().texture(),
    )
    .unwrap();
    for source_frame in 0..6 {
        let fit = renderer
            .render_depth_fitted_with(&context, &view, (&color, &depth), source_frame, &mut frame)
            .unwrap();
        context
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .unwrap();
        if source_frame >= 2 {
            let fit = fit.expect("completed SDSM evidence must be adopted");
            assert_eq!(fit.source_frame, source_frame - 2);
            assert!(
                matches!(fit.depth, DepthRange::Receivers { near, far } if near > 0.0 && far > near)
            );
        }
    }
}
