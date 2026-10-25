mod fixes;
use super::*;
use crate::shadows::sdsm::{AdoptionPolicy, SdsmSettings};

#[test]
fn gpu_sdsm_capture_rejects_changed_layer_count_before_encoding() {
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let configured = super::super::cascades::source_with(
        &source,
        SceneShadowCascades::new(vec![1_000, 2_000], 100_000).unwrap(),
    );
    renderer.set_effects(configured.effects().unwrap()).unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let serial = context.submission().submitted_serial();
    let error = renderer
        .render_depth_fitted(
            &context,
            &view(),
            (&target.color_attachment(), &target.depth_attachment()),
            0,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        ShadowError::DepthFitting(crate::shadows::sdsm::SdsmError::CascadeLayers {
            allocated: 1,
            requested: 2,
        })
    ));
    assert_eq!(context.submission().submitted_serial(), serial);
    assert_eq!(renderer.map.depth.layers(), 1);
    renderer.set_effects(source.effects().unwrap()).unwrap();
    assert!(
        renderer
            .render_depth_fitted(
                &context,
                &view(),
                (&target.color_attachment(), &target.depth_attachment()),
                0,
            )
            .unwrap()
            .is_some()
    );
    renderer.set_depth_fitting(&context, None).unwrap();
    renderer.set_effects(configured.effects().unwrap()).unwrap();
    render_target(&mut renderer, &context, &view(), &target).unwrap();
    assert_eq!(renderer.map.depth.layers(), 2);
}

#[test]
fn gpu_sdsm_capture_includes_cold_and_invalidated_terrain_pages() {
    let context = context();
    let source = super::super::terrain::terrain_scene(&scene(&[], 600_000, None), false, false);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    enable_pages(&context, &mut renderer, 5);
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let mut target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    let mut outputs = Vec::new();
    for frame in 0..2 {
        if frame == 1 {
            renderer.gpu.invalidate_terrain_pages([0.0; 2], [1.0; 2]);
        }
        let fit = renderer
            .render_depth_fitted(
                &context,
                &view(),
                (&target.color_attachment(), &target.depth_attachment()),
                frame,
            )
            .unwrap()
            .unwrap();
        assert!(
            matches!(fit.depth, DepthRange::Receivers { near, far } if near > 0.1 && far < 100.0)
        );
        assert!(renderer.gpu.terrain_pages_ready());
        let actual = target.read_rgba8(&context).unwrap();
        assert_fitted_pixels(&mut renderer, &context, &view(), &mut target, fit, &actual);
        outputs.push(actual);
    }
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn gpu_sdsm_capture_reuses_page_uploads_across_segments() {
    let context = context();
    let source = super::super::terrain::terrain_scene(&scene(&[], 600_000, None), false, false);
    let mut renderer = ShadowedScene::new(&context, &source, settings()).unwrap();
    enable_pages(&context, &mut renderer, 1);
    renderer
        .set_depth_fitting(
            &context,
            Some(SdsmSettings {
                policy: AdoptionPolicy::SameFrameCapture,
                range: policy(),
            }),
        )
        .unwrap();
    let target = OffscreenTarget::new(context.device(), view().viewport()).unwrap();
    for frame in 0..3 {
        let fit = renderer
            .render_depth_fitted(
                &context,
                &view(),
                (&target.color_attachment(), &target.depth_attachment()),
                frame,
            )
            .unwrap()
            .unwrap();
        assert!(matches!(fit.depth, DepthRange::Receivers { .. }));
        assert_eq!(renderer.gpu.terrain_pages_ready(), frame == 2);
    }
}

fn enable_pages(context: &GpuContext, renderer: &mut ShadowedScene, upload_pages: u64) {
    use crate::terrain::pages::{composition::LayerSample, scene::TerrainPageSource};
    use worldrunner_render_plan::{TerrainPageSettings, TerrainPageSettingsParts};
    let page_settings = TerrainPageSettings::new(TerrainPageSettingsParts {
        interior: [3, 5],
        border: 1,
        filter_radius: 1,
        maximum_depth: 1,
        split_threshold_millipixels: 2000,
        merge_threshold_millipixels: 1000,
        maximum_selected_pages: 4,
        resident_bytes: 100_000,
        upload_bytes_per_frame: 336 * upload_pages,
    })
    .unwrap();
    renderer
        .gpu
        .enable_terrain_pages(
            context.device(),
            context.submission(),
            &renderer.pipelines,
            TerrainPageSource {
                settings: page_settings,
                slots: 5,
                budget: worldrunner_render_gpu::configuration::AllocationBudget(100_000),
                sample: Box::new(|_| {
                    vec![LayerSample {
                        color: [180, 140, 100, 255],
                        normal: [0, 0, i16::MAX],
                        roughness: 180,
                        weight: 1,
                    }]
                }),
            },
        )
        .unwrap();
}
